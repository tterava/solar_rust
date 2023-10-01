#![allow(dead_code)]

#[derive(Debug, Clone, Default)]
pub struct Vector3d {
    pub data: [f64; 3]
}

#[derive(Debug, Clone)]
pub struct Vector4d {
    pub data: [f64; 4],
}

impl Vector3d {
    pub fn distance(&self, other: &Vector3d) -> f64 {
        self.distance_squared(other).sqrt()
    }

    pub fn distance_squared(&self, other: &Vector3d) -> f64 {
        (0..3).fold(0.0, |acc, i| acc + (self.data[i] - other.data[i]).powi(2))
    }

    pub fn to_4d(&self) -> Vector4d {
        Vector4d { data: [self.data[0], self.data[1], self.data[2], 1.0] }
    }

    pub fn add(&self, other: &Vector3d) -> Vector3d {
        Vector3d {
            data: [
                self.data[0] + other.data[0],
                self.data[1] + other.data[1],
                self.data[2] + other.data[2]
            ],
        }
    }

    pub fn add_mut(&mut self, other: &Vector3d) -> &mut Self {
        self.data[0] += other.data[0];
        self.data[1] += other.data[1];
        self.data[2] += other.data[2];

        self
    }

    pub fn multiply(&self, value: f64) -> Vector3d {
        Vector3d {
            data: [
                self.data[0] * value,
                self.data[1] * value,
                self.data[2] * value
            ],
        }
    }

    pub fn multiply_mut(&mut self, value: f64) -> &mut Self {
        self.data[0] *= value;
        self.data[1] *= value;
        self.data[2] *= value;

        self
    }

    pub fn substract(&self, other: &Vector3d) -> Vector3d {
        Vector3d {
            data: [
                self.data[0] - other.data[0],
                self.data[1] - other.data[1],
                self.data[2] - other.data[2],
            ],
        }
    }

    pub fn length(&self) -> f64 {
        let squares = (0..3).fold(0.0, |acc, i| acc + self.data[i].powi(2));
        squares.sqrt()
    }

    pub fn length_squared(&self) -> f64 {
        (0..3).fold(0.0, |acc, i| acc + self.data[i].powi(2))
    }

    pub fn get_unit_vector(&self) -> Vector3d {
        let length = self.length();
        self.multiply(1.0 / length)
    }

    pub fn dot_product(&self, other: &Vector3d) -> f64 {
        (0..3).fold(0.0f64, |acc, x| acc + self.data[x] * other.data[x])
    }
}

impl Vector4d {
    pub fn to_3d(&self) -> Vector3d {
        Vector3d { data: [self.data[0], self.data[1], self.data[2]] }
    }

    pub fn distance(&self, other: &Vector4d) -> f64 {
        self.distance_squared(other).sqrt()
    }

    pub fn distance_squared(&self, other: &Vector4d) -> f64 {
        (0..3).fold(0.0, |acc, i| acc + (self.data[i] - other.data[i]).powi(2))
    }

    pub fn length(&self) -> f64 {
        let squares = (0..3).fold(0.0, |acc, i| acc + self.data[i].powi(2));
        squares.sqrt()
    }

    pub fn add(&self, other: &Vector4d) -> Vector4d {
        Vector4d {
            data: [
                self.data[0] + other.data[0],
                self.data[1] + other.data[1],
                self.data[2] + other.data[2],
                1.0,
            ],
        }
    }

    pub fn add_mut(&mut self, other: &Vector4d) -> &mut Self {
        self.data[0] += other.data[0];
        self.data[1] += other.data[1];
        self.data[2] += other.data[2];

        self
    }

    pub fn substract(&self, other: &Vector4d) -> Vector4d {
        Vector4d {
            data: [
                self.data[0] - other.data[0],
                self.data[1] - other.data[1],
                self.data[2] - other.data[2],
                1.0,
            ],
        }
    }

    pub fn substract_mut(&mut self, other: &Vector4d) -> &mut Self {
        self.data[0] -= other.data[0];
        self.data[1] -= other.data[1];
        self.data[2] -= other.data[2];

        self
    }

    pub fn multiply(&self, value: f64) -> Vector4d {
        Vector4d {
            data: [
                self.data[0] * value,
                self.data[1] * value,
                self.data[2] * value,
                1.0,
            ],
        }
    }

    pub fn multiply_mut(&mut self, value: f64) -> &mut Self {
        self.data[0] *= value;
        self.data[1] *= value;
        self.data[2] *= value;

        self
    }

    pub fn get_unit_vector(&self) -> Vector4d {
        let length = self.length();
        self.multiply(1.0 / length)
    }

    // https://en.wikipedia.org/wiki/Cross_product
    pub fn cross_product(&self, other: &Vector4d) -> Vector4d {
        let a = self;
        let b = other;
        Vector4d {
            data: [
                a.data[1] * b.data[2] - a.data[2] * b.data[1],
                a.data[0] * b.data[2] - a.data[2] * b.data[0],
                a.data[0] * b.data[1] - a.data[1] * b.data[0],
                1.0,
            ],
        }
    }
}

impl Default for Vector4d {
    fn default() -> Self {
        Vector4d {
            data: [0.0, 0.0, 0.0, 1.0],
        }
    }
}
