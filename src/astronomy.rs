use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DAffine3, DVec3};
use rand::{Rng, RngExt};

use crate::integration::G;

pub const AU: f64 = 1.495978707E11;
pub const SOLAR_MASS: f64 = 1.98847E30;
pub const SOLAR_RADIUS: f64 = 6.957E8;

/// Stable identity for a body. Indices shift when bodies merge, ids don't.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BodyId(u64);

/// Data only needed for display, kept out of the arrays the integrator works on.
#[derive(Clone, Debug)]
pub struct BodyInfo {
    pub id: BodyId,
    pub name: Arc<str>,
    pub color: [u8; 3],
    pub magnification: f64,
}

/// A fully specified body, ready to be added to [`Bodies`].
#[derive(Clone, Debug)]
pub struct NewBody {
    pub name: String,
    pub mass: f64,
    pub radius: f64,
    pub position: DVec3,
    pub velocity: DVec3,
    pub magnification: f64,
    pub color: [u8; 3],
}

/// Result of two bodies colliding: `absorbed` merged into `into`.
#[derive(Clone, Debug)]
pub struct Merge {
    pub absorbed: Arc<str>,
    pub into: Arc<str>,
}

/// All bodies in struct-of-arrays layout: index `i` of every vector refers to the same body.
///
/// The integrator only streams through the physics arrays (position, velocity, mass, radius),
/// so they're packed tightly instead of being interleaved with names, colors etc.
#[derive(Default, Debug)]
pub struct Bodies {
    pub position: Vec<DVec3>,
    pub velocity: Vec<DVec3>,
    /// Acceleration from the latest force evaluation
    pub acceleration: Vec<DVec3>,
    pub mass: Vec<f64>,
    pub radius: Vec<f64>,
    pub info: Vec<BodyInfo>,
    next_id: u64,
}

impl Bodies {
    pub fn len(&self) -> usize {
        self.position.len()
    }

    pub fn is_empty(&self) -> bool {
        self.position.is_empty()
    }

    /// Adds a body and returns its index
    pub fn push(&mut self, body: NewBody) -> usize {
        let id = BodyId(self.next_id);
        self.next_id += 1;

        self.position.push(body.position);
        self.velocity.push(body.velocity);
        self.acceleration.push(DVec3::ZERO);
        self.mass.push(body.mass);
        self.radius.push(body.radius);
        self.info.push(BodyInfo {
            id,
            name: body.name.into(),
            color: body.color,
            magnification: body.magnification,
        });

        self.len() - 1
    }

    pub fn index_of(&self, id: BodyId) -> Option<usize> {
        self.info.iter().position(|info| info.id == id)
    }

    /// Places `object` on an orbit around the body at index `parent` and returns its index
    pub fn push_orbiting(
        &mut self,
        object: OrbitalObject,
        parent: usize,
        rng: &mut impl Rng,
    ) -> usize {
        let body = object.place_on_orbit(
            self.position[parent],
            self.velocity[parent],
            self.mass[parent],
            rng,
        );
        self.push(body)
    }

    /// Merges two colliding bodies into the heavier one, conserving mass and momentum.
    /// The heavier body keeps its density, so its radius grows with the cube root of the mass.
    pub fn merge(&mut self, a: usize, b: usize) -> Merge {
        let (keep, gone) = if self.mass[a] >= self.mass[b] {
            (a, b)
        } else {
            (b, a)
        };

        let (m_keep, m_gone) = (self.mass[keep], self.mass[gone]);
        let total_mass = m_keep + m_gone;

        self.velocity[keep] =
            (self.velocity[keep] * m_keep + self.velocity[gone] * m_gone) / total_mass;
        let to_center_of_mass = (self.position[gone] - self.position[keep]) * (m_gone / total_mass);
        self.position[keep] += to_center_of_mass;
        self.radius[keep] *= (total_mass / m_keep).cbrt();
        self.mass[keep] = total_mass;

        let merge = Merge {
            absorbed: self.info[gone].name.clone(),
            into: self.info[keep].name.clone(),
        };
        self.remove(gone);
        merge
    }

    fn remove(&mut self, i: usize) {
        self.position.remove(i);
        self.velocity.remove(i);
        self.acceleration.remove(i);
        self.mass.remove(i);
        self.radius.remove(i);
        self.info.remove(i);
    }

    /// Finds the body this body is orbiting: one that alone accounts for (nearly) all of its
    /// acceleration, both in direction and magnitude.
    pub fn orbital_parent(&self, child: usize) -> Option<usize> {
        const ALLOWED_ERROR: f64 = 0.1;

        let child_acc = self.acceleration[child];
        let child_acc_len = child_acc.length();
        if child_acc_len == 0.0 {
            return None;
        }
        let child_acc_unit = child_acc / child_acc_len;

        (0..self.len()).filter(|&i| i != child).find(|&i| {
            let difference = self.position[i] - self.position[child];
            let distance = difference.length();

            let alignment = difference.dot(child_acc_unit) / distance;
            let acc_caused = G * self.mass[i] / (distance * distance);

            1.0 - alignment < ALLOWED_ERROR && 1.0 - acc_caused / child_acc_len < ALLOWED_ERROR
        })
    }
}

// Derived `clone_from` would reallocate every vector. The engine publishes a copy of all bodies
// to the UI every frame, so reuse the existing allocations instead.
impl Clone for Bodies {
    fn clone(&self) -> Self {
        let mut clone = Bodies::default();
        clone.clone_from(self);
        clone
    }

    fn clone_from(&mut self, source: &Self) {
        self.position.clone_from(&source.position);
        self.velocity.clone_from(&source.velocity);
        self.acceleration.clone_from(&source.acceleration);
        self.mass.clone_from(&source.mass);
        self.radius.clone_from(&source.radius);
        self.info.clone_from(&source.info);
        self.next_id = source.next_id;
    }
}

pub enum OrbitalMethod {
    Radius(f64),
    Speed(f64),
}

pub enum Inclination {
    Random(f64),
    Fixed(f64),
}

/// Description of a body on a circular orbit, before it's placed relative to its parent
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

impl OrbitalObject {
    /// Circular, prograde orbit at a fixed distance and inclination
    pub fn circular(
        name: &str,
        mass: f64,
        radius: f64,
        orbit_radius: f64,
        inclination_deg: f64,
        magnification: f64,
        color: [u8; 3],
    ) -> Self {
        OrbitalObject {
            name: name.to_string(),
            mass,
            radius,
            positive_y_rotation: true,
            method: OrbitalMethod::Radius(orbit_radius),
            inclination: Inclination::Fixed(inclination_deg.to_radians()),
            magnification,
            color,
        }
    }

    pub fn random_planet(rng: &mut impl Rng) -> Self {
        let density_earth = 5.972168E24 / 6371.0E3f64.powi(3);
        let mass = rng.random_range(1.303E22..=6.8982E27);
        let radius = (mass / density_earth).cbrt();

        OrbitalObject {
            name: rng.random_range(0..=1000000).to_string(),
            mass,
            radius,
            positive_y_rotation: true,
            method: OrbitalMethod::Radius(rng.random_range(0.5..=20.0) * AU),
            inclination: Inclination::Random(30.0f64.to_radians()),
            magnification: 1.0E7,
            color: [
                rng.random_range(0..=255),
                rng.random_range(0..=255),
                rng.random_range(0..=255),
            ],
        }
    }

    /// Places the object on its orbit around a parent, with a random orientation
    pub fn place_on_orbit(
        self,
        parent_position: DVec3,
        parent_velocity: DVec3,
        parent_mass: f64,
        rng: &mut impl Rng,
    ) -> NewBody {
        let (mut speed, radius) = match self.method {
            OrbitalMethod::Radius(r) => ((G * parent_mass / r).sqrt(), r),
            OrbitalMethod::Speed(v) => (v, G * parent_mass / v.powi(2)),
        };

        if !self.positive_y_rotation {
            speed *= -1.0;
        }

        let inclination_angle = match self.inclination {
            Inclination::Fixed(a) => a,
            Inclination::Random(a) => rng.random_range(0.0..=a),
        };

        let rot_y = DAffine3::from_rotation_y(rng.random_range(0.0..2.0 * PI));
        let rot_z = DAffine3::from_rotation_z(inclination_angle);
        let rot_y_2 = DAffine3::from_rotation_y(rng.random_range(0.0..2.0 * PI));
        let rotation = rot_y_2 * rot_z * rot_y;

        NewBody {
            name: self.name,
            mass: self.mass,
            radius: self.radius,
            position: parent_position + rotation.transform_vector3(DVec3::new(0.0, 0.0, radius)),
            velocity: parent_velocity + rotation.transform_vector3(DVec3::new(speed, 0.0, 0.0)),
            magnification: self.magnification,
            color: self.color,
        }
    }
}

/// The solar system with major planets, a few moons, Pluto and the ISS.
pub fn solar_system(rng: &mut impl Rng) -> Bodies {
    let mut bodies = Bodies::default();

    let sun = bodies.push(NewBody {
        name: "Sun".to_string(),
        mass: SOLAR_MASS,
        radius: SOLAR_RADIUS,
        position: DVec3::ZERO,
        velocity: DVec3::ZERO,
        magnification: 100.0,
        color: [255, 255, 0],
    });
    add_orbiting_bodies(&mut bodies, sun, rng);

    bodies
}

/// name, mass (kg), radius (m), orbit radius (m), inclination (degrees), magnification, color.
/// The order matters: each placement consumes random numbers for the orbit orientation.
#[rustfmt::skip]
fn add_orbiting_bodies(bodies: &mut Bodies, sun: usize, rng: &mut impl Rng) {
    use OrbitalObject as O;

    bodies.push_orbiting(O::circular("Mercury", 3.3011E23, 2439.7E3, 0.387098 * AU, 7.005, 2.0E7, [255, 0, 0]), sun, rng);
    bodies.push_orbiting(O::circular("Venus", 4.8675E24, 6051.8E3, 0.723332 * AU, 3.39458, 2.0E7, [0, 255, 0]), sun, rng);
    let earth = bodies.push_orbiting(O::circular("Earth", 5.972168E24, 6371.0E3, AU, 0.0, 1.0E7, [0, 0, 255]), sun, rng);
    bodies.push_orbiting(O::circular("Moon", 7.342E22, 1737.4E3, 384399E3, 5.145, 1.0E7, [255, 255, 255]), earth, rng);
    let mars = bodies.push_orbiting(O::circular("Mars", 6.4171E23, 3389.5E3, 1.52368055 * AU, 1.850, 2.0E7, [255, 50, 0]), sun, rng);
    bodies.push_orbiting(O::circular("Jupiter", 1.8982E27, 69911E3, 5.2038 * AU, 1.303, 2.0E7, [216, 202, 157]), sun, rng);
    bodies.push_orbiting(O::circular("Saturn", 5.6834E26, 58232E3, 9.5826 * AU, 2.485, 2.0E7, [191, 189, 175]), sun, rng);
    bodies.push_orbiting(O::circular("Uranus", 8.681E25, 25362E3, 19.19126 * AU, 0.773, 2.0E7, [209, 231, 231]), sun, rng);
    bodies.push_orbiting(O::circular("Neptune", 1.02413E26, 24622E3, 30.07 * AU, 1.770, 2.0E7, [39, 70, 135]), sun, rng);
    bodies.push_orbiting(O::circular("Phobos", 1.0659E16, 11.2667E3, 9376E3, 26.04, 2.0E11, [200, 200, 200]), mars, rng);
    bodies.push_orbiting(O::circular("Deimos", 1.4762E15, 6.2E3, 23463.2E3, 27.58, 2.0E11, [150, 150, 150]), mars, rng);
    // Never 4get
    bodies.push_orbiting(O::circular("Pluto", 1.303E22, 2376.6E3, 39.482 * AU, 17.16, 2.0E7, [190, 190, 255]), sun, rng);
    bodies.push_orbiting(O::circular("International Space Station", 450.0E3, 100.0, 6371.0E3 + 418000.0, 51.64, 1.0E7, [0, 0, 160]), earth, rng);
}
