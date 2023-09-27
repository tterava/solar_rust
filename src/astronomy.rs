use std::cmp::Ordering;
use std::f64::consts::PI;

use crate::integration::G;
use crate::matrix::Matrix4d;
use crate::vector::{Vector3d, Vector4d};

use rand::Rng;
use uuid::Uuid;

pub const AU: f64 = 1.495978707E11;
pub const SOLAR_MASS: f64 = 1.98847E30;

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
    pub position: Vector3d,
    pub velocity: Vector3d,
    pub acceleration: Vector3d,
    pub radius: f64,
    pub magnification: f64,
    pub color: [u8; 3],
    pub uuid: Uuid
}

impl AstronomicalObject {
    pub fn default() -> Vec<AstronomicalObject> {
        let mut system = vec![AstronomicalObject {
            name: "Sun".to_string(),
            mass: SOLAR_MASS,
            position: Vector3d::default(),
            velocity: Vector3d::default(),
            acceleration: Vector3d::default(),
            radius: 695700.0E3,
            magnification: 100.0,
            color: [255, 255, 0],
            uuid: Uuid::new_v4()
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
                magnification: 1.0E7,
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
                // inclination: Inclination::Fixed(0.0f64.to_radians()),
                inclination: Inclination::Fixed(5.145f64.to_radians()),
                magnification: 1.0E7,
                color: [255, 255, 255],
            },
            system.iter().find(|x| x.name == "Earth").unwrap(),
        ));
        // system.push(AstronomicalObject::place_on_orbit(
        //     OrbitalObject {
        //         name: "Moon2".to_string(),
        //         mass: 7.342E22,
        //         radius: 1737.4E3,
        //         positive_y_rotation: false,
        //         method: OrbitalMethod::Radius(384399E3),
        //         inclination: Inclination::Fixed(0.0f64.to_radians()),
        //         // inclination: Inclination::Fixed(5.145f64.to_radians()),
        //         magnification: 1.0E7,
        //         color: [255, 255, 255],
        //     },
        //     system.iter().find(|x| x.name == "Earth").unwrap(),
        // ));
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

        // // Never 4get
        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Pluto".to_string(),
                mass: 1.303E22,
                radius: 2376.6E3,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(39.482 * AU),
                inclination: Inclination::Fixed(17.16f64.to_radians()),
                magnification: 2.0E7,
                color: [190, 190, 255],
            },
            system.iter().find(|x| x.name == "Sun").unwrap(),
        ));

        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "Internation Space Station".to_string(),
                mass: 450.0E3,
                radius: 100.0,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(6371.0E3 + 418000.0),
                inclination: Inclination::Fixed(51.64f64.to_radians()),
                magnification: 1.0E7,
                color: [0, 0, 160],
            },
            system.iter().find(|x| x.name == "Earth").unwrap(),
        ));

        // system.push(AstronomicalObject {
        //     name: "Neutron star".into(),
        //     mass: 1.4 * SOLAR_MASS,
        //     position: Vector3d { data: [-1.3 * AU, 0.0, 50.0 * AU] },
        //     velocity: Vector3d { data: [0.0, 0.0, -32000.0] },
        //     acceleration: Vector3d::default(),
        //     radius: 10000.0,
        //     magnification: 1.0E7,
        //     color: [255, 255, 255],
        // });

        // system.push(AstronomicalObject::place_on_orbit(
        //     OrbitalObject {
        //         name: "Monster".to_string(),
        //         mass: 2E31,
        //         radius: 1E9,
        //         positive_y_rotation: false,
        //         method: OrbitalMethod::Radius(0.4 * AU),
        //         inclination: Inclination::Random(90.0f64.to_radians()),
        //         magnification: 2.0E11,
        //         color: [150, 0, 150],
        //     },
        //     system.iter().find(|x| x.name == "Sun").unwrap(),
        // ));

        system
    }

    pub fn cmp(&self, other: &AstronomicalObject, target: &Vector3d) -> Ordering {
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

        let rot_y = Matrix4d::rot_y(rng.gen_range(0.0..2.0 * PI));
        let rot_z = Matrix4d::rot_z(inclination_angle);
        let rot_y_2 = Matrix4d::rot_y(rng.gen_range(0.0..2.0 * PI));
        let translate_pos = Matrix4d::trans(&target.position.to_4d().multiply(-1.0));
        let translate_vel = Matrix4d::trans(&target.velocity.to_4d().multiply(-1.0));

        let matrix_pos = translate_pos * rot_y_2 * rot_z * rot_y;
        let matrix_vel = translate_vel * rot_y_2 * rot_z * rot_y;

        velocity = matrix_vel * &velocity;
        position = matrix_pos * &position;

        AstronomicalObject {
            name: obj.name,
            mass: obj.mass,
            position: position.to_3d(),
            velocity: velocity.to_3d(),
            acceleration: Vector3d::default(),
            radius: obj.radius,
            magnification: obj.magnification,
            color: obj.color,
            uuid: Uuid::new_v4()
        }
    }

    pub fn get_random_planet() -> OrbitalObject {
        let mut rng = rand::thread_rng();

        let density_earth = 5.972168E24 / 6371.0E3f64.powi(3);
        let mass = rng.gen_range(1.303E22..=6.8982E27);
        let radius = (mass / density_earth).powf(1.0 / 3.0);

        OrbitalObject {
            name: rng.gen_range(0..=1000000).to_string(),
            mass,
            radius,
            // positive_y_rotation: rng.gen_bool(0.5),
            positive_y_rotation: true,
            method: OrbitalMethod::Radius(rng.gen_range(0.5..=20.0) * AU),
            inclination: Inclination::Random(30.0f64.to_radians()),
            magnification: 1.0E7,
            color: [
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
            ],
        }
    }
}
