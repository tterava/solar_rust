use crate::vector::Vector4d;
use crate::matrix::Matrix4d;

// For drawing camera is assumed to be situated on the positive side of the Z-axis at (0,0,1), with target being origin.
// Matrix operations are used to transform the simulation space into camera space.
// Camera is always kept in line with Y-axis (Y-axis is directly up). In other words camera only has pitch and yaw.
use std::f64::consts::PI;

#[derive(Debug)]
pub struct Camera {
    pub pos: Vector4d,
    pub target: Vector4d,
    pub fov: f64
}

impl Camera {
    pub fn get_yaw(&self) -> f64 {
        // This gives a vector that points towards the camera as if target was the origin
        let direction_vec = self.pos.substract(&self.target);

        // Default position of camera lies on positive Z-axis, so we can remap Z -> X and X -> Y
        direction_vec.data[0].atan2(direction_vec.data[2])
    }

    // Pitch is relative to the XZ-plane. 
    pub fn get_pitch(&self) -> f64 {
        let direction_vec = self.pos.substract(&self.target);

        // Length of the component that is located on XZ-plane. This is our X for atan2
        let base_xz = (direction_vec.data[0].powi(2) + direction_vec.data[2].powi(2)).sqrt();
        direction_vec.data[1].atan2(base_xz)
    }

    pub fn get_full_transformation(&self) -> Matrix4d {
        let translation = Matrix4d::trans(&self.target);
        
        let pitch = self.get_pitch();
        let yaw = self.get_yaw();

        let scale_matrix = Matrix4d::scale(1.0 / self.pos.substract(&self.target).length());

        scale_matrix * Matrix4d::rot_x(pitch) * Matrix4d::rot_y(-yaw) * translation
    }

    pub fn distance(&self) -> f64 {
        self.pos.substract(&self.target).length()
    }
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            pos: Vector4d { data: [0.0, 0.0, 1.0, 1.0] },
            target: Vector4d { data: [0.0, 0.0, 0.0, 1.0] },
            fov: PI / 180.0 * 75.0  // 75 degrees
        }
    }
}