use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Barrier, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use glam::DVec3;
use rand::rngs::StdRng;

use crate::astronomy::AstronomicalObject;
use crate::integration::{self, IntegrationMethod, G};

type WorkResult = Result<Vec<DVec3>, (usize, usize)>;

struct WorkItem {
    start: (usize, usize),
    end: (usize, usize),
}

struct WorkerControl {
    thread_results: Vec<Arc<Mutex<WorkResult>>>,
    work_queue: Arc<RwLock<Vec<WorkItem>>>,
    barrier: Arc<Barrier>,
    worker_kill: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct SimulatorControl {
    pub target_speed: f64,
    pub is_running: bool, // For outside communication
    pub method: IntegrationMethod,
    pub num_threads: usize,
    pub iteration_speed: f64,
    pub time_step: f64,
    pub use_target_speed: bool,
    pub time_elapsed: f64,
}

pub struct Engine {
    pub objects: Arc<Mutex<Vec<AstronomicalObject>>>,
    pub framerate: Arc<Mutex<u32>>,
    pub params: Arc<Mutex<SimulatorControl>>,
    thread_stopped: Arc<Mutex<bool>>,
    // Thread communicates that it has cleanly ended
}

impl Engine {
    pub fn start_mt(&self) {
        let mut stopped = self.thread_stopped.lock().unwrap();
        let objects_local = Arc::new(RwLock::new(self.objects.lock().unwrap().clone()));

        if !*stopped || objects_local.read().unwrap().len() < 2 {
            return;
        }

        *stopped = false;

        {
            let mut params = self.params.lock().unwrap();
            params.is_running = true;
        }

        let objects_shared = self.objects.clone();
        let params_lock = self.params.clone();
        let stopped_lock = self.thread_stopped.clone();

        let framerate = *self.framerate.lock().unwrap() as f64;

        thread::spawn(move || {
            let mut params_local = params_lock.lock().unwrap().clone();

            let mut time_step = if params_local.use_target_speed {
                0.001f64
            } else {
                params_local.time_step
            };

            let mut time_running = params_local.time_elapsed;
            let mut time_step_counter: u128 = 0;

            let mut i = 0;
            let mut steps_until_update = 1000u128;

            let mut use_symplectic = match params_local.method {
                IntegrationMethod::Symplectic(_) => true,
                IntegrationMethod::RK4 => false,
            };

            // Prepare threads if needed
            let mut state = Engine::prepare_worker_threads(
                params_local.num_threads,
                objects_local.read().unwrap().len(),
            );
            let mut handles = vec![];

            if use_symplectic && params_local.num_threads > 1 {
                handles = Engine::start_worker_threads(&state, &objects_local);
            }

            let mut time_now = Instant::now();
            loop {
                if params_local.num_threads == 1 || !use_symplectic {
                    let mut objects_local = objects_local.write().unwrap();
                    if use_symplectic {
                        let coefficient_table = params_local.method.get_coefficients();
                        'outer_integration_loop: while i < steps_until_update {
                            for (c, d) in coefficient_table.iter() {
                                objects_local.iter_mut().for_each(|x| {
                                    x.position += time_step * c * x.velocity;
                                });

                                // This check speeds up 4th order symplectic integration significantly
                                if *d != 0.0 {
                                    loop {
                                        match integration::symplectic(&objects_local) {
                                            Ok(res) => {
                                                for (body, vector) in
                                                    objects_local.iter_mut().zip(res)
                                                {
                                                    body.velocity += time_step * d * vector;
                                                    body.acceleration = vector;
                                                }

                                                break;
                                            }
                                            Err(indices) => {
                                                println!(
                                                    "New event at {:.2} y:",
                                                    (time_running
                                                        + time_step_counter as f64 * time_step)
                                                        / (3600.0 * 24.0 * 365.0)
                                                );
                                                integration::collide_objects(
                                                    &mut objects_local,
                                                    &indices,
                                                );

                                                if objects_local.len() < 2 {
                                                    params_lock.lock().unwrap().is_running = false;
                                                    break 'outer_integration_loop;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            i += 1;
                            time_step_counter += 1;
                        }

                    // RK 4
                    } else {
                        while i < steps_until_update {
                            let collision =
                                integration::runge_kutta_4(&mut objects_local, time_step);

                            if let Some(indices) = collision {
                                println!(
                                    "New event at {:.2} y:",
                                    (time_running + time_step_counter as f64 * time_step)
                                        / (3600.0 * 24.0 * 365.0)
                                );
                                integration::collide_objects(&mut objects_local, &indices);
                                if objects_local.len() < 2 {
                                    params_lock.lock().unwrap().is_running = false;
                                    break;
                                }
                            }

                            i += 1;
                            time_step_counter += 1;
                        }
                    }
                } else {
                    let coefficient_table = params_local.method.get_coefficients();
                    'outer_integration_loop: while i < steps_until_update {
                        for (c, d) in coefficient_table.iter() {
                            if *c != 0.0 {
                                let mut objects = objects_local.write().unwrap();
                                objects.iter_mut().for_each(|x| {
                                    x.position += time_step * c * x.velocity;
                                });
                            }

                            if *d != 0.0 {
                                'integration_loop: loop {
                                    state.barrier.wait(); // Release worker threads to do work
                                    state.barrier.wait(); // Work is completed, now we can gather results

                                    let results = &state.thread_results;
                                    let mut objects = objects_local.write().unwrap();

                                    // These will hold the final acceleration values
                                    let mut acceleration_vectors = vec![DVec3::ZERO; objects.len()];

                                    for lock in results {
                                        let result = lock.lock().unwrap();

                                        match &*result {
                                            Ok(vectors) => {
                                                for (acc, res) in
                                                    acceleration_vectors.iter_mut().zip(vectors)
                                                {
                                                    *acc += *res;
                                                }
                                            }
                                            Err(collision) => {
                                                println!(
                                                    "New event at {:.2} y:",
                                                    (time_running
                                                        + time_step_counter as f64 * time_step)
                                                        / (3600.0 * 24.0 * 365.0)
                                                );
                                                integration::collide_objects(
                                                    &mut objects,
                                                    collision,
                                                );

                                                if objects.len() < 2 {
                                                    params_lock.lock().unwrap().is_running = false;
                                                    break 'outer_integration_loop;
                                                }

                                                let mut work_queue =
                                                    state.work_queue.write().unwrap();

                                                *work_queue = Engine::get_mt_splices(
                                                    objects.len(),
                                                    params_local.num_threads,
                                                );

                                                continue 'integration_loop;
                                            }
                                        }
                                    }

                                    // Step was successful (no collisions) so we can update state
                                    for (object, acc) in
                                        objects.iter_mut().zip(acceleration_vectors)
                                    {
                                        object.velocity += time_step * d * acc;
                                        object.acceleration = acc;
                                    }

                                    break;
                                }
                            }
                        }
                        i += 1;
                        time_step_counter += 1;
                    }
                }

                {
                    // Update state for UI
                    let objects = objects_local.read().unwrap();
                    let mut objects_shared = objects_shared.lock().unwrap();
                    objects_shared.clear();
                    objects.iter().for_each(|o| objects_shared.push(o.clone()));
                }

                let mut params = params_lock.lock().unwrap();

                let new_time = Instant::now();
                let duration = (new_time - time_now).as_nanos();

                let speed: f64 = if duration == 0 {
                    // This should double steps for next iteration until Duration can be measured
                    steps_until_update as f64 * 2.0 * framerate
                } else {
                    // n/s
                    steps_until_update as f64 * 1_000_000_000.0 / duration as f64
                };

                steps_until_update = (speed / framerate).round() as u128;

                // Limit next update to have at least 10 steps
                steps_until_update = steps_until_update.max(10);

                if params.use_target_speed {
                    let target_speed = params.target_speed;

                    time_running += time_step_counter as f64 * time_step;
                    time_step_counter = 0;

                    time_step = target_speed / speed;
                    params.time_step = time_step;
                } else {
                    if params.time_step != time_step {
                        time_running += time_step_counter as f64 * time_step;
                        time_step_counter = 0;
                        time_step = params.time_step;
                    }

                    params.target_speed = time_step * speed;
                }

                params.time_elapsed = time_step_counter as f64 * time_step + time_running;
                params.iteration_speed = speed;
                time_now = new_time;

                if !params.is_running
                    || params_local.num_threads != params.num_threads
                    || params_local.method != params.method
                {
                    if !handles.is_empty() {
                        state
                            .worker_kill
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        state.barrier.wait(); // Release threads so they see kill signal
                        handles.into_iter().for_each(|h| h.join().unwrap());
                    }

                    if !params.is_running {
                        *stopped_lock.lock().unwrap() = true;
                        break;
                    }

                    params_local = params.clone();

                    use_symplectic = match params_local.method {
                        IntegrationMethod::Symplectic(_) => true,
                        IntegrationMethod::RK4 => false,
                    };

                    // Prepare threads if needed
                    state = Engine::prepare_worker_threads(
                        params_local.num_threads,
                        objects_local.read().unwrap().len(),
                    );
                    handles = vec![];

                    if use_symplectic && params_local.num_threads > 1 {
                        handles = Engine::start_worker_threads(&state, &objects_local);
                    }
                }

                i = 0;
            }
        });
    }

    pub fn stop(&self) {
        self.params.lock().unwrap().is_running = false;
    }

    fn get_mt_splices(num_bodies: usize, num_threads: usize) -> Vec<WorkItem> {
        if num_bodies < 2 {
            return Vec::new();
        }

        let mut combinations = Vec::new();
        for i in 0..num_bodies - 1 {
            for j in i + 1..num_bodies {
                combinations.push((i, j));
            }
        }

        let len = combinations.len();
        let max_threads = len.min(num_threads);

        let mut buckets = Vec::new();
        for i in 0..max_threads {
            let mut num = if i < len % max_threads { 1 } else { 0 };
            num += len / max_threads;

            let bucket = combinations.split_off(combinations.len() - num);
            let (a, b) = bucket.first().unwrap();
            let (c, d) = bucket.last().unwrap();
            buckets.push(((*a, *b), (*c, *d)));
        }

        buckets
            .iter()
            .map(|x| WorkItem {
                start: x.0,
                end: x.1,
            })
            .collect()
    }

    fn prepare_worker_threads(num_threads: usize, num_objects: usize) -> WorkerControl {
        let mut thread_results = Vec::with_capacity(num_threads);
        for _ in 0..num_threads {
            thread_results.push(Arc::new(Mutex::new(Ok(Vec::new()))));
        }

        let work_queue: Arc<RwLock<Vec<WorkItem>>> =
            Arc::new(RwLock::new(Vec::with_capacity(num_threads)));
        *work_queue.write().unwrap() = Engine::get_mt_splices(num_objects, num_threads);

        let barrier = Arc::new(Barrier::new(num_threads + 1));
        let worker_kill = Arc::new(AtomicBool::new(false));

        WorkerControl {
            thread_results,
            work_queue,
            barrier,
            worker_kill,
        }
    }

    fn start_worker_threads(
        state: &WorkerControl,
        local_objects: &Arc<RwLock<Vec<AstronomicalObject>>>,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = vec![];
        for (i, thread_result) in state.thread_results.iter().enumerate() {
            let i_thread = i;
            let work_queue_lock = state.work_queue.clone();
            let objects_lock = local_objects.clone();
            let barrier_lock = state.barrier.clone();
            let result_lock = thread_result.clone();
            let kill_lock = state.worker_kill.clone();

            let handle = thread::spawn(move || {
                loop {
                    barrier_lock.wait(); // Wait until main thread has assigned work

                    if kill_lock.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }

                    let work_item_queue = work_queue_lock.read().unwrap();

                    if work_item_queue.len() <= i_thread {
                        drop(work_item_queue);
                        barrier_lock.wait();
                        continue;
                    }

                    let work_item = &work_item_queue[i_thread];
                    let objects = objects_lock.read().unwrap();

                    let integration_result =
                        integration::symplectic_mt(&objects, work_item.start, work_item.end);

                    *result_lock.lock().unwrap() = integration_result;

                    drop(work_item_queue);
                    barrier_lock.wait(); // Important to have two barrier waits. Main thread prepares work between
                }
            });

            handles.push(handle);
        }

        handles
    }

    pub fn find_orbital_parent<'a>(
        child: &'a AstronomicalObject,
        objects: &'a [AstronomicalObject],
    ) -> Option<&'a AstronomicalObject> {
        let allowed_error = 0.1f64;
        if child.acceleration.length() == 0.0 {
            return None;
        }

        let child_acc_unit = child.acceleration.clamp_length(1.0, 1.0);

        for other in objects {
            if std::ptr::eq(child, other) {
                continue;
            }

            let difference = other.position - child.position;
            let scalar = difference.dot(child_acc_unit);

            if 1.0 - scalar / difference.length() >= allowed_error {
                continue;
            }

            let acc_caused_by_other = G * other.mass / difference.length_squared();
            if 1.0 - acc_caused_by_other / child.acceleration.length() >= allowed_error {
                continue;
            }

            return Some(other);
        }

        None
    }

    pub fn default(rng: &mut StdRng) -> Self {
        Engine {
            objects: Arc::new(Mutex::new(AstronomicalObject::default(rng))),
            framerate: Arc::new(Mutex::new(60)),
            params: Arc::new(Mutex::new(SimulatorControl {
                target_speed: 86400.0 * 1.0,
                is_running: false,
                method: IntegrationMethod::Symplectic(4),
                num_threads: 1,
                iteration_speed: 0.0,
                time_step: 0.01,
                use_target_speed: false,
                time_elapsed: 0.0,
            })),
            thread_stopped: Arc::new(Mutex::new(true)),
        }
    }
}
