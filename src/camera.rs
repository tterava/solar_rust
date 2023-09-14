use crate::vector::Vector4d;
use crate::matrix::Matrix4d;

// For drawing camera is assumed to be situated on the positive side of the Z-axis, with target being origin.
// Matrix operations are used to transform the simulation space into camera space.
use std::f64::consts::PI;

const AU: f64 = 149597870691.0;

#[derive(Debug)]
pub struct Camera {
    pub pos: Vector4d,
    pub target: Vector4d,
    pub fov: f64
}

impl Camera {
    pub fn default() -> Camera {
        Camera {
            pos: Vector4d { data: [0.0, 0.0, 2.0 * AU, 1.0] },
            target: Vector4d { data: [0.0, 0.0, 0.0, 1.0] },
            fov: PI / 180.0 * 75.0  // 75 degrees
        }
    }

    pub fn get_y_rotation(&self) -> f64 {
        // This gives a vector that points towards the camera as if target was the origin
        let direction_vec = self.pos.substract(&self.target);

        // Default position of camera lies on positive Z-axis, so we can remap Z -> X and X -> Y
        direction_vec.data[0].atan2(direction_vec.data[2])
    }

    pub fn get_x_rotation(&self) -> f64 {
        let direction_vec = self.pos.substract(&self.target);

        (-direction_vec.data[1]).atan2(direction_vec.data[2])
    }
}