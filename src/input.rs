use std::f64::consts::PI;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, self};
use std::time::Duration;

use crate::camera::Camera;
use crate::matrix::Matrix4d;
use crate::vector::Vector4d;

pub fn get_pitch(pos: &Vector4d) -> f64 {
    
    // Length of the component that is located on XZ-plane. This is our X for atan2
    let base_xz = (pos.data[0].powi(2) + pos.data[2].powi(2)).sqrt();
    pos.data[1].atan2(base_xz)
}

pub fn input_listener(
    mouse_down: Arc<AtomicBool>,
    camera: Arc<Mutex<Camera>>,
) -> JoinHandle<()> {
    const SENSITIVITY: f64 = 0.006;

    thread::spawn(move || {
        let duration = Duration::from_millis(10);
        let (camera_pos_start, camera_target_start, pitch_start, yaw_start);
        {
            let camera = camera.lock().unwrap();
            camera_pos_start = camera.pos;
            camera_target_start = camera.target;
            pitch_start = camera.get_pitch();
            yaw_start = camera.get_yaw();
        }

        let (x_start, y_start) = winput::Mouse::position().unwrap();

        while mouse_down.load(std::sync::atomic::Ordering::Relaxed) {
            thread::sleep(duration);
            let (x, y) = winput::Mouse::position().unwrap();

            let mut pitch_add = (y - y_start) as f64 * SENSITIVITY; // Positive value should increase
            let yaw_add = (x_start - x) as f64 * SENSITIVITY;  // Positive value should reduce yaw, so swap order of direction to make math easier. This way positive value increases yaw

            if yaw_add == 0.0 && pitch_add == 0.0 {
                continue;
            }

            let pitch_limit = 89.0f64.to_radians();
            
            if pitch_start + pitch_add > pitch_limit {
                pitch_add = pitch_limit - pitch_start;
            }
            else if pitch_start + pitch_add < -pitch_limit {
                pitch_add = -pitch_limit - pitch_start;
            }

            let trans = Matrix4d::trans(&camera_target_start);
            let y1 = Matrix4d::rot_y(-yaw_start);
            let x1 = Matrix4d::rot_x(-pitch_add); // has opposite direction when rotated
            let y2 = Matrix4d::rot_y(yaw_start + yaw_add);
            let trans2 = Matrix4d::trans(&camera_target_start.multiply(-1.0));

            let transformation = trans2 * y2 * x1 * y1 * trans;
            let new_pos = transformation * &camera_pos_start;

            let mut cam = camera.lock().unwrap();
            cam.pos = new_pos;
        }
    })
    
}
