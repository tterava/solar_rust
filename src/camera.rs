use std::f64::consts::PI;

use glam::{DAffine3, DVec3};

use crate::astronomy::AU;

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
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            target: DVec3::ZERO,
            distance: 2.0 * AU,
            yaw: 0.0,
            pitch: 0.0,
            fov: 75.0,
        }
    }
}
