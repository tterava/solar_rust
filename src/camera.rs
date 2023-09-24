use std::f64::consts::PI;

use crate::astronomy::AU;
use crate::matrix::Matrix4d;
use crate::vector::Vector4d;

// For drawing camera is assumed to be situated on the positive side of the Z-axis at (0,0,1), with target being origin.
// Matrix operations are used to transform the simulation space into camera space.
// Camera is always kept in line with Y-axis (Y-axis is directly up). In other words camera only has pitch and yaw.

#[derive(Debug)]
pub struct Camera {
    pub target: Vector4d,
    pub distance: f64,
    yaw: f64,
    pitch: f64,
    pub fov: f64,
}

impl Camera {
    pub fn get_pitch(&self) -> f64 {
        self.pitch
    }
    pub fn add_pitch(&mut self, angle: f64) {
        self.set_pitch(self.pitch + angle);
    }
    pub fn set_pitch(&mut self, angle: f64) {
        // Limit pitch between -89.9 degrees and 89.9 degrees or things go upside down.
        self.pitch = angle.min(PI / 180.0 * 89.9).max(-PI / 180.0 * 89.9);
    }

    pub fn get_yaw(&self) -> f64 {
        self.yaw
    }
    pub fn add_yaw(&mut self, angle: f64) {
        self.set_yaw(self.yaw + angle);
    }
    pub fn set_yaw(&mut self, angle: f64) {
        self.yaw = angle % (2.0 * PI);
    }   

    pub fn get_full_transformation(&self) -> Matrix4d {
        let translation = Matrix4d::trans(&self.target);

        let rot_y = Matrix4d::rot_y(-self.yaw);
        let rot_x = Matrix4d::rot_x(-self.pitch);

        let scale_matrix = Matrix4d::scale(1.0 / self.distance);

        scale_matrix * rot_x * rot_y * translation
    }

    pub fn zoom(&mut self, amount: i32) {
        match amount {
            0.. => self.distance /= 1.1,
            _ => self.distance *= 1.1,
        };
    }

    pub fn get_position(&self) -> Vector4d {
        let transform = Matrix4d::scale(self.distance) * Matrix4d::rot_y(self.yaw) * Matrix4d::rot_x(self.pitch);
        let unit_vec = Vector4d { data: [0.0, 0.0, 1.0, 1.0] };
        let direction_vec = transform * &unit_vec;

        self.target.add(&direction_vec)
    }
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            target: Vector4d { data: [0.0, 0.0, 0.0, 1.0] },
            distance: 2.0 * AU,
            yaw: 0.0,
            pitch: 0.0,
            fov: 75.0
        }
    }
}
