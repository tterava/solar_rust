use std::{f64::consts::PI, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};

use glam::{DAffine3, DVec3};

use crate::{astronomy::AU};

// For drawing camera is assumed to be situated on the positive side of the Z-axis at (0,0,1), with target being origin.
// Matrix operations are used to transform the simulation space into camera space.
// Camera is always kept in line with Y-axis (Y-axis is directly up). In other words camera only has pitch and yaw.

#[derive(Debug)]
pub struct Camera {
    pub target: DVec3,
    pub distance: f64,
    yaw: f64,
    pitch: f64,
    pub fov: f64,
    animation_progress: Arc<Mutex<u32>>,
    animation_start_distance: f64,
    pub animation_start: Option<DVec3>
}

impl Camera {
    pub fn get_pitch(&self) -> f64 {
        self.pitch
    }
    pub fn set_pitch(&mut self, angle: f64) {
        // Limit pitch between -89.9 degrees and 89.9 degrees or things go upside down.
        self.pitch = angle.min(PI / 180.0 * 89.9).max(-PI / 180.0 * 89.9);
    }

    pub fn get_yaw(&self) -> f64 {
        self.yaw
    }
    pub fn set_yaw(&mut self, angle: f64) {
        self.yaw = angle % (2.0 * PI);
    }

    pub fn get_full_transformation(&self) -> DAffine3 {
        let translation = DAffine3::from_translation(-self.target);

        let rot_y = DAffine3::from_rotation_y(-self.yaw);
        let rot_x = DAffine3::from_rotation_x(-self.pitch);

        let scale = DAffine3::from_scale(DVec3::ONE / self.distance);

        scale * rot_x * rot_y * translation
    }

    pub fn zoom(&mut self, amount: i32) {
        match amount {
            0.. => self.distance /= 1.1,
            _ => self.distance *= 1.1,
        };
    }

    pub fn get_position(&self) -> DVec3 {
        let scale = DAffine3::from_scale(DVec3::ONE * self.distance);
        let rot_y = DAffine3::from_rotation_y(self.yaw);
        let rot_x = DAffine3::from_rotation_x(self.pitch);

        let direction_vec = (scale * rot_y * rot_x).transform_point3(DVec3::new(0.0, 0.0, 1.0));

        self.target + direction_vec
    }

    pub fn start_animation(&mut self, start: DVec3, distance: f64) {
        self.animation_start = Some(start);
        self.animation_start_distance = distance;

        let progress_c = self.animation_progress.clone();
        *progress_c.lock().unwrap() = 0;

        thread::spawn(move || {
            let start = Instant::now();
            let target_time = 1.5;
            loop {
                {
                    let mut progress = progress_c.lock().unwrap();
                    let duration = (Instant::now() - start).as_secs_f64();

                    if duration >= target_time {
                        *progress = 1000;
                        break;
                    }

                    *progress = (duration / target_time * 1000.0) as u32  ;
                }
                thread::sleep(Duration::from_millis(10));
            }    
        });
    }

    pub fn get_animation_position(&self, target: DVec3, radius: f64) -> Option<(DVec3, f64)> {
        let radius_multiplier = 100.0;
        match self.animation_start {
            Some(start) => {
                let progress = self.animation_progress.lock().unwrap();
                if *progress >= 1000 {
                    return Some((target, radius * radius_multiplier))
                } 

                // let eased_progress = (*progress as f64 / 1000.0 * 2.0 * PI - PI).tanh() * 0.5 + 0.5;
                let eased_progress = Camera::get_camera_easing(*progress);

                let difference = target - start;
                let difference_distance = radius * radius_multiplier - self.animation_start_distance;
                Some(
                    (difference * eased_progress + start, difference_distance * eased_progress + self.animation_start_distance)
                )
            },
            None => None
        }
    }

    fn get_camera_easing(progress: u32) -> f64 {
        let mut progress_f64 = progress as f64 * 2.0 / 1000.0;
        if progress_f64 < 1.0 {
            return progress_f64.powi(3) / 2.0;
        }
        progress_f64 -= 2.0;

        progress_f64.powi(3) / 2.0 + 1.0
    }
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            target: DVec3::ZERO,
            distance: 2.0 * AU,
            yaw: 0.0,
            pitch: 0.0,
            fov: 75.0,
            animation_start: None,
            animation_progress: Arc::new(Mutex::new(0)),
            animation_start_distance: 0.0
        }
    }
}
