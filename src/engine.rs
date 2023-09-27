use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Barrier, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::astronomy::AstronomicalObject;
use crate::integration::{self, IntegrationMethod};
use crate::vector::Vector3d;

type WorkResult = Result<Vec<Vector3d>, (usize, usize)>;

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

#[derive(Debug)]
pub struct SimulatorControl {
    pub target_speed: f64,
    pub is_running: bool, // For outside communication
    pub method: IntegrationMethod,
    pub num_threads: usize,
    pub iteration_speed: f64,
    pub time_step: f64,
    pub use_target_speed: bool,
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
        if !*stopped {
            return;
        }

        *stopped = false;

        {
            let mut params = self.params.lock().unwrap();
            params.is_running = true;
        }

        let objects_local = Arc::new(RwLock::new(self.objects.lock().unwrap().clone()));
        let objects_shared = self.objects.clone();
        let params_lock = self.params.clone();
        let stopped_lock = self.thread_stopped.clone();

        let framerate = *self.framerate.lock().unwrap();

        thread::spawn(move || {
            let mut method;
            let mut num_threads;
            let mut time_step; 

            {
                let params = params_lock.lock().unwrap();

                method = params.method.clone();
                num_threads = params.num_threads;
                time_step = if params.use_target_speed {
                    0.001f64
                } else {
                    params.time_step
                };
            }

            let mut i = 0;
            let mut steps_until_update = 1000u128;
            
            let mut use_symplectic = match method {
                IntegrationMethod::Symplectic(_) => true,
                IntegrationMethod::RK4 => false,
            };

            // Prepare threads if needed
            let mut state =
                Engine::prepare_worker_threads(num_threads, objects_local.read().unwrap().len());
            let mut handles = vec![];

            if use_symplectic && num_threads > 1 {
                handles = Engine::start_worker_threads(&state, &objects_local);
            }

            let mut time_now = Instant::now();
            loop {
                if num_threads == 1 || !use_symplectic {
                    let mut objects_local = objects_local.write().unwrap();
                    if use_symplectic {
                        let coefficient_table = method.get_coefficients();

                        while i < steps_until_update {
                            for (c, d) in coefficient_table.iter() {
                                if *c != 0.0 {
                                    objects_local.iter_mut().for_each(|x| {
                                        _ = x.position.add_mut(&x.velocity.multiply(time_step * c))
                                    });
                                }

                                if *d != 0.0 {
                                    // This will speed up 4th order Simplectic integration
                                    loop {
                                        let num_objects = objects_local.len();
                                        match integration::symplectic(
                                            &objects_local,
                                            (0, 0),
                                            (num_objects - 2, num_objects - 1), // Inclusive range
                                        ) {
                                            Ok(res) => {
                                                for (body, vector) in
                                                    objects_local.iter_mut().zip(res)
                                                {
                                                    body.velocity
                                                        .add_mut(&vector.multiply(time_step * d));
                                                    body.acceleration = vector;
                                                }

                                                break;
                                            }
                                            Err(indices) => {
                                                integration::collide_objects(
                                                    &mut objects_local,
                                                    &indices,
                                                );
                                            }
                                        }
                                    }
                                }
                            }

                            i += 1;
                        }

                    // RK 4
                    } else {
                        while i < steps_until_update {
                            match integration::runge_kutta_4(&mut objects_local, time_step) {
                                Ok(_) => {}
                                Err(indices) => {
                                    integration::collide_objects(&mut objects_local, &indices)
                                }
                            }

                            i += 1;
                        }
                    }
                } else {
                    let coefficient_table = method.get_coefficients();
                    while i < steps_until_update {
                        for (c, d) in coefficient_table.iter() {
                            if *c != 0.0 {
                                let mut objects = objects_local.write().unwrap();
                                objects.iter_mut().for_each(|x| {
                                    _ = x.position.add_mut(&x.velocity.multiply(time_step * c))
                                });
                            }

                            if *d != 0.0 {
                                'integration_loop: loop {
                                    state.barrier.wait(); // Release worker threads to do work
                                    state.barrier.wait(); // Work is completed, now we can gather results

                                    let results = &state.thread_results;
                                    let mut objects = objects_local.write().unwrap();

                                    // These will hold the final acceleration values
                                    let mut acceleration_vectors =
                                        vec![Vector3d::default(); objects.len()];

                                    for lock in results {
                                        let result = lock.lock().unwrap();

                                        match &*result {
                                            Ok(vectors) => {
                                                for (acc, res) in
                                                    acceleration_vectors.iter_mut().zip(vectors)
                                                {
                                                    acc.add_mut(res);
                                                }
                                            }
                                            Err(collision) => {
                                                integration::collide_objects(
                                                    &mut objects,
                                                    collision,
                                                );

                                                let mut work_queue =
                                                    state.work_queue.write().unwrap();

                                                *work_queue = Engine::get_mt_splices(
                                                    objects.len(),
                                                    num_threads,
                                                );

                                                continue 'integration_loop;
                                            }
                                        }
                                    }

                                    // Step was successful (no collisions) so we can update state
                                    for (object, acc) in
                                        objects.iter_mut().zip(acceleration_vectors)
                                    {
                                        object.velocity.add_mut(&acc.multiply(time_step * d));
                                        object.acceleration = acc;
                                    }

                                    break;
                                }
                            }
                        }
                        i += 1;
                    }
                }

                let mut params = params_lock.lock().unwrap();

                if !params.is_running
                    || num_threads != params.num_threads
                    || method != params.method
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

                    method = params.method.clone();
                    num_threads = params.num_threads;

                    use_symplectic = match method {
                        IntegrationMethod::Symplectic(_) => true,
                        IntegrationMethod::RK4 => false,
                    };

                    // Prepare threads if needed
                    state = Engine::prepare_worker_threads(
                        num_threads,
                        objects_local.read().unwrap().len(),
                    );
                    handles = vec![];

                    if use_symplectic && num_threads > 1 {
                        handles = Engine::start_worker_threads(&state, &objects_local);
                    }
                }

                let new_time = Instant::now();
                let duration = (new_time - time_now).as_secs_f64();

                let speed = if duration == 0.0 {
                    2000.0
                } else {
                    steps_until_update as f64 / duration
                };

                steps_until_update = (speed / framerate as f64).round() as u128;

                if params.use_target_speed {
                    let target_speed = params.target_speed;
                    time_step = target_speed / speed;
                    params.time_step = time_step;
                } else {
                    time_step = params.time_step;
                    let target_speed = time_step * speed;
                    params.target_speed = target_speed;
                }

                params.iteration_speed = speed;
                time_now = new_time;

                let objects = objects_local.read().unwrap();
                let mut objects_shared = objects_shared.lock().unwrap();
                objects_shared.clear();
                objects.iter().for_each(|o| objects_shared.push(o.clone()));

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
        let mut buckets = Vec::new();
        for i in 0..num_threads {
            let mut num = if i < len % num_threads { 1 } else { 0 };
            num += len / num_threads;

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

                    let work_item = &work_queue_lock.read().unwrap()[i_thread];
                    let objects = objects_lock.read().unwrap();

                    let mut result = result_lock.lock().unwrap();
                    *result = integration::symplectic(&objects, work_item.start, work_item.end);

                    barrier_lock.wait(); // Important to have two barrier waits. Main thread prepares work between
                }
            });

            handles.push(handle);
        }

        handles
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine {
            objects: Arc::new(Mutex::new(AstronomicalObject::default())),
            framerate: Arc::new(Mutex::new(60)),
            params: Arc::new(Mutex::new(SimulatorControl {
                target_speed: 86400.0 * 1.0,
                is_running: false,
                method: IntegrationMethod::Symplectic(4),
                num_threads: 1,
                iteration_speed: 0.0,
                time_step: 0.01,
                use_target_speed: false,
            })),
            thread_stopped: Arc::new(Mutex::new(true)),
        }
    }
}
