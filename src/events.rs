use std::{sync::atomic::Ordering, time::Instant};

use rand::SeedableRng;

use crate::{astronomy::AstronomicalObject, input, integration::IntegrationMethod, DrawingApp};

pub fn handle_event(app: &DrawingApp, evt: nwg::Event, evt_data: &nwg::EventData) {
    use nwg::Event as E;
    use nwg::EventData::OnKey as K;
    use nwg::EventData::OnMouseWheel as MW;
    use nwg::MousePressEvent as M;

    match evt {
        E::OnMousePress(M::MousePressRightUp) => app.is_dragging.store(false, Ordering::Relaxed),
        E::OnMousePress(M::MousePressRightDown) => {
            app.is_dragging.store(true, Ordering::Relaxed);
            input::input_listener(app.is_dragging.clone(), app.camera.clone());
        }
        E::OnMousePress(M::MousePressLeftDown) => {
            let (w_x, w_y) = app.window.position();
            let (m_x, m_y) = winput::Mouse::position().unwrap();

            let (x, y) = (m_x - w_x - 8, m_y - w_y - 31); // Offset x: 8, y: 31 works for Windows 11. TODO: Figure a better way for this
            let targets = app.targets.borrow();

            println!("{}, {}", m_x - w_x, m_y - w_y);

            for target in targets.iter().rev() {
                let tx = target.x as i64;
                let ty = target.y as i64;
                let r = target.radius as i64;

                if (tx - x as i64).pow(2) + (ty - y as i64).pow(2) <= r.pow(2) {
                    if app.camera.lock().unwrap().animation_start.is_some() {
                        break;
                    }

                    let mut current_target = app.current_target.borrow_mut();

                    if let Some(t) = *current_target {
                        if target.uuid == t {
                            break;
                        }
                    }

                    *current_target = Some(target.uuid);
                    *app.next_status_update.borrow_mut() = Instant::now();

                    drop(current_target); // Important! get_paint_objects acquires both of the locks and can cause deadlocks if this is not dropped

                    let mut camera = app.camera.lock().unwrap();
                    let start = camera.target;
                    let start_dis = camera.distance;

                    camera.start_animation(start, start_dis);

                    break;
                }
            }
        }
        E::OnMouseWheel => {
            if let MW(amount) = evt_data {
                app.zoom(*amount);
            }
        }
        E::OnKeyPress => {
            if let K(key) = evt_data {
                match key {
                    107 => {
                        let mut params = app.engine.params.lock().unwrap();
                        if params.use_target_speed {
                            params.target_speed *= 1.2;
                        } else {
                            params.time_step *= 1.2;
                        }

                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    109 => {
                        let mut params = app.engine.params.lock().unwrap();
                        if params.use_target_speed {
                            params.target_speed /= 1.2;
                        } else {
                            params.time_step /= 1.2;
                        }

                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    32 => {
                        if app.engine.params.lock().unwrap().is_running {
                            app.engine.stop();
                        } else {
                            app.engine.start_mt();
                        }
                    }
                    49..=57 => {
                        let threads = key - 48;
                        let mut params = app.engine.params.lock().unwrap();

                        params.num_threads = threads as usize;
                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    81 => {
                        let mut params = app.engine.params.lock().unwrap();

                        params.num_threads = (params.num_threads - 1).max(1);
                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    87 => {
                        let mut params = app.engine.params.lock().unwrap();

                        params.num_threads += 1;
                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    77 => {
                        let mut params = app.engine.params.lock().unwrap();
                        params.method = match params.method {
                            IntegrationMethod::Symplectic(n) => {
                                if n == 4 {
                                    IntegrationMethod::RK4
                                } else {
                                    IntegrationMethod::Symplectic(n + 1)
                                }
                            }
                            IntegrationMethod::RK4 => IntegrationMethod::Symplectic(1),
                        };
                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    82 => {
                        let params = app.engine.params.lock().unwrap();
                        if params.is_running {
                            return;
                        }

                        let mut objects = app.engine.objects.lock().unwrap();
                        let mut rng = rand::rngs::StdRng::from_entropy();
                        let new_object = AstronomicalObject::place_on_orbit(
                            AstronomicalObject::get_random_planet(&mut rng),
                            &objects[0],
                            &mut rng
                        );
                        objects.push(new_object);
                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    84 => {
                        let mut params = app.engine.params.lock().unwrap();
                        params.use_target_speed = !params.use_target_speed;
                        *app.next_status_update.borrow_mut() = Instant::now();
                    }
                    key => {
                        println!("Key: {}", key)
                    }
                }
            }
        }
        _ => {}
    }

    app.canvas.invalidate();
}
