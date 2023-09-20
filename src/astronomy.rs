//                 bodies.Add(new CelestialBody("Jupiter", 1.8986E27, new Vector3D(0, 5.20260 * AU, 0), new Vector3D(-13070, 0, 0), 71492, 1E6, Brushes.GhostWhite));
//                 bodies.Add(new CelestialBody("Europa", 5.799844E22, new Vector3D(0, 5.20260 * AU + 670900, 0), new Vector3D(-13070 - 13740, 0, 0), 1560.8, 1E6, Brushes.LightYellow));
//                 bodies.Add(new CelestialBody("Saturn", 5.6836E26, new Vector3D(9.554909 * AU, 0, 0), new Vector3D(0, 9690, 0), 58232, 1E6, Brushes.Gray));

//                 bodies.Add(new CelestialBody("Monster", 6E30, new Vector3D(90 * AU, 4 * AU, 0), new Vector3D(-11000, 0, 0), 10E5, 50, Brushes.Bisque));

use std::cmp::Ordering;
use std::f64::consts::PI;

use crate::engine::G;
use crate::matrix::Matrix4d;
use crate::vector::Vector4d;

use rand::Rng;

const AU: f64 = 1.495978707E11;

pub enum OrbitalMethod {
    Radius(f64),
    Speed(f64),
}

pub enum Inclination {
    Random(f64),
    Fixed(f64),
}

pub struct OrbitalObject {
    pub name: String,
    pub mass: f64,
    pub radius: f64,
    pub positive_y_rotation: bool,
    pub method: OrbitalMethod,
    pub inclination: Inclination,
    pub magnification: f64,
    pub color: [u8; 3],
}

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
        let mut system = vec![AstronomicalObject {
            name: "Sun".to_string(),
            mass: 1.9885E30,
            position: Vector4d::default(),
            velocity: Vector4d::default(),
            radius: 695700.0E3,
            magnification: 100.0,
            color: [255, 255, 0],
        }];
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Mercury".to_string(),
                mass: 3.3011E23,
                radius: 2439.7E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(0.387098 * AU),
                inclination: Inclination::Fixed(7.005f64.to_radians()),
                magnification: 2.0E7,
                color: [255, 0, 0],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Venus".to_string(),
                mass: 4.8675E24,
                radius: 6051.8E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(0.723332 * AU),
                inclination: Inclination::Fixed(3.39458f64.to_radians()),
                magnification: 2.0E7,
                color: [0, 255, 0],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Earth".to_string(),
                mass: 5.972168E24,
                radius: 6371.0E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(AU),
                inclination: Inclination::Fixed(0.0),
                magnification: 2.0E7,
                color: [0, 0, 255],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Moon".to_string(),
                mass: 7.342E22,
                radius: 1737.4E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(384399E3),
                inclination: Inclination::Fixed(5.145f64.to_radians()),
                magnification: 2.0E7,
                color: [255, 255, 255],
            },
            system.iter().find(|x| x.name == "Earth").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Mars".to_string(),
                mass: 6.4171E23,
                radius: 3389.5E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(1.52368055 * AU),
                inclination: Inclination::Fixed(1.850f64.to_radians()),
                magnification: 2.0E7,
                color: [255, 50, 0],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Jupiter".to_string(),
                mass: 1.8982E27,
                radius: 69911E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(5.2038 * AU),
                inclination: Inclination::Fixed(1.303f64.to_radians()),
                magnification: 2.0E7,
                color: [216, 202, 157],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Saturn".to_string(),
                mass: 5.6834E26,
                radius: 58232E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(9.5826 * AU),
                inclination: Inclination::Fixed(2.485f64.to_radians()),
                magnification: 2.0E7,
                color: [191, 189, 175],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Uranus".to_string(),
                mass: 8.681E25,
                radius: 25362E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(19.19126 * AU),
                inclination: Inclination::Fixed(0.773f64.to_radians()),
                magnification: 2.0E7,
                color: [209, 231, 231],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Neptune".to_string(),
                mass: 1.02413E26,
                radius: 24622E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(30.07 * AU),
                inclination: Inclination::Fixed(1.770f64.to_radians()),
                magnification: 2.0E7,
                color: [39, 70, 135],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Phobos".to_string(),
                mass: 1.0659E16,
                radius: 11.2667E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(9376E3),
                inclination: Inclination::Fixed(26.04f64.to_radians()),
                magnification: 2.0E11,
                color: [200, 200, 200],
            },
            system.iter().find(|x| x.name == "Mars").unwrap(),
        ));
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Deimos".to_string(),
                mass: 1.4762E15,
                radius: 6.2E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(23463.2E3),
                inclination: Inclination::Fixed(27.58f64.to_radians()),
                magnification: 2.0E11,
                color: [150, 150, 150],
            },
            system.iter().find(|x| x.name == "Mars").unwrap(),
        ));

        // AstronomicalObject {
        //     name: "Monster".to_string(),
        //     mass: 2E31,
        //     position: Vector4d {
        //         data: [-8.0 * AU, 2.0 * AU, 8.0 * AU, 1.0],
        //     },
        //     velocity: Vector4d {
        //         data: [60.0E3, 0.0, -60.43E3, 1.0],
        //     },
        //     radius: 695700.0E3,
        //     magnification: 500.0,
        //     color: [230, 155, 200],
        // },

        // system.iter_mut().for_each(|x| x.randomize_orbit());
        system
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

    pub fn place_on_orbit(obj: OrbitalObject, target: &AstronomicalObject) -> AstronomicalObject {
        let mut rng = rand::thread_rng();

        let (mut speed, radius);
        match obj.method {
            OrbitalMethod::Radius(r) => {
                speed = (G * target.mass / r).sqrt();
                radius = r;
            }
            OrbitalMethod::Speed(v) => {
                radius = G * target.mass / v.powi(2);
                speed = v;
            }
        };

        if !obj.positive_y_rotation {
            speed *= -1.0;
        }

        let mut position = Vector4d {
            data: [0.0, 0.0, radius, 1.0],
        };
        let mut velocity = Vector4d {
            data: [speed, 0.0, 0.0, 1.0],
        };

        let inclination_angle = match obj.inclination {
            Inclination::Fixed(a) => a,
            Inclination::Random(a) => rng.gen_range(0.0..=a),
        };

        let rot_z = Matrix4d::rot_z(inclination_angle);
        let rot_y = Matrix4d::rot_y(rng.gen_range(0.0..2.0 * PI));

        velocity = rot_z * &velocity; // Tilt the orbit

        position = rot_y * &position;
        velocity = rot_y * &velocity;

        position = target.position.add(&position);
        velocity = target.velocity.add(&velocity);

        let ret = AstronomicalObject {
            name: obj.name,
            mass: obj.mass,
            position,
            velocity,
            radius: obj.radius,
            magnification: obj.magnification,
            color: obj.color,
        };

        // println!("Placing: {}", ret.name);
        // println!("Speed: {}", ret.velocity.length());
        // println!("Distance from origin: {}", position.length());
        // println!("Inclination: {:.2} degrees", inclination_angle.to_degrees());

        ret
    }
}
