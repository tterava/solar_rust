use glam::DVec3;

use crate::astronomy::{Bodies, Merge};
use crate::forces::{Forces, Physics};
use crate::ias15::Ias15;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntegrationMethod {
    /// Symplectic integrator of order 1-4
    Symplectic(u8),
    RK4,
    /// 15th order adaptive step integrator, accurate to the limits of double precision
    Ias15,
}

impl IntegrationMethod {
    /// Next method in the cycle: Symplectic 1 -> 2 -> 3 -> 4 -> RK4 -> IAS15 -> Symplectic 1
    pub fn next(self) -> Self {
        match self {
            IntegrationMethod::Symplectic(n) if n < 4 => IntegrationMethod::Symplectic(n + 1),
            IntegrationMethod::Symplectic(_) => IntegrationMethod::RK4,
            IntegrationMethod::RK4 => IntegrationMethod::Ias15,
            IntegrationMethod::Ias15 => IntegrationMethod::Symplectic(1),
        }
    }

    /// Whether the method chooses its own time steps
    pub fn is_adaptive(self) -> bool {
        self == IntegrationMethod::Ias15
    }

    /// (c, d) coefficients for the symplectic integrator:
    /// https://en.wikipedia.org/wiki/Symplectic_integrator
    fn coefficients(self) -> Vec<(f64, f64)> {
        match self {
            IntegrationMethod::Symplectic(1) => vec![(1.0, 1.0)],
            IntegrationMethod::Symplectic(2) => vec![(0.0, 0.5), (1.0, 0.5)],
            IntegrationMethod::Symplectic(3) => vec![
                (1.0, -1.0 / 24.0),
                (-2.0 / 3.0, 3.0 / 4.0),
                (2.0 / 3.0, 7.0 / 24.0),
            ],
            IntegrationMethod::Symplectic(4) => {
                let cbrt2 = 2.0f64.cbrt();
                let w = 1.0 / (2.0 - cbrt2);
                vec![
                    (w / 2.0, w),
                    ((1.0 - cbrt2) * w / 2.0, -cbrt2 * w),
                    ((1.0 - cbrt2) * w / 2.0, w),
                    (w / 2.0, 0.0),
                ]
            }
            _ => vec![],
        }
    }
}

/// Advances the simulation one time step at a time with the chosen method,
/// on one thread or a pool of them.
pub struct Integrator {
    method: IntegrationMethod,
    coefficients: Vec<(f64, f64)>,
    forces: Forces,
    rk4: Rk4Scratch,
    ias15: Ias15,
}

impl Integrator {
    pub fn new(method: IntegrationMethod, threads: usize, physics: Physics) -> Self {
        Integrator {
            method,
            coefficients: method.coefficients(),
            forces: Forces::new(threads, physics),
            rk4: Rk4Scratch::default(),
            ias15: Ias15::default(),
        }
    }

    /// Advances one time step. Collisions are resolved by merging the bodies, and reported in `merges`.
    /// Returns the time step taken: `max_dt` for fixed step methods, at most that for adaptive ones.
    pub fn step(&mut self, bodies: &mut Bodies, max_dt: f64, merges: &mut Vec<Merge>) -> f64 {
        match self.method {
            IntegrationMethod::Symplectic(_) => self.symplectic_step(bodies, max_dt, merges),
            IntegrationMethod::RK4 => self.rk4_step(bodies, max_dt, merges),
            IntegrationMethod::Ias15 => {
                return self.ias15.step(&mut self.forces, bodies, max_dt, merges);
            }
        }
        max_dt
    }

    fn symplectic_step(&mut self, bodies: &mut Bodies, dt: f64, merges: &mut Vec<Merge>) {
        for &(c, d) in &self.coefficients {
            if c != 0.0 {
                add_scaled(&mut bodies.position, &bodies.velocity, c * dt);
            }
            // Skipping zero coefficients saves a full force evaluation per step on the 4th order method
            if d != 0.0 {
                self.forces.update_with_merges(bodies, merges);
                add_scaled(&mut bodies.velocity, &bodies.acceleration, d * dt);
            }
        }
    }

    fn rk4_step(&mut self, bodies: &mut Bodies, dt: f64, merges: &mut Vec<Merge>) {
        // k1 = (v, a(x)). Collisions are resolved here; the trial positions below only sample forces.
        self.forces.update_with_merges(bodies, merges);

        let s = &mut self.rk4;
        s.resize(bodies.len());
        let (x, v, a) = (&bodies.position, &bodies.velocity, &bodies.acceleration);
        let half = dt / 2.0;

        // k2 = (v + a1 * dt/2, a(x + v * dt/2))
        offset(&mut s.pos, x, v, half);
        offset(&mut s.v2, v, a, half);
        self.forces.compute(bodies, &s.pos, &s.v2, &mut s.a2);

        // k3 = (v + a2 * dt/2, a(x + v2 * dt/2))
        offset(&mut s.pos, x, &s.v2, half);
        offset(&mut s.v3, v, &s.a2, half);
        self.forces.compute(bodies, &s.pos, &s.v3, &mut s.a3);

        // k4 = (v + a3 * dt, a(x + v3 * dt))
        offset(&mut s.pos, x, &s.v3, dt);
        offset(&mut s.v4, v, &s.a3, dt);
        self.forces.compute(bodies, &s.pos, &s.v4, &mut s.a4);

        for i in 0..bodies.len() {
            let dxdt = (bodies.velocity[i] + 2.0 * (s.v2[i] + s.v3[i]) + s.v4[i]) / 6.0;
            let dvdt = (bodies.acceleration[i] + 2.0 * (s.a2[i] + s.a3[i]) + s.a4[i]) / 6.0;

            bodies.position[i] += dxdt * dt;
            bodies.velocity[i] += dvdt * dt;
            bodies.acceleration[i] = dvdt;
        }
    }
}

/// Reusable buffers for the intermediate RK4 stages
#[derive(Default)]
struct Rk4Scratch {
    pos: Vec<DVec3>,
    v2: Vec<DVec3>,
    v3: Vec<DVec3>,
    v4: Vec<DVec3>,
    a2: Vec<DVec3>,
    a3: Vec<DVec3>,
    a4: Vec<DVec3>,
}

impl Rk4Scratch {
    fn resize(&mut self, n: usize) {
        for buffer in [
            &mut self.pos,
            &mut self.v2,
            &mut self.v3,
            &mut self.v4,
            &mut self.a2,
            &mut self.a3,
            &mut self.a4,
        ] {
            buffer.resize(n, DVec3::ZERO);
        }
    }
}

/// target[i] += delta[i] * scale
pub(crate) fn add_scaled(target: &mut [DVec3], delta: &[DVec3], scale: f64) {
    for (t, d) in target.iter_mut().zip(delta) {
        *t += *d * scale;
    }
}

/// out[i] = base[i] + delta[i] * scale
fn offset(out: &mut [DVec3], base: &[DVec3], delta: &[DVec3], scale: f64) {
    for ((o, b), d) in out.iter_mut().zip(base).zip(delta) {
        *o = *b + *d * scale;
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;
    use crate::astronomy::{NewBody, OrbitalObject, AU};
    use crate::ephemeris::solar_system;

    #[test]
    fn merge_conserves_mass_and_momentum() {
        let mut bodies = solar_system();
        let total_mass: f64 = bodies.mass.iter().sum();
        let momentum =
            |b: &Bodies| -> DVec3 { b.velocity.iter().zip(&b.mass).map(|(v, m)| *v * *m).sum() };
        let p_before = momentum(&bodies);
        let earth_radius = bodies.radius[3];

        let merge = bodies.merge(3, 4); // Earth absorbs the Moon
        assert_eq!((&*merge.absorbed, &*merge.into), ("Moon", "Earth"));
        assert!(bodies.radius[3] > earth_radius);
        assert!((bodies.mass.iter().sum::<f64>() - total_mass).abs() <= 1e-12 * total_mass);
        assert!((momentum(&bodies) - p_before).length() <= 1e-9 * p_before.length());
    }

    /// Sun + Earth only: the Earth should come back to where it started after one year,
    /// on every method and thread count
    #[test]
    fn earth_completes_an_orbit() {
        let year: f64 = 365.256 * 86400.0;
        let dt: f64 = 3600.0;

        for method in [
            IntegrationMethod::Symplectic(1),
            IntegrationMethod::Symplectic(2),
            IntegrationMethod::Symplectic(3),
            IntegrationMethod::Symplectic(4),
            IntegrationMethod::RK4,
            IntegrationMethod::Ias15,
        ] {
            for threads in [1, 2] {
                let mut rng = rand::rngs::StdRng::seed_from_u64(1);
                let mut bodies = Bodies::default();
                let sun = bodies.push(NewBody {
                    name: "Sun".into(),
                    mass: 1.98847E30,
                    radius: 6.957E8,
                    position: DVec3::ZERO,
                    velocity: DVec3::ZERO,
                    oblateness: None,
                    magnification: 1.0,
                    color: [0; 3],
                    emission: Default::default(),
                });
                let earth =
                    OrbitalObject::circular("Earth", 5.972168E24, 6371.0E3, AU, 0.0, 1.0, [0; 3]);
                bodies.push_orbiting(earth, sun, &mut rng);

                let start = bodies.position[1] - bodies.position[0];
                let mut integrator = Integrator::new(method, threads, Physics::NEWTONIAN);
                let mut merges = Vec::new();

                let mut time = 0.0;
                while time < year {
                    time += integrator.step(&mut bodies, dt.min(year - time), &mut merges);
                }

                let end = bodies.position[1] - bodies.position[0];
                let error = (end - start).length() / start.length();
                assert!(
                    error < 2e-3,
                    "{method:?}, {threads} threads: relative error {error}"
                );
                assert!(merges.is_empty());
            }
        }
    }

    /// Two equal masses on a circular orbit in a strong field (GM / (r c^2) = 10^-3). With
    /// relativity, a circular orbit needs the post-Newtonian angular velocity
    /// w^2 = GM / r^3 * (1 - (3 - eta) GM / (r c^2)), eta = m1 m2 / M^2 (harmonic coordinates).
    /// Started with it the separation stays constant; started with the Newtonian one it doesn't.
    #[test]
    fn relativistic_binary_stays_circular() {
        use crate::forces::{C, G};

        let gm: f64 = 1.0E20;
        let strength = 1.0E-3;
        let separation = gm / (C * C * strength);
        let newtonian_w = (gm / separation.powi(3)).sqrt();
        let relativistic_w = newtonian_w * (1.0 - (3.0 - 0.25) * strength).sqrt();

        let max_deviation = |w: f64| -> f64 {
            let mut bodies = Bodies::default();
            for side in [1.0, -1.0] {
                bodies.push(NewBody {
                    name: String::new(),
                    mass: gm / 2.0 / G,
                    radius: 1.0,
                    position: DVec3::new(side * separation / 2.0, 0.0, 0.0),
                    velocity: DVec3::new(0.0, side * w * separation / 2.0, 0.0),
                    oblateness: None,
                    magnification: 1.0,
                    color: [0; 3],
                    emission: Default::default(),
                });
            }
            let physics = Physics {
                oblateness: false,
                relativity: true,
            };
            let mut integrator = Integrator::new(IntegrationMethod::Ias15, 1, physics);
            let period = 2.0 * std::f64::consts::PI / w;

            let (mut time, mut deviation, mut merges) = (0.0, 0.0f64, Vec::new());
            while time < 5.0 * period {
                time += integrator.step(&mut bodies, period / 50.0, &mut merges);
                let r = bodies.position[0].distance(bodies.position[1]);
                deviation = deviation.max((r / separation - 1.0).abs());
            }
            deviation
        };

        let relativistic = max_deviation(relativistic_w);
        let newtonian = max_deviation(newtonian_w);
        // What's left is the second order term, about 9 (GM / (r c^2))^2 = 9e-6
        assert!(relativistic < 3e-5, "{relativistic}");
        assert!(newtonian > 1e-3, "{newtonian}");
    }
}
