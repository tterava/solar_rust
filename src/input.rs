use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, self};
use std::time::Duration;

use crate::camera::Camera;

pub fn input_listener(
    mouse_down: Arc<AtomicBool>,
    camera: Arc<Mutex<Camera>>,
) -> JoinHandle<()> {
    const SENSITIVITY: f64 = 0.003;

    thread::spawn(move || {
        let duration = Duration::from_millis(10);
        let (pitch_start, yaw_start);
        {
            let cam = camera.lock().unwrap();
            pitch_start = cam.get_pitch();
            yaw_start = cam.get_yaw();
        }

        let (x_start, y_start) = winput::Mouse::position().unwrap();

        while mouse_down.load(std::sync::atomic::Ordering::Relaxed) {
            thread::sleep(duration);
            let (x, y) = winput::Mouse::position().unwrap();

            let pitch_add = (y_start - y) as f64 * SENSITIVITY; // Positive value should decrease pitch, so swap order of direction to make math easier. This way positive value increases pitch
            let yaw_add = (x_start - x) as f64 * SENSITIVITY;  // Positive value should decrease yaw

            let mut cam = camera.lock().unwrap();
            cam.set_pitch(pitch_start + pitch_add);
            cam.set_yaw(yaw_start + yaw_add);  
        }
    })
}
