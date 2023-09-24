use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Barrier, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Instant;

use crate::astronomy::AstronomicalObject;
use crate::integration;
use crate::vector::Vector4d;

pub enum IntegrationMethod {
    ImplicitEuler,
    RK4,
}

pub struct Engine {
    pub objects: Arc<Mutex<Vec<AstronomicalObject>>>,
    pub framerate: Arc<Mutex<u32>>,
    pub target_speed: Arc<Mutex<f64>>,
    pub is_running: Arc<Mutex<bool>>, // For outside communication
    thread_stopped: Arc<Mutex<bool>>, // Thread communicates that it has cleanly ended
}

impl Engine {
    pub fn start_mt(&self, method: IntegrationMethod, num_threads: usize) {
        let mut stopped = self.thread_stopped.lock().unwrap();
        if !*stopped {
            return;
        }

        *stopped = false;
        *self.is_running.lock().unwrap() = true;

        let objects_local = Arc::new(RwLock::new(self.objects.lock().unwrap().clone()));
        let use_euler = match method {
            IntegrationMethod::ImplicitEuler => true,
            IntegrationMethod::RK4 => false,
        };

        let target_speed_local = self.target_speed.clone();
        let objects_shared = self.objects.clone();
        let is_running_clone = self.is_running.clone();
        let thread_stopped_clone = self.thread_stopped.clone();

        let framerate = *self.framerate.lock().unwrap();

        thread::spawn(move || {
            let mut time_step = 0.001f64;
            let mut i = 0;
            let mut steps_until_update = 1000u128;
            let mut time_now = Instant::now();
            let mut num_objects = objects_local.read().unwrap().len();

            let mut thread_results = Vec::with_capacity(num_threads);
            (0..num_threads).for_each(|_| {
                thread_results.push(Arc::new(Mutex::new(Vec::with_capacity(num_objects))))
            });

            let work_queue = Arc::new(RwLock::new(Vec::with_capacity(num_threads)));
            *work_queue.write().unwrap() = Engine::get_mt_splices(num_objects, num_threads);

            let barrier = Arc::new(Barrier::new(num_threads + 1));
            let worker_kill = Arc::new(AtomicBool::new(false));
            let collision_signal = Arc::new(Mutex::new(false));

            // Prepare threads if needed
            if use_euler && num_threads > 1 {
                for (i, thread_result) in thread_results.iter().enumerate() {
                    let i_thread = i;
                    let work_queue_lock = work_queue.clone();
                    let objects_lock = objects_local.clone();
                    let barrier_lock = barrier.clone();
                    let result_lock = thread_result.clone();
                    let kill_lock = worker_kill.clone();
                    let collision_lock = collision_signal.clone();

                    thread::spawn(move || {
                        loop {
                            barrier_lock.wait(); // Wait until main thread has assigned work

                            if kill_lock.load(std::sync::atomic::Ordering::Relaxed) {
                                break;
                            }

                            let (start, end) = work_queue_lock.read().unwrap()[i_thread];
                            let objects = objects_lock.read().unwrap();

                            let (vectors, collisions_found) =
                                integration::semi_implicit_euler(&objects, start, end);

                            if collisions_found {
                                *collision_lock.lock().unwrap() = true;
                            } else {
                                // No need to even update result vectors if collisions are found
                                let mut result_vectors = result_lock.lock().unwrap();
                                *result_vectors = vectors;
                            }

                            barrier_lock.wait();
                            // Important to have two barrier waits. Main thread prepares work between
                        }

                        println!("Worker thread killed");
                    });
                }
            }

            loop {
                if num_threads == 1 || !use_euler {
                    let mut objects_local = objects_local.write().unwrap();
                    if use_euler {
                        while i < steps_until_update {
                            let (acceleration_vectors, collisions_found) =
                                integration::semi_implicit_euler(
                                    &objects_local,
                                    (0, 0),
                                    (num_objects - 2, num_objects - 1),
                                );

                            if collisions_found {
                                // Only find collisions and perform object combining if they are found in integration
                                integration::process_collisions(&mut objects_local);
                            } else {
                                for (body, vector) in
                                    objects_local.iter_mut().zip(acceleration_vectors)
                                {
                                    body.velocity.add_mut(&vector.multiply(time_step));
                                    body.position.add_mut(&body.velocity.multiply(time_step));
                                }
                            }

                            i += 1;
                        }
                    } else {
                        while i < steps_until_update {
                            let collisions_found =
                                integration::runge_kutta_4(&mut objects_local, time_step);

                            if collisions_found {
                                // Only find collisions and perform object combining if they are found in integration
                                integration::process_collisions(&mut objects_local);
                            }

                            i += 1;
                        }
                    }
                } else {
                    while i < steps_until_update {
                        barrier.wait(); // Release worker threads to do work
                        barrier.wait(); // Work is completed, now we can gather results

                        let mut objects = objects_local.write().unwrap();
                        let mut collisions_lock = collision_signal.lock().unwrap();

                        if *collisions_lock {
                            // Only process collisions if threads found one in integration
                            integration::process_collisions(&mut objects);
                            num_objects = objects.len();

                            *work_queue.write().unwrap() =
                                Engine::get_mt_splices(num_objects, num_threads);
                            *collisions_lock = false;
                        } else {
                            let mut acceleration_vectors = vec![Vector4d::default(); num_objects];

                            for lock in &thread_results {
                                let thread_vectors = lock.lock().unwrap();
                                for (acc_vector, thread_vector) in
                                    acceleration_vectors.iter_mut().zip(thread_vectors.iter())
                                {
                                    acc_vector.add_mut(thread_vector);
                                }
                            }

                            for (body, vector) in objects.iter_mut().zip(acceleration_vectors) {
                                body.velocity.add_mut(&vector.multiply(time_step));
                                body.position.add_mut(&body.velocity.multiply(time_step));
                            }
                        }

                        i += 1;
                    }
                }

                let new_time = Instant::now();
                let duration = (new_time - time_now).as_secs_f64();

                let speed = if duration == 0.0 {
                    steps_until_update as f64
                } else {
                    steps_until_update as f64 / duration
                };

                i = 0;
                steps_until_update = (speed / framerate as f64).round() as u128;
                time_step = *target_speed_local.lock().unwrap() / speed;

                time_now = new_time;

                println!("Speed: {:.1}", speed);
                // println!("Time step: {:.4}", time_step);

                let mut objects_shared = objects_shared.lock().unwrap();
                objects_shared.clear();
                objects_local
                    .read()
                    .unwrap()
                    .iter()
                    .for_each(|o| objects_shared.push(o.clone()));

                if !*is_running_clone.lock().unwrap() {
                    *thread_stopped_clone.lock().unwrap() = true;
                    if num_threads > 1 {
                        worker_kill.store(true, std::sync::atomic::Ordering::Relaxed);
                        barrier.wait(); // Release threads to see the kill request
                    }
                    break;
                }
            }
        });
    }

    pub fn stop(&self) {
        *self.is_running.lock().unwrap() = false;
    }

    fn get_mt_splices(
        num_bodies: usize,
        num_threads: usize,
    ) -> Vec<((usize, usize), (usize, usize))> {
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
    }
}

impl Default for Engine {
    fn default() -> Self {
        Engine {
            objects: Arc::new(Mutex::new(AstronomicalObject::default())),
            framerate: Arc::new(Mutex::new(60)),
            target_speed: Arc::new(Mutex::new(86400.0 * 1.0)),
            is_running: Arc::new(Mutex::new(false)),
            thread_stopped: Arc::new(Mutex::new(true)),
        }
    }
}
