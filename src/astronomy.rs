//                 bodies.Add(new CelestialBody("Jupiter", 1.8986E27, new Vector3D(0, 5.20260 * AU, 0), new Vector3D(-13070, 0, 0), 71492, 1E6, Brushes.GhostWhite));
//                 bodies.Add(new CelestialBody("Europa", 5.799844E22, new Vector3D(0, 5.20260 * AU + 670900, 0), new Vector3D(-13070 - 13740, 0, 0), 1560.8, 1E6, Brushes.LightYellow));
//                 bodies.Add(new CelestialBody("Saturn", 5.6836E26, new Vector3D(9.554909 * AU, 0, 0), new Vector3D(0, 9690, 0), 58232, 1E6, Brushes.Gray));

//                 bodies.Add(new CelestialBody("Monster", 6E30, new Vector3D(90 * AU, 4 * AU, 0), new Vector3D(-11000, 0, 0), 10E5, 50, Brushes.Bisque));

use std::cmp::Ordering;

use crate::camera::Camera;
use crate::matrix::Matrix4d;
use crate::vector::Vector4d;

use rand::Rng;

const AU: f64 = 1.495978707E11;

#[derive(Debug, Clone)]
pub struct AstronomicalObject {
    pub name: String,
    pub mass: f64,
    pub position: Vector4d,
    pub velocity: Vector4d,
    pub radius: f64,
    pub magnification: f64,
    pub color: [u8; 3],
}

impl AstronomicalObject {
    pub fn default() -> Vec<AstronomicalObject> {
        let mut system = vec![
            AstronomicalObject {
                name: "Sun".to_string(),
                mass: 1.9885E30,
                position: Vector4d::default(),
                velocity: Vector4d::default(),
                radius: 695700.0E3,
                magnification: 100.0,
                color: [255, 255, 0],
            },
            // AstronomicalObject {
            //     name: "Sun".to_string(),
            //     mass: 1.9885E30,
            //     position: Vector4d {
            //         data: [0.0, 0.0, -57.91E9, 1.0],
            //     },
            //     velocity: Vector4d {
            //         data: [-47.36E3, 0.0, -11.36E3, 1.0],
            //     },
            //     radius: 695700.0E3,
            //     magnification: 100.0,
            //     color: [255, 255, 0],
            // },
            AstronomicalObject {
                name: "Mercury".to_string(),
                mass: 3.3011E23,
                position: Vector4d {
                    data: [57.91E9, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -47.36E3, 1.0],
                },
                radius: 2439.7E3,
                magnification: 2.0E7,
                color: [255, 0, 0],
            },
            AstronomicalObject {
                name: "Venus".to_string(),
                mass: 4.8675E24,
                position: Vector4d {
                    data: [108.21E9, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -35.02E3, 1.0],
                },
                radius: 6051.8E3,
                magnification: 2.0E7,
                color: [0, 255, 0],
            },
            AstronomicalObject {
                name: "Earth".to_string(),
                mass: 5.972168E24,
                position: Vector4d {
                    data: [149598023.0E3, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -29.7827E3, 1.0],
                },
                radius: 6371.0E3,
                magnification: 2.0E7,
                color: [0, 0, 255],
            },
            AstronomicalObject {
                name: "Moon".to_string(),
                mass: 7.342E22,
                position: Vector4d {
                    data: [149598023.0E3 + 384399.0E3, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -29.7827E3 - 1.022E3, 1.0],
                },
                radius: 1736.0E3,
                magnification: 2.0E7,
                color: [255, 255, 255],
            },
            AstronomicalObject {
                name: "Mars".to_string(),
                mass: 6.4171E23,
                position: Vector4d {
                    data: [227939366.0E3, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -24.07E3, 1.0],
                },
                radius: 3389.5E3,
                magnification: 2.0E7,
                color: [255, 50, 0],
            },
            AstronomicalObject {
                name: "Jupiter".to_string(),
                mass: 1.8982E27,
                position: Vector4d {
                    data: [778.479E9, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -13.07E3, 1.0],
                },
                radius: 69911.0E3,
                magnification: 2.0E7,
                color: [216, 202, 157],
            },
            AstronomicalObject {
                name: "Saturn".to_string(),
                mass: 5.6834E26,
                position: Vector4d {
                    data: [1433.53E9, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -9.68E3, 1.0],
                },
                radius: 58232.0E3,
                magnification: 2.0E7,
                color: [191, 189, 175],
            },
            AstronomicalObject {
                name: "Uranus".to_string(),
                mass: 8.6810E25,
                position: Vector4d {
                    data: [19.191 * AU, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -6.8E3, 1.0],
                },
                radius: 25362E3,
                magnification: 2.0E7,
                color: [209,231,231],
            },
            AstronomicalObject {
                name: "Neptune".to_string(),
                mass: 1.02413E26,
                position: Vector4d {
                    data: [30.07 * AU, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 0.0, -5.43E3, 1.0],
                },
                radius: 24622E3,
                magnification: 2.0E7,
                color: [39,70,135],
            },

        ];

        // system.iter_mut().for_each(|x| x.randomize_orbit());
        system
    }

    fn randomize_orbit(&mut self) {
        let mut rng = rand::thread_rng();
        let rotation = rng.gen::<f64>() * std::f64::consts::PI * 2.0;
        let matrix = Matrix4d::rot_z(rotation);

        self.position = matrix * &self.position;
        self.velocity = matrix * &self.velocity;
    }

    pub fn cmp(&self, other: &AstronomicalObject, target: &Vector4d) -> Ordering {
        let a = self.position.distance_squared(target);
        let b = other.position.distance_squared(target);

        if a < b {
            return Ordering::Less;
        } else if a > b {
            return Ordering::Greater;
        }

        Ordering::Equal
    }
}
