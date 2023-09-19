use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicBool, Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::astronomy::AstronomicalObject;
use crate::vector::Vector4d;
const G: f64 = 6.6743E-11;

#[derive(Default)]
pub struct Engine {
    objects: Arc<RwLock<Vec<AstronomicalObject>>>,
    framerate: u64,
}

impl Engine {
    pub fn init(framerate: u64) -> Engine {
        Engine {
            objects: Arc::new(RwLock::new(AstronomicalObject::default())),
            framerate,
        }
    }

    pub fn start_multithread(&self, object_buffer: Arc<Mutex<Vec<AstronomicalObject>>>) -> (std::thread::JoinHandle<()>, Arc<AtomicBool>) {
        let kill_request = Arc::new(AtomicBool::new(false));
        let kill_clone = kill_request.clone();

        let mut objects_local = self.objects.read().unwrap().clone();
        let framerate = self.framerate;
        (
            thread::spawn(move || {
                let mut time_step = 1.0;
                let mut i = 0;
                let mut steps_until_update: u128 = 1000000;
                let mut time_now = Instant::now();

                loop {
                    let mut new_values = vec![Vector4d::default(); objects_local.len()];

                    for first in 0..objects_local.len() - 1 {
                        for second in first + 1..objects_local.len() {
                            let (a, b) = (&objects_local[first], &objects_local[second]);
                            let grav_mult = G / a.position.distance_squared(&b.position);

                            let unit_vec = b.position.substract(&a.position).get_unit_vector();
                            let acc_a = unit_vec.multiply(grav_mult * b.mass);
                            let acc_b = unit_vec.multiply(-grav_mult * a.mass);

                            new_values[first].add_mut(&acc_a);
                            new_values[second].add_mut(&acc_b);
                        }
                    }

                    for (i, body) in objects_local.iter_mut().enumerate() {
                        body.velocity.add_mut(&new_values[i].multiply(time_step));
                        body.position.add_mut(&body.velocity.multiply(time_step));
                    }

                    i += 1;
                    if i >= steps_until_update {
                        let new_time = Instant::now();
                        let duration = (new_time - time_now).as_secs_f64();

                        let speed = if duration == 0.0 {
                            steps_until_update as f64
                        } else {
                            steps_until_update as f64 / duration
                        };

                        i = 0;
                        steps_until_update = (speed / framerate as f64).round() as u128;
                        time_step = 86400.0 * 5.0 / speed;
                        time_now = new_time;

                        {
                            let mut objects_shared = object_buffer.lock().unwrap();
                            objects_shared.clear();
                            objects_local
                                .iter()
                                .for_each(|o| objects_shared.push(o.clone()));
                        }

                        if kill_clone.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }),
            kill_request,
        )
    }
}
