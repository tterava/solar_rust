use itertools::izip;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicBool, Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::astronomy::AstronomicalObject;
use crate::vector::Vector4d;
pub const G: f64 = 6.6743E-11;

pub enum IntegrationMethod {
    ImplicitEuler,
    RK4,
}

#[derive(Default)]
struct IntermediateState {
    mass: f64,
    velocity: Vector4d,
    dv: Vector4d,
    position: Vector4d,
}

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

    pub fn start(
        &self,
        object_buffer: Arc<Mutex<Vec<AstronomicalObject>>>,
        method: IntegrationMethod,
    ) -> (std::thread::JoinHandle<()>, Arc<AtomicBool>) {
        let kill_request = Arc::new(AtomicBool::new(false));
        let kill_clone = kill_request.clone();

        let mut objects_local = self.objects.read().unwrap().clone();
        let integrate = match method {
            IntegrationMethod::ImplicitEuler => Engine::semi_implicit_euler,
            IntegrationMethod::RK4 => Engine::runge_kutta_4,
        };

        let framerate = self.framerate;
        (
            thread::spawn(move || {
                let target_speed = 86400.0 * 1.0;  // 86400 seconds is a day

                let mut time_step = 1.0;
                let mut i = 0;
                let mut steps_until_update: u128;
                let mut time_now = Instant::now();

                {
                    let mut temp_objects = objects_local.clone();
                    let mut counter = 0u128;

                    while (Instant::now() - time_now).as_secs_f64() < 0.2 {
                        integrate(&mut temp_objects, time_step);
                        counter += 1;
                    }

                    steps_until_update = counter * 5 / framerate as u128;
                    println!("Initializing with {} iterations in a second", counter);
                }
                
                loop {
                    integrate(&mut objects_local, time_step);
                    Engine::process_collisions(&mut objects_local);

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
                        time_step = target_speed / speed;
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

    fn runge_kutta_4(local_bodies: &mut Vec<AstronomicalObject>, time_step: f64) {
        let mut dt = 0.0f64;
        let num_bodies = local_bodies.len();

        let mut s: [Vec<IntermediateState>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

        for state in 0..4 {
            if state == 1 || state == 3 {
                dt += 0.5 * time_step;
            }

            let mut positions: Vec<_> = local_bodies.iter().map(|x| x.position).collect();
            let mut dv = vec![Vector4d::default(); num_bodies];

            if state >= 1 {
                for (position, prev_state) in positions.iter_mut().zip(&s[state - 1]) {
                    position.add_mut(&prev_state.velocity.multiply(dt));
                }
            }

            for i in 0..num_bodies - 1 {
                for j in i + 1..num_bodies {
                    let difference = positions[j].substract(&positions[i]);
                    let grav_modifier = G / (difference.length().powi(3));

                    dv[i].add_mut(&difference.multiply(grav_modifier * local_bodies[j].mass));
                    dv[j].add_mut(&difference.multiply(-grav_modifier * local_bodies[i].mass));
                }
            }

            s[state] = (0..num_bodies)
                .map(|i| IntermediateState {
                    mass: local_bodies[i].mass,
                    velocity: if state == 0 {
                        local_bodies[i].velocity
                    } else {
                        local_bodies[i]
                            .velocity
                            .add(&s[state - 1][i].dv.multiply(dt))
                    },
                    position: positions[i],
                    dv: dv[i],
                })
                .collect();
        }

        for (i, body) in local_bodies.iter_mut().enumerate() {
            let mut dxdt = s[0][i]
                .velocity
                .add(&s[1][i].velocity.add(&s[2][i].velocity).multiply(2.0))
                .add(&s[3][i].velocity)
                .multiply(1.0 / 6.0);

            let mut dvdt = s[0][i]
                .dv
                .add(&s[1][i].dv.add(&s[2][i].dv).multiply(2.0))
                .add(&s[3][i].dv)
                .multiply(1.0 / 6.0);

            body.velocity.add_mut(dvdt.multiply_mut(time_step));
            body.position.add_mut(dxdt.multiply_mut(time_step));
        }
    }

    fn semi_implicit_euler(local_bodies: &mut Vec<AstronomicalObject>, time_step: f64) {
        let len = local_bodies.len();
        let mut acceleration_vectors = vec![Vector4d::default(); len];

        for first in 0..len - 1 {
            for second in first + 1..len {
                let (a, b) = (&local_bodies[first], &local_bodies[second]);

                let difference = b.position.substract(&a.position);
                let grav_mult = G / (difference.length().powi(3)); // Divide by r^3 to get a unit vector out of difference

                acceleration_vectors[first].add_mut(&difference.multiply(grav_mult * b.mass));
                acceleration_vectors[second].add_mut(&difference.multiply(-grav_mult * a.mass));
            }
        }

        for (i, body) in local_bodies.iter_mut().enumerate() {
            body.velocity
                .add_mut(acceleration_vectors[i].multiply_mut(time_step));
            body.position.add_mut(&body.velocity.multiply(time_step));
        }
    }

    fn process_collisions(local_objects: &mut Vec<AstronomicalObject>) {
        let obs = local_objects;
        'collision_loop: loop {
            let len = obs.len();
            for i in 0..len - 1 {
                for j in i + 1..len {
                    // No collision
                    if obs[i].position.distance(&obs[j].position) > obs[i].radius + obs[j].radius {
                        continue;
                    }

                    let (heavy, light) = if obs[i].mass >= obs[j].mass {
                        (i, j)
                    } else {
                        (j, i)
                    };
                    let total_mass = obs[i].mass + obs[j].mass;

                    obs[heavy].velocity = obs[heavy]
                        .velocity
                        .multiply(obs[heavy].mass)
                        .add(&obs[light].velocity.multiply(obs[light].mass))
                        .multiply(1.0 / total_mass);
                    obs[heavy].position = obs[heavy].position.add(
                        &obs[light]
                            .position
                            .substract(&obs[heavy].position)
                            .multiply(obs[light].mass / total_mass),
                    );

                    obs[heavy].radius *= (total_mass / obs[heavy].mass).powf(1.0 / 3.0);

                    println!("{} collided into {}!", obs[light].name, obs[heavy].name);

                    obs.remove(light);
                    continue 'collision_loop;
                }
            }
            break;
        }
    }

    // fn general_rk(
    //     local_bodies: &mut Vec<AstronomicalObject>,
    //     time_step: f64,
    //     stages: u32,
    //     tableau: Vec<Vec<f64>>,
    // ) {
    //     let mut k: Vec<IntermediateState> = Vec::new();
    //     for s in 0..stages {}
    // }
}
