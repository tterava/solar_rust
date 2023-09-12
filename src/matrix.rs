// Matrix4d[column][row]
#![allow(dead_code)]

#[derive(Debug, Default)]
pub struct Matrix4d {
    pub data: [[f64; 4]; 4],
}

impl Matrix4d {
    pub fn multiply(&self, other: &Matrix4d) -> Matrix4d {
        let mut result = Matrix4d::default();

        for column in 0..4 {
            for row in 0..4 {
                result.data[column][row] = (0..4).fold(0.0, |acc, i| {
                    acc + self.data[i][row] * other.data[column][i]
                });
            }
        }

        result
    }

    pub fn identity() -> Matrix4d {
        Matrix4d {
            data: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn multiply_vec(&self, vec: &[f64; 4]) -> [f64; 4] {
        let mut result = [0.0, 0.0, 0.0, 0.0];

        for row in 0..4 {
            result[row] = (0..4).fold(0.0, |acc, i| acc + self.data[i][row] * vec[i]);
        }

        result
    }

    pub fn rot_x(angle: f64) -> Matrix4d {
        let a_cos = angle.cos();
        let a_sin = angle.sin();

        Matrix4d {
            data: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, a_cos, a_sin, 0.0],
                [0.0, -a_sin, a_cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rot_y(angle: f64) -> Matrix4d {
        let a_cos = angle.cos();
        let a_sin = angle.sin();

        Matrix4d {
            data: [
                [a_cos, 0.0, -a_sin, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [a_sin, 0.0, a_cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rot_z(angle: f64) -> Matrix4d {
        let a_cos = angle.cos();
        let a_sin = angle.sin();

        Matrix4d {
            data: [
                [a_cos, a_sin, 0.0, 0.0],
                [-a_sin, a_cos, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn trans(pos: [f64; 3]) -> Matrix4d {
        Matrix4d {
            data: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [pos[0], pos[1], pos[2], 1.0],
            ],
        }
    }
}

use std::ops::Mul;
impl Mul<Matrix4d> for Matrix4d {
    type Output = Matrix4d;

    fn mul(self, other: Matrix4d) -> Matrix4d {
        self.multiply(&other)
    }
}

impl Mul<[f64; 4]> for Matrix4d {
    type Output = [f64; 4];

    fn mul(self, vec: [f64; 4]) -> [f64; 4] {
        self.multiply_vec(&vec)
    }
}