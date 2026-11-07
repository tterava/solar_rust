//! The force model: Newtonian gravity between point masses, plus optionally the planets'
//! oblateness and general relativity.

use std::ops::Range;

use glam::DVec3;
use rayon::prelude::*;
use rayon::ThreadPool;
use wide::f64x4;

use crate::astronomy::{Bodies, Merge};

/// Gravitational constant, m^3 / (kg s^2)
pub const G: f64 = 6.6743E-11;
/// Speed of light, m/s
pub const C: f64 = 299_792_458.0;

/// Indices of two bodies that are touching
pub type Collision = (usize, usize);

/// Distance in reference radii beyond which oblateness is ignored. At 1000 radii the largest J2
/// (Saturn's, 0.016) times (R/r)^2 = 10^-6 adds about 10^-8 to the oblate body's pull, which is
/// itself tiny there compared to the Sun's.
const OBLATENESS_RANGE: f64 = 1000.0;

/// Effects included on top of Newtonian gravity between point masses
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Physics {
    /// Pull of the planets' (and the Sun's) equatorial bulges, see [`crate::astronomy::Oblateness`]
    pub oblateness: bool,
    /// First order post-Newtonian corrections from general relativity
    pub relativity: bool,
}

impl Physics {
    pub const NEWTONIAN: Physics = Physics {
        oblateness: false,
        relativity: false,
    };
    pub const FULL: Physics = Physics {
        oblateness: true,
        relativity: true,
    };
}

/// Force evaluation, either on the calling thread or spread over a thread pool
pub(crate) struct Forces {
    pool: Option<ThreadPool>,
    physics: Physics,
    input: KernelInput,
    /// One acceleration buffer per thread, summed after each evaluation
    partials: Vec<Columns>,
    /// Scratch for relativity: its inputs, and its outputs with a buffer per thread
    relativity: RelativityInput,
    potential_partials: Vec<[Vec<f64>; 1]>,
    corrections: [Vec<f64>; 3],
    correction_partials: Vec<[Vec<f64>; 3]>,
}

impl Forces {
    pub fn new(threads: usize, physics: Physics) -> Self {
        let pool = (threads > 1).then(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .expect("Failed to create thread pool")
        });

        Forces {
            pool,
            physics,
            input: KernelInput::default(),
            partials: Vec::new(),
            relativity: RelativityInput::default(),
            potential_partials: Vec::new(),
            corrections: Default::default(),
            correction_partials: Vec::new(),
        }
    }

    /// Updates `bodies.acceleration`, first merging any bodies that are touching
    pub fn update_with_merges(&mut self, bodies: &mut Bodies, merges: &mut Vec<Merge>) {
        loop {
            let mut acc = std::mem::take(&mut bodies.acceleration);
            let collision = self.compute(bodies, &bodies.position, &bodies.velocity, &mut acc);
            bodies.acceleration = acc;

            match collision {
                Some((i, j)) => merges.push(bodies.merge(i, j)),
                None => return,
            }
        }
    }

    /// Writes the acceleration of every body into `acc`, for the bodies at positions `pos`
    /// moving at `vel`, and returns a touching pair, if any. Masses, radii and shapes come
    /// from `bodies`.
    pub fn compute(
        &mut self,
        bodies: &Bodies,
        pos: &[DVec3],
        vel: &[DVec3],
        acc: &mut [DVec3],
    ) -> Option<Collision> {
        let collision = self.newtonian(pos, &bodies.mass, &bodies.radius, acc);

        if self.physics.oblateness {
            add_oblateness(bodies, pos, acc);
        }
        if self.physics.relativity {
            self.add_relativity(vel, acc);
        }

        collision
    }

    fn newtonian(
        &mut self,
        pos: &[DVec3],
        mass: &[f64],
        radius: &[f64],
        acc: &mut [DVec3],
    ) -> Option<Collision> {
        let n = pos.len();
        self.input.load(pos, mass, radius);
        let input = &self.input;

        match &self.pool {
            None => {
                self.partials.resize_with(1, Columns::default);
                let collision = accumulate_pairs(0..n, input, &mut self.partials[0]);
                sum_partials(&self.partials, 0, acc);
                collision
            }
            Some(pool) => pool.install(|| {
                // Each thread takes a block of rows with an equal share of the pairs, and adds
                // into its own buffer, since a pair also writes to the other body's row.
                let blocks = balanced_row_blocks(n, pool.current_num_threads());
                self.partials.resize_with(blocks.len(), Columns::default);

                let collision = self
                    .partials
                    .par_iter_mut()
                    .zip(blocks)
                    .filter_map(|(partial, rows)| accumulate_pairs(rows, input, partial))
                    .min(); // The first pair in row order, same as single-threaded

                // The buffers are summed in parallel too: they're in other cores' caches,
                // which makes this slow enough to matter on one thread
                let partials = &self.partials;
                acc.par_chunks_mut(SUM_CHUNK)
                    .enumerate()
                    .for_each(|(c, chunk)| sum_partials(partials, c * SUM_CHUNK, chunk));

                collision
            }),
        }
    }

    /// Adds the first order post-Newtonian (1PN) corrections of general relativity: the
    /// Einstein-Infeld-Hoffmann equations, which JPL's planetary ephemerides are integrated with
    /// (Park et al. 2021, "The JPL Planetary and Lunar Ephemerides DE440 and DE441", AJ 161:105),
    /// with the PPN parameters beta = gamma = 1.
    ///
    /// `acc` must hold the accelerations without relativity, which stand in for the true
    /// accelerations inside the correction terms (the difference is of second order). Positions
    /// and masses come from `self.input`, so this has to run after [`Self::newtonian`].
    fn add_relativity(&mut self, vel: &[DVec3], acc: &mut [DVec3]) {
        let n = acc.len();
        let input = &self.input;
        let relativity = &mut self.relativity;
        relativity.load(vel, acc);

        // Newtonian potential at each body: G times the sum of m / r over the other bodies
        let potential = &mut relativity.potential;
        pairwise(
            &self.pool,
            n,
            &mut self.potential_partials,
            potential,
            |rows, [out]| accumulate_potential(rows, input, out),
        );
        let [potential] = potential;
        potential.iter_mut().for_each(|p| *p *= G);
        potential[n..].fill(0.0);

        let relativity = &self.relativity;
        pairwise(
            &self.pool,
            n,
            &mut self.correction_partials,
            &mut self.corrections,
            |rows, out| accumulate_corrections(rows, input, relativity, out),
        );

        let [x, y, z] = &self.corrections;
        for (i, a) in acc.iter_mut().enumerate() {
            *a += G / (C * C) * DVec3::new(x[i], y[i], z[i]);
        }
    }
}

/// Adds the pull of every oblate body's equatorial bulge on all the other bodies, and the
/// equal and opposite pull back on the oblate body itself.
///
/// Bodies farther than [`OBLATENESS_RANGE`] reference radii are skipped.
fn add_oblateness(bodies: &Bodies, pos: &[DVec3], acc: &mut [DVec3]) {
    let mass = &bodies.mass;

    for (p, shape) in bodies.oblateness.iter().enumerate() {
        let Some(shape) = shape else { continue };
        let gm = G * mass[p];
        let range_sq = (OBLATENESS_RANGE * shape.radius).powi(2);

        for i in (0..pos.len()).filter(|&i| i != p) {
            let r = pos[i] - pos[p];
            if r.length_squared() > range_sq {
                continue;
            }
            let pull = shape.acceleration(gm, r);
            acc[i] += pull;
            acc[p] -= pull * (mass[i] / mass[p]);
        }
    }
}

/// Runs `kernel(rows, out)` over the rows 0..n, where row i visits the pairs (i, j > i) and may
/// add to any element of the K output columns, which are padded like [`KernelInput`]. With a
/// thread pool, each thread takes a block of rows with an equal share of the pairs and adds into
/// its own buffer, and the buffers are summed into `out`.
fn pairwise<const K: usize>(
    pool: &Option<ThreadPool>,
    n: usize,
    partials: &mut Vec<[Vec<f64>; K]>,
    out: &mut [Vec<f64>; K],
    kernel: impl Fn(Range<usize>, &mut [Vec<f64>; K]) + Sync,
) {
    let padded = n + LANES - 1;
    let reset = |columns: &mut [Vec<f64>; K]| {
        for column in columns {
            column.clear();
            column.resize(padded, 0.0);
        }
    };
    reset(out);

    let Some(pool) = pool else {
        kernel(0..n, out);
        return;
    };
    pool.install(|| {
        let blocks = balanced_row_blocks(n, pool.current_num_threads());
        partials.resize_with(blocks.len(), || std::array::from_fn(|_| Vec::new()));
        partials
            .par_iter_mut()
            .zip(blocks)
            .for_each(|(partial, rows)| {
                reset(partial);
                kernel(rows, partial);
            });

        let partials = &*partials;
        for (c, column) in out.iter_mut().enumerate() {
            column
                .par_chunks_mut(SUM_CHUNK)
                .enumerate()
                .for_each(|(k, chunk)| {
                    for (i, value) in (k * SUM_CHUNK..).zip(chunk) {
                        *value = partials.iter().map(|p| p[c][i]).sum();
                    }
                });
        }
    });
}

/// acc[k] = G * the sum of the partial accelerations of body `first + k`.
/// G is applied once per body instead of once per pair.
fn sum_partials(partials: &[Columns], first: usize, acc: &mut [DVec3]) {
    for (i, a) in (first..).zip(acc) {
        *a = G * partials.iter().map(|p| p.get(i)).sum::<DVec3>();
    }
}

/// Bodies per SIMD register
const LANES: usize = 4;

/// Bodies per task when summing the threads' buffers in parallel
const SUM_CHUNK: usize = 64;

/// Coordinate of the dummy bodies that pad the kernels' input. It's far enough that the pull
/// of a dummy is negligible even before it's multiplied by its zero mass, but not so far that the
/// relativity terms overflow, which would turn 0 * inf into NaN.
const FAR: f64 = 1E100;

/// One array per vector component, so the values of 4 neighbouring bodies load straight
/// into a SIMD register.
#[derive(Default)]
struct Columns {
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
}

impl Columns {
    fn get(&self, i: usize) -> DVec3 {
        DVec3::new(self.x[i], self.y[i], self.z[i])
    }

    fn fill(&mut self, len: usize, value: f64) {
        for column in [&mut self.x, &mut self.y, &mut self.z] {
            column.clear();
            column.resize(len, value);
        }
    }

    /// Copies `values`, padded with `pad` up to `len`
    fn load(&mut self, values: &[DVec3], len: usize, pad: f64) {
        self.fill(len, pad);
        for (i, v) in values.iter().enumerate() {
            (self.x[i], self.y[i], self.z[i]) = (v.x, v.y, v.z);
        }
    }
}

/// The kernel's copy of the bodies, padded with dummy bodies up to a whole SIMD register past
/// the last one, so every row can be processed in whole chunks of [`LANES`]. The dummies are
/// massless and [`FAR`] away, so their pull comes out as exactly 0, and they never touch anything.
#[derive(Default)]
struct KernelInput {
    len: usize,
    pos: Columns,
    mass: Vec<f64>,
    radius: Vec<f64>,
}

impl KernelInput {
    fn load(&mut self, pos: &[DVec3], mass: &[f64], radius: &[f64]) {
        let padded = pos.len() + LANES - 1;
        self.len = pos.len();

        self.pos.load(pos, padded, FAR);
        for (column, values) in [(&mut self.mass, mass), (&mut self.radius, radius)] {
            column.clear();
            column.extend_from_slice(values);
            column.resize(padded, 0.0);
        }
    }
}

/// The rest of the relativity kernel's input, padded like [`KernelInput`] with dummies that
/// stand still
#[derive(Default)]
struct RelativityInput {
    vel: Columns,
    /// Accelerations without relativity
    acc: Columns,
    speed_sq: Vec<f64>,
    potential: [Vec<f64>; 1],
}

impl RelativityInput {
    fn load(&mut self, vel: &[DVec3], acc: &[DVec3]) {
        let padded = vel.len() + LANES - 1;
        self.vel.load(vel, padded, 0.0);
        self.acc.load(acc, padded, 0.0);
        self.speed_sq.clear();
        self.speed_sq.extend(vel.iter().map(|v| v.length_squared()));
        self.speed_sq.resize(padded, 0.0);
    }
}

fn load(column: &[f64], j: usize) -> f64x4 {
    f64x4::new(column[j..j + LANES].try_into().unwrap())
}

/// column[j..j + 4] -= value
fn subtract(column: &mut [f64], j: usize, value: f64x4) {
    let lanes: &mut [f64; LANES] = (&mut column[j..j + LANES]).try_into().unwrap();
    *lanes = (f64x4::new(*lanes) - value).to_array();
}

/// column[j..j + 4] += value
fn add(column: &mut [f64], j: usize, value: f64x4) {
    let lanes: &mut [f64; LANES] = (&mut column[j..j + LANES]).try_into().unwrap();
    *lanes = (f64x4::new(*lanes) + value).to_array();
}

/// A 3D vector per SIMD lane
#[derive(Clone, Copy)]
struct Lanes3 {
    x: f64x4,
    y: f64x4,
    z: f64x4,
}

impl Lanes3 {
    const ZERO: Lanes3 = Lanes3 {
        x: f64x4::ZERO,
        y: f64x4::ZERO,
        z: f64x4::ZERO,
    };

    /// Body i's vector in every lane
    fn splat(columns: &Columns, i: usize) -> Self {
        Lanes3 {
            x: f64x4::splat(columns.x[i]),
            y: f64x4::splat(columns.y[i]),
            z: f64x4::splat(columns.z[i]),
        }
    }

    /// The vectors of bodies j..j + 4
    fn load(columns: &Columns, j: usize) -> Self {
        Lanes3 {
            x: load(&columns.x, j),
            y: load(&columns.y, j),
            z: load(&columns.z, j),
        }
    }

    fn dot(self, other: Self) -> f64x4 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// The sum over the lanes
    fn reduce_add(self) -> DVec3 {
        DVec3::new(
            self.x.reduce_add(),
            self.y.reduce_add(),
            self.z.reduce_add(),
        )
    }

    /// columns[c][j..j + 4] += component c
    fn add_to(self, columns: &mut [Vec<f64>; 3], j: usize) {
        let [x, y, z] = columns;
        add(x, j, self.x);
        add(y, j, self.y);
        add(z, j, self.z);
    }
}

impl std::ops::Add for Lanes3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Lanes3 {
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z + o.z,
        }
    }
}

impl std::ops::Sub for Lanes3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Lanes3 {
            x: self.x - o.x,
            y: self.y - o.y,
            z: self.z - o.z,
        }
    }
}

impl std::ops::Mul<f64x4> for Lanes3 {
    type Output = Self;
    fn mul(self, s: f64x4) -> Self {
        Lanes3 {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

impl std::ops::AddAssign for Lanes3 {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

/// Adds m_j / r_ij to `potential[i]` and m_i / r_ij to `potential[j]` for the pairs (i, j > i)
/// in `rows`, 4 pairs at a time: the Newtonian potential without the factor G
fn accumulate_potential(rows: Range<usize>, input: &KernelInput, potential: &mut [f64]) {
    let KernelInput {
        len: n,
        ref pos,
        ref mass,
        ..
    } = *input;

    for i in rows {
        let (r_i, mass_i) = (Lanes3::splat(pos, i), f64x4::splat(mass[i]));
        let mut potential_i = f64x4::ZERO;

        for j in (i + 1..n).step_by(LANES) {
            let r_ij = Lanes3::load(pos, j) - r_i;
            let inverse_distance = f64x4::ONE / r_ij.dot(r_ij).sqrt();
            potential_i += load(mass, j) * inverse_distance;
            add(potential, j, mass_i * inverse_distance);
        }

        potential[i] += potential_i.reduce_add();
    }
}

/// Adds the EIH corrections between body i and every later body j, for i in `rows`, into `out`
/// (without the factor G / c^2), 4 pairs at a time. Each pair is computed once and applied to
/// both bodies. With r_ij = r_j - r_i, phi the Newtonian potential and a the Newtonian
/// acceleration, the correction on body i from body j is
///
/// m_j / r^3 * (scale_ij * r_ij - (r_ij . (4 v_i - 3 v_j)) (v_i - v_j)) + 3.5 m_j / r * a_j
///
/// scale_ij = v_i^2 + 2 v_j^2 - 4 v_i . v_j - 4 phi_i - phi_j - 1.5 (r_ij . v_j / r)^2
///            + 0.5 r_ij . a_j
fn accumulate_corrections(
    rows: Range<usize>,
    input: &KernelInput,
    relativity: &RelativityInput,
    out: &mut [Vec<f64>; 3],
) {
    let KernelInput {
        len: n,
        ref pos,
        ref mass,
        ..
    } = *input;
    let RelativityInput {
        ref vel,
        ref acc,
        ref speed_sq,
        potential: [ref potential],
    } = *relativity;
    let splat = f64x4::splat;
    let (half, one_and_half, three, three_and_half, four) =
        (splat(0.5), splat(1.5), splat(3.0), splat(3.5), splat(4.0));

    for i in rows {
        let (r_i, v_i, a_i) = (
            Lanes3::splat(pos, i),
            Lanes3::splat(vel, i),
            Lanes3::splat(acc, i),
        );
        let (mass_i, speed_sq_i, potential_i) =
            (splat(mass[i]), splat(speed_sq[i]), splat(potential[i]));
        // Body i's sums are kept in registers, one partial sum per lane
        let mut correction_i = Lanes3::ZERO;

        for j in (i + 1..n).step_by(LANES) {
            let r_ij = Lanes3::load(pos, j) - r_i;
            let (v_j, a_j) = (Lanes3::load(vel, j), Lanes3::load(acc, j));
            let (mass_j, speed_sq_j, potential_j) =
                (load(mass, j), load(speed_sq, j), load(potential, j));

            let inverse_distance = f64x4::ONE / r_ij.dot(r_ij).sqrt();
            let inverse_distance_sq = inverse_distance * inverse_distance;
            let inverse_distance_cubed = inverse_distance_sq * inverse_distance;

            let (r_dot_v_i, r_dot_v_j) = (r_ij.dot(v_i), r_ij.dot(v_j));
            let (r_dot_a_i, r_dot_a_j) = (r_ij.dot(a_i), r_ij.dot(a_j));
            let relative_v = v_i - v_j;
            // The part of scale that's the same both ways
            let shared = speed_sq_i + speed_sq_j - four * v_i.dot(v_j);

            // On i from j
            let scale = shared + speed_sq_j
                - four * potential_i
                - potential_j
                - one_and_half * r_dot_v_j * r_dot_v_j * inverse_distance_sq
                + half * r_dot_a_j;
            let along_v = four * r_dot_v_i - three * r_dot_v_j;
            correction_i += (r_ij * scale - relative_v * along_v)
                * (mass_j * inverse_distance_cubed)
                + a_j * (three_and_half * mass_j * inverse_distance);

            // On j from i, where r_ji = -r_ij and v_j - v_i = -relative_v
            let scale = shared + speed_sq_i
                - four * potential_j
                - potential_i
                - one_and_half * r_dot_v_i * r_dot_v_i * inverse_distance_sq
                - half * r_dot_a_i;
            let along_v = four * r_dot_v_j - three * r_dot_v_i;
            let correction_j = a_i * (three_and_half * mass_i * inverse_distance)
                - (r_ij * scale + relative_v * along_v) * (mass_i * inverse_distance_cubed);
            correction_j.add_to(out, j);
        }

        let sum = correction_i.reduce_add();
        out[0][i] += sum.x;
        out[1][i] += sum.y;
        out[2][i] += sum.z;
    }
}

/// Adds the pull between body i and every later body j, for i in `rows`, into `acc`
/// (without the factor G). Each pair is visited once and applied to both bodies
/// (Newton's 3rd law), 4 pairs at a time. Returns the first touching pair found.
fn accumulate_pairs(
    rows: Range<usize>,
    input: &KernelInput,
    acc: &mut Columns,
) -> Option<Collision> {
    let KernelInput {
        len: n,
        ref pos,
        ref mass,
        ref radius,
    } = *input;
    acc.fill(mass.len(), 0.0);
    let mut collision = None;

    for i in rows {
        let (x_i, y_i, z_i) = (
            f64x4::splat(pos.x[i]),
            f64x4::splat(pos.y[i]),
            f64x4::splat(pos.z[i]),
        );
        let (mass_i, radius_i) = (f64x4::splat(mass[i]), f64x4::splat(radius[i]));
        // Body i's sums are kept in registers, one partial sum per lane
        let (mut ax_i, mut ay_i, mut az_i) = (f64x4::ZERO, f64x4::ZERO, f64x4::ZERO);

        for j in (i + 1..n).step_by(LANES) {
            let dx = load(&pos.x, j) - x_i;
            let dy = load(&pos.y, j) - y_i;
            let dz = load(&pos.z, j) - z_i;
            let distance_sq = dx * dx + dy * dy + dz * dz;

            if collision.is_none() {
                let touching_distance = load(radius, j) + radius_i;
                let touching = distance_sq.simd_le(touching_distance * touching_distance);
                if touching.any() {
                    collision = Some((i, j + touching.to_bitmask().trailing_zeros() as usize));
                }
            }

            // difference / distance^3 = unit vector / distance^2
            let inv_distance_cubed = f64x4::ONE / (distance_sq * distance_sq.sqrt());
            let (px, py, pz) = (
                dx * inv_distance_cubed,
                dy * inv_distance_cubed,
                dz * inv_distance_cubed,
            );

            let mass_j = load(mass, j);
            ax_i += px * mass_j;
            ay_i += py * mass_j;
            az_i += pz * mass_j;

            subtract(&mut acc.x, j, px * mass_i);
            subtract(&mut acc.y, j, py * mass_i);
            subtract(&mut acc.z, j, pz * mass_i);
        }

        acc.x[i] += ax_i.reduce_add();
        acc.y[i] += ay_i.reduce_add();
        acc.z[i] += az_i.reduce_add();
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
    use crate::astronomy::{NewBody, OrbitalObject};
    use crate::ephemeris::solar_system;

    fn test_system(extra_planets: usize) -> Bodies {
        let mut rng = StdRng::seed_from_u64(7);
        let mut bodies = solar_system();
        for _ in 0..extra_planets {
            bodies.push_orbiting(OrbitalObject::random_planet(&mut rng), 0, &mut rng);
        }
        bodies
    }

    /// Straight from Newton's law of gravitation, one body at a time
    fn reference_accelerations(bodies: &Bodies) -> Vec<DVec3> {
        (0..bodies.len())
            .map(|i| {
                (0..bodies.len())
                    .filter(|&j| j != i)
                    .map(|j| {
                        let difference = bodies.position[j] - bodies.position[i];
                        G * bodies.mass[j] * difference / difference.length().powi(3)
                    })
                    .sum()
            })
            .collect()
    }

    /// Covers every body count modulo the SIMD width, to exercise the padding
    #[test]
    fn forces_match_reference() {
        for extra in [0, 1, 2, 3, 200] {
            let bodies = test_system(extra);
            let expected = reference_accelerations(&bodies);

            for threads in [1, 3] {
                let mut acc = vec![DVec3::ZERO; bodies.len()];
                let collision = Forces::new(threads, Physics::NEWTONIAN).compute(
                    &bodies,
                    &bodies.position,
                    &bodies.velocity,
                    &mut acc,
                );
                assert_eq!(collision, None);

                for (i, (a, e)) in acc.iter().zip(&expected).enumerate() {
                    assert!(
                        (*a - *e).length() <= 1e-9 * e.length(),
                        "n={}, threads={threads}, body {i}: {a} vs {e}",
                        bodies.len()
                    );
                }
            }
        }
    }

    #[test]
    fn parallel_forces_match_pairwise() {
        let bodies = test_system(200);
        let n = bodies.len();
        let (mut serial, mut parallel) = (vec![DVec3::ZERO; n], vec![DVec3::ZERO; n]);

        Forces::new(1, Physics::NEWTONIAN).compute(
            &bodies,
            &bodies.position,
            &bodies.velocity,
            &mut serial,
        );
        Forces::new(4, Physics::NEWTONIAN).compute(
            &bodies,
            &bodies.position,
            &bodies.velocity,
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
        // Put a body on top of the Earth (3). Earth's row starts at 4, so 4..=8 covers every
        // SIMD lane and the start of the next register.
        for other in 4..=8 {
            let mut bodies = test_system(0);
            bodies.position[other] = bodies.position[3];

            for threads in [1, 4] {
                let mut acc = vec![DVec3::ZERO; bodies.len()];
                let found = Forces::new(threads, Physics::NEWTONIAN).compute(
                    &bodies,
                    &bodies.position,
                    &bodies.velocity,
                    &mut acc,
                );
                assert_eq!(found, Some((3, other)), "threads = {threads}");
            }
        }
    }

    /// For a test particle around a resting mass, the EIH equations reduce to the
    /// Schwarzschild field: a = GM / (c^2 r^3) * ((4 GM / r - v^2) r + 4 (r . v) v)
    #[test]
    fn relativity_matches_schwarzschild_limit() {
        let mut bodies = Bodies::default();
        let sun_gm = 1.32712440041E20;
        for (mass, position, velocity) in [
            (sun_gm / G, DVec3::ZERO, DVec3::ZERO),
            (
                1.0,
                DVec3::new(4.6E10, 1.0E9, 3.0E9),
                DVec3::new(-2.0E3, 5.8E4, 4.0E3),
            ),
        ] {
            bodies.push(NewBody {
                name: String::new(),
                mass,
                radius: 1.0,
                position,
                velocity,
                oblateness: None,
                magnification: 1.0,
                color: [0; 3],
            });
        }

        let relativity = Physics {
            oblateness: false,
            relativity: true,
        };
        let (mut newtonian, mut full) = (vec![DVec3::ZERO; 2], vec![DVec3::ZERO; 2]);
        let (pos, vel) = (&bodies.position, &bodies.velocity);
        Forces::new(1, Physics::NEWTONIAN).compute(&bodies, pos, vel, &mut newtonian);
        Forces::new(1, relativity).compute(&bodies, pos, vel, &mut full);

        let (r, v) = (pos[1], vel[1]);
        let distance = r.length();
        let expected = sun_gm / (C * C * distance.powi(3))
            * ((4.0 * sun_gm / distance - v.length_squared()) * r + 4.0 * r.dot(v) * v);
        let correction = full[1] - newtonian[1];
        assert!(
            (correction - expected).length() <= 1e-6 * expected.length(),
            "{correction} vs {expected}"
        );
    }

    /// The EIH corrections one body at a time, straight from the formula (with `newtonian` as
    /// the accelerations inside it), divided by c^2
    fn reference_corrections(bodies: &Bodies, newtonian: &[DVec3]) -> Vec<DVec3> {
        let (pos, vel) = (&bodies.position, &bodies.velocity);
        let n = bodies.len();
        let gm = |j: usize| G * bodies.mass[j];
        let potential: Vec<f64> = (0..n)
            .map(|i| {
                (0..n)
                    .filter(|&j| j != i)
                    .map(|j| gm(j) / pos[i].distance(pos[j]))
                    .sum()
            })
            .collect();

        (0..n)
            .map(|i| {
                let correction: DVec3 = (0..n)
                    .filter(|&j| j != i)
                    .map(|j| {
                        let (v_i, v_j, a_j) = (vel[i], vel[j], newtonian[j]);
                        let r_ij = pos[j] - pos[i];
                        let r = r_ij.length();
                        let scale = -4.0 * potential[i] - potential[j]
                            + v_i.length_squared()
                            + 2.0 * v_j.length_squared()
                            - 4.0 * v_i.dot(v_j)
                            - 1.5 * (r_ij.dot(v_j) / r).powi(2)
                            + 0.5 * r_ij.dot(a_j);
                        gm(j) / r.powi(3)
                            * (scale * r_ij - r_ij.dot(4.0 * v_i - 3.0 * v_j) * (v_i - v_j))
                            + 3.5 * gm(j) / r * a_j
                    })
                    .sum();
                correction / (C * C)
            })
            .collect()
    }

    /// Covers every body count modulo the SIMD width, and threads
    #[test]
    fn relativity_matches_reference() {
        let relativity = Physics {
            oblateness: false,
            relativity: true,
        };
        for extra in [0, 1, 2, 3, 200] {
            let bodies = test_system(extra);
            let (pos, vel) = (&bodies.position, &bodies.velocity);

            for threads in [1, 3] {
                let n = bodies.len();
                let (mut newtonian, mut full) = (vec![DVec3::ZERO; n], vec![DVec3::ZERO; n]);
                Forces::new(threads, Physics::NEWTONIAN).compute(&bodies, pos, vel, &mut newtonian);
                Forces::new(threads, relativity).compute(&bodies, pos, vel, &mut full);
                let expected = reference_corrections(&bodies, &newtonian);

                for i in 0..n {
                    let correction = full[i] - newtonian[i];
                    // Rounding the corrections into the much larger accelerations leaves
                    // about 1e-8 of them, relative to themselves
                    assert!(
                        (correction - expected[i]).length() <= 1e-6 * expected[i].length(),
                        "n={n}, threads={threads}, body {i}: {correction} vs {}",
                        expected[i]
                    );
                }
            }
        }
    }

    /// The bulges pull back on their planets, so the total force stays zero
    #[test]
    fn oblateness_conserves_momentum() {
        let bodies = solar_system();
        let physics = Physics {
            oblateness: true,
            relativity: false,
        };
        let mut acc = vec![DVec3::ZERO; bodies.len()];
        Forces::new(1, physics).compute(&bodies, &bodies.position, &bodies.velocity, &mut acc);

        let total_force: DVec3 = acc.iter().zip(&bodies.mass).map(|(a, m)| *a * *m).sum();
        let sun_force = acc[0].length() * bodies.mass[0];
        assert!(total_force.length() <= 1e-12 * sun_force, "{total_force}");
    }
}
