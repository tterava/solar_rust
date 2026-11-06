use std::time::Instant;

use egui::{Key, PointerButton, Response, Ui};
use rand::SeedableRng;

use crate::astronomy::AstronomicalObject;
use crate::integration::IntegrationMethod;
use crate::SolarApp;

pub fn handle_input(app: &mut SolarApp, ui: &Ui, response: &Response) {
    const CAMERA_SENSITIVITY: f64 = 0.003;

    // Rotate camera by dragging with the right mouse button
    if response.dragged_by(PointerButton::Secondary) {
        let delta = response.drag_delta();

        app.camera
            .set_yaw(app.camera.get_yaw() - delta.x as f64 * CAMERA_SENSITIVITY);
        app.camera
            .set_pitch(app.camera.get_pitch() - delta.y as f64 * CAMERA_SENSITIVITY);
    }

    // Select an object with left click
    if response.clicked_by(PointerButton::Primary) {
        if let Some(pos) = response.interact_pointer_pos() {
            for target in app.targets.iter().rev() {
                let dx = pos.x - target.x;
                let dy = pos.y - target.y;

                if dx * dx + dy * dy <= target.radius * target.radius {
                    if app.camera.animation_start.is_some() {
                        break;
                    }

                    if app.current_target == Some(target.uuid) {
                        break;
                    }

                    app.current_target = Some(target.uuid);
                    app.next_status_update = Instant::now();

                    let start = app.camera.target;
                    let start_dis = app.camera.distance;

                    let start_target = app
                        .engine
                        .objects
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|x| x.uuid == target.uuid)
                        .map(|b| b.position)
                        .unwrap_or(start);

                    app.camera.start_animation(start, start_dis, start_target);

                    break;
                }
            }
        }
    }

    // Zoom with mouse wheel
    if response.contains_pointer() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);

        if scroll != 0.0 {
            app.camera.zoom(if scroll > 0.0 { 1 } else { -1 });
        }
    }

    // Space: start/stop simulation
    if ui.input(|i| i.key_pressed(Key::Space)) {
        if app.engine.params.lock().unwrap().is_running {
            app.engine.stop();
        } else {
            app.engine.start_mt();
        }
    }

    // Numpad +: speed up
    if ui.input(|i| i.key_pressed(Key::Plus)) {
        let mut params = app.engine.params.lock().unwrap();
        if params.use_target_speed {
            params.target_speed *= 1.2;
        } else {
            params.time_step *= 1.2;
        }

        app.next_status_update = Instant::now();
    }

    // Numpad -: slow down
    if ui.input(|i| i.key_pressed(Key::Minus)) {
        let mut params = app.engine.params.lock().unwrap();
        if params.use_target_speed {
            params.target_speed /= 1.2;
        } else {
            params.time_step /= 1.2;
        }

        app.next_status_update = Instant::now();
    }

    // Number keys: set thread count
    for (threads, key) in (1..=9).zip([
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ]) {
        if ui.input(|i| i.key_pressed(key)) {
            app.engine.params.lock().unwrap().num_threads = threads;
            app.next_status_update = Instant::now();
        }
    }

    // q: fewer threads
    if ui.input(|i| i.key_pressed(Key::Q)) {
        let mut params = app.engine.params.lock().unwrap();
        params.num_threads = (params.num_threads - 1).max(1);
        app.next_status_update = Instant::now();
    }

    // w: more threads
    if ui.input(|i| i.key_pressed(Key::W)) {
        let mut params = app.engine.params.lock().unwrap();
        params.num_threads += 1;
        app.next_status_update = Instant::now();
    }

    // m: cycle integration method
    if ui.input(|i| i.key_pressed(Key::M)) {
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
        app.next_status_update = Instant::now();
    }

    // t: add a random planet
    if ui.input(|i| i.key_pressed(Key::T)) && !app.engine.params.lock().unwrap().is_running {
        let mut objects = app.engine.objects.lock().unwrap();
        let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
        let new_object = AstronomicalObject::place_on_orbit(
            AstronomicalObject::get_random_planet(&mut rng),
            &objects[0],
            &mut rng,
        );
        objects.push(new_object);
        app.next_status_update = Instant::now();
    }

    // n: toggle target speed mode
    if ui.input(|i| i.key_pressed(Key::N)) {
        let mut params = app.engine.params.lock().unwrap();
        params.use_target_speed = !params.use_target_speed;
        app.next_status_update = Instant::now();
    }
}
