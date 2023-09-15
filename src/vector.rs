#![allow(dead_code)]

#[derive(Debug, Default, Clone)]
pub struct Vector4d {
    pub data: [f64; 4],
}

impl Vector4d {
    pub fn distance(&self, other: &Vector4d) -> f64 {
        let squared_differece =
            (0..3).fold(0.0, |acc, i| acc + (self.data[i] - other.data[i]).powi(2));
        squared_differece.sqrt()
    }

    pub fn length(&self) -> f64 {
        let squares = (0..3).fold(0.0, |acc, i| acc + self.data[i].powi(2));
        println!("squares {}", squares);
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

    pub fn get_unit_vector(&self) -> Vector4d {
        let length = self.length();
        self.multiply(1.0 / length)
    }

    // https://en.wikipedia.org/wiki/Cross_product
    pub fn cross_product(&self, other: &Vector4d) -> Vector4d {
        let a = self;
        let b = other;
        Vector4d { data: [
            a.data[1] * b.data[2] - a.data[2] * b.data[1],
            a.data[0] * b.data[2] - a.data[2] * b.data[0],
            a.data[0] * b.data[1] - a.data[1] * b.data[0],
            1.0
        ] }
    }
}
 