use std::cmp::Ordering;
use std::f64::consts::PI;

use crate::integration::G;

use glam::{DAffine3, DVec3};
use rand::{rngs::StdRng, Rng};
use uuid::Uuid;

pub const AU: f64 = 1.495978707E11;
pub const SOLAR_MASS: f64 = 1.98847E30;
pub const SOLAR_RADIUS: f64 = 6.957E8;

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
    pub position: DVec3,
    pub velocity: DVec3,
    pub acceleration: DVec3,
    pub radius: f64,
    pub magnification: f64,
    pub color: [u8; 3],
    pub uuid: Uuid,
}

impl AstronomicalObject {
    pub fn default(rng: &mut StdRng) -> Vec<AstronomicalObject> {
        let mut system = vec![AstronomicalObject {
            name: "Sun".to_string(),
            mass: SOLAR_MASS,
            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
            acceleration: DVec3::ZERO,
            radius: SOLAR_RADIUS,
            magnification: 100.0,
            color: [255, 255, 0],
            uuid: Uuid::new_v4(),
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
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
            rng,
        ));

        // Never 4get
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
            rng,
        ));

        system.push(AstronomicalObject::place_on_orbit(
            OrbitalObject {
                name: "International Space Station".to_string(),
                mass: 450.0E3,
                radius: 100.0,
                positive_y_rotation: true,
                method: OrbitalMethod::Radius(6371.0E3 + 418000.0),
                inclination: Inclination::Fixed(51.64f64.to_radians()),
                magnification: 1.0E7,
                color: [0, 0, 160],
            },
            system.iter().find(|x| x.name == "Earth").unwrap(),
            rng,
        ));

        // system.push(AstronomicalObject {
        //     name: "Alpha Centauri A".into(),
        //     mass: 1.0788 * SOLAR_MASS,
        //     position: DVec3 {
        //         x: 4.2465 * 63241.077 * AU,
        //         y: 0.0,
        //         z: 0.0,
        //     },
        //     velocity: DVec3 {
        //         x: 0.0,
        //         y: 0.0,
        //         z: 0.0,
        //     },
        //     acceleration: DVec3::ZERO,
        //     radius: 1.2175 * SOLAR_RADIUS,
        //     magnification: 50.0,
        //     color: [255, 255, 0],
        //     uuid: Uuid::new_v4(),
        // });

        // system.push(AstronomicalObject {
        //     name: "Alpha Centauri B".into(),
        //     mass: 0.9092 * SOLAR_MASS,
        //     position: DVec3 {
        //         x: 4.2465 * 63241.077 * AU,
        //         y: 0.0,
        //         z: 17.493 * AU,
        //     },
        //     velocity: DVec3 {
        //         x: 0.0,
        //         y: 0.0,
        //         z: 0.0,
        //     },
        //     acceleration: DVec3::ZERO,
        //     radius: 0.8591 * SOLAR_RADIUS,
        //     magnification: 50.0,
        //     color: [255, 100, 0],
        //     uuid: Uuid::new_v4(),
        // });

        // system.push(AstronomicalObject {
        //     name: "Neutron star".into(),
        //     mass: 1.4 * SOLAR_MASS,
        //     position: Vector3d { data: [-0.7 * AU, 0.9 * AU, 50.0 * AU] },
        //     velocity: Vector3d { data: [0.0, 0.0, -32000.0] },
        //     acceleration: Vector3d::default(),
        //     radius: 10000.0,
        //     magnification: 1.0E7,
        //     color: [255, 255, 255],
        //     uuid: Uuid::new_v4()
        // });

        // system.push(AstronomicalObject {
        //     name: "Monster star".into(),
        //     mass: 25.4 * SOLAR_MASS,
        //     position: DVec3 { x: -0.7 * AU, y: 0.9 * AU, z: 50.0 * AU },
        //     velocity: DVec3 { x: 0.0, y: 0.0, z: -32000.0 },
        //     acceleration: DVec3::ZERO,
        //     radius: 20.0 * 695700.0E3,
        //     magnification: 10.0,
        //     color: [255, 255, 255],
        //     uuid: Uuid::new_v4()
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

    pub fn cmp(&self, other: &AstronomicalObject, target: DVec3) -> Ordering {
        let a = self.position.distance_squared(target);
        let b = other.position.distance_squared(target);

        if a < b {
            return Ordering::Less;
        } else if a > b {
            return Ordering::Greater;
        }

        Ordering::Equal
    }

    pub fn place_on_orbit(
        obj: OrbitalObject,
        target: &AstronomicalObject,
        rng: &mut StdRng,
    ) -> AstronomicalObject {
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

        let inclination_angle = match obj.inclination {
            Inclination::Fixed(a) => a,
            Inclination::Random(a) => rng.gen_range(0.0..=a),
        };

        let rot_y = DAffine3::from_rotation_y(rng.gen_range(0.0..2.0 * PI));
        let rot_z = DAffine3::from_rotation_z(inclination_angle);
        let rot_y_2 = DAffine3::from_rotation_y(rng.gen_range(0.0..2.0 * PI));
        let translate_pos = DAffine3::from_translation(target.position);
        let translate_vel = DAffine3::from_translation(target.velocity);

        let transform_pos = translate_pos * rot_y_2 * rot_z * rot_y;
        let transform_vel = translate_vel * rot_y_2 * rot_z * rot_y;

        let position = transform_pos.transform_point3(DVec3::new(0.0, 0.0, radius));
        let velocity = transform_vel.transform_point3(DVec3::new(speed, 0.0, 0.0));

        AstronomicalObject {
            name: obj.name,
            mass: obj.mass,
            position,
            velocity,
            acceleration: DVec3::ZERO,
            radius: obj.radius,
            magnification: obj.magnification,
            color: obj.color,
            uuid: Uuid::new_v4(),
        }
    }

    pub fn get_random_planet(rng: &mut StdRng) -> OrbitalObject {
        let density_earth = 5.972168E24 / 6371.0E3f64.powi(3);
        let mass = rng.gen_range(1.303E22..=6.8982E27);
        let radius = (mass / density_earth).powf(3.0_f64.recip());

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
