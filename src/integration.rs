use std::ops::Range;

use glam::DVec3;
use rayon::prelude::*;
use rayon::ThreadPool;

use crate::astronomy::{Bodies, Merge};

pub const G: f64 = 6.6743E-11;

/// Indices of two bodies that are touching
pub type Collision = (usize, usize);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntegrationMethod {
    /// Symplectic integrator of order 1-4
    Symplectic(u8),
    RK4,
}

impl IntegrationMethod {
    /// Next method in the cycle: Symplectic 1 -> 2 -> 3 -> 4 -> RK4 -> Symplectic 1
    pub fn next(self) -> Self {
        match self {
            IntegrationMethod::Symplectic(n) if n < 4 => IntegrationMethod::Symplectic(n + 1),
            IntegrationMethod::Symplectic(_) => IntegrationMethod::RK4,
            IntegrationMethod::RK4 => IntegrationMethod::Symplectic(1),
        }
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
}

impl Integrator {
    pub fn new(method: IntegrationMethod, threads: usize) -> Self {
        Integrator {
            method,
            coefficients: method.coefficients(),
            forces: Forces::new(threads),
            rk4: Rk4Scratch::default(),
        }
    }

    /// Advances one time step. Collisions are resolved by merging the bodies, and reported in `merges`.
    pub fn step(&mut self, bodies: &mut Bodies, dt: f64, merges: &mut Vec<Merge>) {
        match self.method {
            IntegrationMethod::Symplectic(_) => self.symplectic_step(bodies, dt, merges),
            IntegrationMethod::RK4 => self.rk4_step(bodies, dt, merges),
        }
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
        self.forces
            .compute(&s.pos, &bodies.mass, &bodies.radius, &mut s.a2);

        // k3 = (v + a2 * dt/2, a(x + v2 * dt/2))
        offset(&mut s.pos, x, &s.v2, half);
        offset(&mut s.v3, v, &s.a2, half);
        self.forces
            .compute(&s.pos, &bodies.mass, &bodies.radius, &mut s.a3);

        // k4 = (v + a3 * dt, a(x + v3 * dt))
        offset(&mut s.pos, x, &s.v3, dt);
        offset(&mut s.v4, v, &s.a3, dt);
        self.forces
            .compute(&s.pos, &bodies.mass, &bodies.radius, &mut s.a4);

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
fn add_scaled(target: &mut [DVec3], delta: &[DVec3], scale: f64) {
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

/// Gravitational force evaluation, either on the calling thread or spread over a thread pool
struct Forces {
    pool: Option<ThreadPool>,
    /// One acceleration buffer per thread, summed after each evaluation
    partials: Vec<Vec<DVec3>>,
}

impl Forces {
    fn new(threads: usize) -> Self {
        let pool = (threads > 1).then(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .expect("Failed to create thread pool")
        });

        Forces {
            pool,
            partials: Vec::new(),
        }
    }

    /// Updates `bodies.acceleration`, first merging any bodies that are touching
    fn update_with_merges(&mut self, bodies: &mut Bodies, merges: &mut Vec<Merge>) {
        while let Some((i, j)) = self.compute(
            &bodies.position,
            &bodies.mass,
            &bodies.radius,
            &mut bodies.acceleration,
        ) {
            merges.push(bodies.merge(i, j));
        }
    }

    /// Writes the acceleration of every body into `acc` and returns a touching pair, if any.
    fn compute(
        &mut self,
        pos: &[DVec3],
        mass: &[f64],
        radius: &[f64],
        acc: &mut [DVec3],
    ) -> Option<Collision> {
        acc.fill(DVec3::ZERO);

        let collision = match &self.pool {
            None => accumulate_pairs(0..pos.len(), pos, mass, radius, acc),
            Some(pool) => {
                // Each thread takes a block of rows with an equal share of the pairs, and adds
                // into its own buffer, since a pair also writes to the other body's row.
                let blocks = balanced_row_blocks(pos.len(), pool.current_num_threads());
                self.partials.resize_with(blocks.len(), Vec::new);

                let collision = pool.install(|| {
                    self.partials
                        .par_iter_mut()
                        .zip(blocks)
                        .filter_map(|(partial, rows)| {
                            partial.clear();
                            partial.resize(pos.len(), DVec3::ZERO);
                            accumulate_pairs(rows, pos, mass, radius, partial)
                        })
                        .min() // The first pair in row order, same as single-threaded
                });

                for partial in &self.partials {
                    add_scaled(acc, partial, 1.0);
                }
                collision
            }
        };

        // G is applied once per body instead of once per pair
        for a in acc.iter_mut() {
            *a *= G;
        }

        collision
    }
}

/// Adds the pull between body i and every later body j, for i in `rows`, into `acc`
/// (without the factor G). Each pair is visited once and applied to both bodies
/// (Newton's 3rd law). Returns the first touching pair found.
fn accumulate_pairs(
    rows: Range<usize>,
    pos: &[DVec3],
    mass: &[f64],
    radius: &[f64],
    acc: &mut [DVec3],
) -> Option<Collision> {
    let mut collision = None;

    for i in rows {
        let (pos_i, mass_i, radius_i) = (pos[i], mass[i], radius[i]);
        // Accumulate in a local so it stays in registers instead of going through memory every pair
        let mut acc_i = DVec3::ZERO;

        for j in i + 1..pos.len() {
            let difference = pos[j] - pos_i;
            let distance_sq = difference.length_squared();

            let touching_distance = radius_i + radius[j];
            if distance_sq <= touching_distance * touching_distance && collision.is_none() {
                collision = Some((i, j));
            }

            // difference / distance^3 = unit vector / distance^2
            let inv_distance = distance_sq.sqrt().recip();
            let pull = difference * (inv_distance * inv_distance * inv_distance);

            acc_i += pull * mass[j];
            acc[j] -= pull * mass_i;
        }

        acc[i] += acc_i;
    }

    collision
}

/// Splits rows 0..n into at most `blocks` contiguous ranges with roughly the same number of
/// pairs each. Row i has n - 1 - i pairs, so early blocks get fewer rows than late ones.
fn balanced_row_blocks(n: usize, blocks: usize) -> Vec<Range<usize>> {
    let total_pairs = n * n.saturating_sub(1) / 2;
    let mut ranges = Vec::with_capacity(blocks);
    let (mut start, mut pairs) = (0, 0);

    for i in 0..n {
        pairs += n - 1 - i;
        if pairs * blocks >= total_pairs * (ranges.len() + 1) {
            ranges.push(start..i + 1);
            start = i + 1;
        }
    }
    if start < n {
        ranges.push(start..n);
    }

    ranges
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;
    use crate::astronomy::{solar_system, OrbitalObject};

    fn test_system(extra_planets: usize) -> Bodies {
        let mut rng = StdRng::seed_from_u64(7);
        let mut bodies = solar_system(&mut rng);
        for _ in 0..extra_planets {
            bodies.push_orbiting(OrbitalObject::random_planet(&mut rng), 0, &mut rng);
        }
        bodies
    }

    #[test]
    fn parallel_forces_match_pairwise() {
        let bodies = test_system(200);
        let n = bodies.len();
        let (mut serial, mut parallel) = (vec![DVec3::ZERO; n], vec![DVec3::ZERO; n]);

        Forces::new(1).compute(&bodies.position, &bodies.mass, &bodies.radius, &mut serial);
        Forces::new(4).compute(
            &bodies.position,
            &bodies.mass,
            &bodies.radius,
            &mut parallel,
        );

        for (s, p) in serial.iter().zip(&parallel) {
            assert!((*s - *p).length() <= 1e-9 * s.length(), "{s} vs {p}");
        }
    }

    #[test]
    fn row_blocks_cover_every_row_once() {
        for n in [0, 1, 2, 3, 13, 100, 1000] {
            for blocks in [1, 2, 3, 12] {
                let ranges = balanced_row_blocks(n, blocks);
                let rows: Vec<usize> = ranges.iter().flat_map(|r| r.clone()).collect();
                assert_eq!(rows, (0..n).collect::<Vec<_>>(), "n={n}, blocks={blocks}");
            }
        }
    }

    #[test]
    fn collisions_found_by_both_kernels() {
        let mut bodies = test_system(0);
        bodies.position[5] = bodies.position[3]; // Put Mars on top of Earth

        for threads in [1, 4] {
            let mut acc = vec![DVec3::ZERO; bodies.len()];
            let found = Forces::new(threads).compute(
                &bodies.position,
                &bodies.mass,
                &bodies.radius,
                &mut acc,
            );
            assert_eq!(found, Some((3, 5)), "threads = {threads}");
        }
    }

    #[test]
    fn merge_conserves_mass_and_momentum() {
        let mut bodies = test_system(0);
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
        use crate::astronomy::{NewBody, AU, SOLAR_MASS, SOLAR_RADIUS};

        let year: f64 = 365.256 * 86400.0;
        let dt = 3600.0;

        for method in [
            IntegrationMethod::Symplectic(1),
            IntegrationMethod::Symplectic(2),
            IntegrationMethod::Symplectic(3),
            IntegrationMethod::Symplectic(4),
            IntegrationMethod::RK4,
        ] {
            for threads in [1, 2] {
                let mut rng = StdRng::seed_from_u64(1);
                let mut bodies = Bodies::default();
                let sun = bodies.push(NewBody {
                    name: "Sun".into(),
                    mass: SOLAR_MASS,
                    radius: SOLAR_RADIUS,
                    position: DVec3::ZERO,
                    velocity: DVec3::ZERO,
                    magnification: 1.0,
                    color: [0; 3],
                });
                let earth =
                    OrbitalObject::circular("Earth", 5.972168E24, 6371.0E3, AU, 0.0, 1.0, [0; 3]);
                bodies.push_orbiting(earth, sun, &mut rng);

                let start = bodies.position[1] - bodies.position[0];
                let mut integrator = Integrator::new(method, threads);
                let mut merges = Vec::new();

                for _ in 0..(year / dt).round() as usize {
                    integrator.step(&mut bodies, dt, &mut merges);
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
}
