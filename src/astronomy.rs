use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DAffine3, DVec3};
use rand::{Rng, RngExt};

use crate::forces::G;

pub const AU: f64 = 1.495978707E11;

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
    pub oblateness: Option<Oblateness>,
    pub magnification: f64,
    pub color: [u8; 3],
}

/// Zonal gravity harmonics of a flattened, spinning body. Its equatorial bulge pulls a little
/// harder than a point mass would near the equator, which makes close orbits precess.
/// Only the axially symmetric (zonal) terms J2, J4 and J6 are included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oblateness {
    /// Reference radius the coefficients are defined for, m
    pub radius: f64,
    /// J2, J4, J6
    pub j: [f64; 3],
    /// North pole direction, unit vector
    pub pole: DVec3,
}

impl Oblateness {
    /// Extra acceleration (on top of the point mass pull) of a body at `r` from the oblate
    /// body's center, for the oblate body's gravitational parameter `gm`.
    pub fn acceleration(&self, gm: f64, r: DVec3) -> DVec3 {
        // Gradient of the zonal potential -GM / r * J_n (R/r)^n P_n(s), where s is the sine of
        // the latitude and P_n the Legendre polynomials:
        // a = GM / r^2 * J_n (R/r)^n * [((n + 1) P_n(s) + s P_n'(s)) r_unit - P_n'(s) pole]
        let distance = r.length();
        let unit = r / distance;
        let s = unit.dot(self.pole);
        let ratio = self.radius / distance;

        // P_n(s) and P_n'(s), starting from degree 0 and 1
        let (mut p_previous, mut p) = (1.0, s);
        let (mut dp_previous, mut dp) = (0.0, 1.0);
        let mut ratio_n = ratio;
        let mut sum = DVec3::ZERO;

        for n in 1..6 {
            // Bonnet's recursion for degree n + 1
            let n_f = n as f64;
            let p_next = ((2.0 * n_f + 1.0) * s * p - n_f * p_previous) / (n_f + 1.0);
            let dp_next = dp_previous + (2.0 * n_f + 1.0) * p;
            (p_previous, p) = (p, p_next);
            (dp_previous, dp) = (dp, dp_next);
            ratio_n *= ratio;

            let degree = n + 1;
            if degree % 2 == 0 {
                let j = self.j[degree / 2 - 1];
                sum += j * ratio_n * (((degree + 1) as f64 * p + s * dp) * unit - dp * self.pole);
            }
        }

        sum * (gm / (distance * distance))
    }
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
    pub oblateness: Vec<Option<Oblateness>>,
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
        self.oblateness.push(body.oblateness);
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

    pub fn remove(&mut self, i: usize) {
        self.position.remove(i);
        self.velocity.remove(i);
        self.acceleration.remove(i);
        self.mass.remove(i);
        self.radius.remove(i);
        self.oblateness.remove(i);
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
        self.oblateness.clone_from(&source.oblateness);
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
    /// Orbits counterclockwise seen from the north (+z), like the planets
    pub prograde: bool,
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
            prograde: true,
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
            prograde: true,
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

        if !self.prograde {
            speed *= -1.0;
        }

        let inclination_angle = match self.inclination {
            Inclination::Fixed(a) => a,
            Inclination::Random(a) => rng.random_range(0.0..=a),
        };

        // Start in the x-y (ecliptic) plane, tilt by the inclination, and turn to a random
        // direction both before and after tilting
        let rotation = DAffine3::from_rotation_z(rng.random_range(0.0..2.0 * PI))
            * DAffine3::from_rotation_x(inclination_angle)
            * DAffine3::from_rotation_z(rng.random_range(0.0..2.0 * PI));

        NewBody {
            name: self.name,
            mass: self.mass,
            radius: self.radius,
            position: parent_position + rotation.transform_vector3(DVec3::new(radius, 0.0, 0.0)),
            velocity: parent_velocity + rotation.transform_vector3(DVec3::new(0.0, speed, 0.0)),
            oblateness: None,
            magnification: self.magnification,
            color: self.color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The acceleration should be the gradient of the zonal potential
    #[test]
    fn oblateness_is_gradient_of_potential() {
        let shape = Oblateness {
            radius: 7.0E7,
            j: [1.5E-2, -6.0E-4, 3.0E-5],
            pole: DVec3::new(0.2, -0.3, 0.9).normalize(),
        };
        let gm = 1.27E17;
        let legendre = [
            |s: f64| (3.0 * s * s - 1.0) / 2.0,
            |s: f64| (35.0 * s.powi(4) - 30.0 * s * s + 3.0) / 8.0,
            |s: f64| (231.0 * s.powi(6) - 315.0 * s.powi(4) + 105.0 * s * s - 5.0) / 16.0,
        ];
        let potential = |r: DVec3| -> f64 {
            let (distance, s) = (r.length(), r.normalize().dot(shape.pole));
            (0..3)
                .map(|k| {
                    let n = 2 * k as i32 + 2;
                    -gm / distance * shape.j[k] * (shape.radius / distance).powi(n) * legendre[k](s)
                })
                .sum()
        };

        for r in [
            DVec3::new(4.2E8, 1.0E7, -3.0E7),
            DVec3::new(-1.0E8, 5.0E7, 9.0E7),
            DVec3::new(2.0E7, 3.0E7, 8.0E7),
        ] {
            let h = 1.0;
            let gradient = DVec3::new(
                potential(r + DVec3::X * h) - potential(r - DVec3::X * h),
                potential(r + DVec3::Y * h) - potential(r - DVec3::Y * h),
                potential(r + DVec3::Z * h) - potential(r - DVec3::Z * h),
            ) / (2.0 * h);
            let a = shape.acceleration(gm, r);
            assert!(
                (a - gradient).length() <= 1e-6 * a.length(),
                "{a} vs {gradient}"
            );
        }

        // On the equator the bulge pulls inwards
        let equator = shape.pole.any_orthonormal_vector() * 4.2E8;
        assert!(shape.acceleration(gm, equator).dot(equator) < 0.0);
    }
}
