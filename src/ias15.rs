//! IAS15: a 15th order Gauss-Radau integrator with adaptive time steps, accurate to the limits
//! of double precision (Rein & Spiegel 2015, MNRAS 446, 1424), with the improved time step
//! criterion of Pham, Rein & Spiegel 2024 (OJAp 7). Follows those papers and REBOUND's
//! implementation.
//!
//! Over a step of length dt, each body's acceleration is approximated by a polynomial in the
//! fraction of the step h = t / dt:
//!
//!   a(h) = a0 + b0 h + b1 h^2 + ... + b6 h^7
//!
//! which integrates to positions and velocities in closed form. The coefficients b are found
//! by evaluating the forces at 7 points inside the step (Gauss-Radau spacings) and iterating
//! until they no longer change. The size of the highest order terms tells how long the next
//! step can be. Velocity dependent forces (relativity) are fine, as velocities are predicted
//! at every sample point too.

use glam::DVec3;

use crate::astronomy::{Bodies, Merge};
use crate::forces::Forces;

/// Gauss-Radau spacings: the points inside the step (0 = start, 1 = end) forces are sampled at.
/// Digits as published, beyond what f64 holds.
#[allow(clippy::excessive_precision)]
const H: [f64; 8] = [
    0.0,
    0.0562625605369221464656521910318,
    0.180240691736892364987579942780,
    0.352624717113169637373907769648,
    0.547153626330555383001448554766,
    0.734210177215410531523210605558,
    0.885320946839095768090359771030,
    0.977520613561287501891174488626,
];

/// Controls the step size, smaller is more accurate. REBOUND's default, which keeps the
/// integration error below double precision rounding for typical orbits.
const EPSILON: f64 = 1e-9;
/// A step is redone if the next one should be shorter than this fraction of it, and steps grow
/// by at most the inverse of it.
const SAFETY_FACTOR: f64 = 0.25;
const MAX_ITERATIONS: usize = 12;
/// The very first step, before there's anything to base the step size on
const INITIAL_STEP: f64 = 60.0;

/// b0..b6 (or g0..g6) of one body
type Coefficients = [DVec3; 7];

pub struct Ias15 {
    /// Length of the next step to try, 0 before the first one
    dt: f64,
    /// Length of the last accepted step, 0 if there's none to predict from
    dt_last: f64,
    /// The acceleration polynomial's coefficients for each body
    b: Vec<Coefficients>,
    /// The same polynomial in Newton's divided difference form, which is what the samples
    /// update directly
    g: Vec<Coefficients>,
    /// b as predicted from the previous step, before iterating
    e: Vec<Coefficients>,
    /// Whether `e` is a real prediction, rather than zeros for lack of history
    predicted: bool,
    /// b and e of the last accepted step, to predict from again if a step is rejected
    b_last: Vec<Coefficients>,
    e_last: Vec<Coefficients>,
    /// Compensated (Kahan) summation: the rounding error of each body's position and velocity
    /// not yet added in, so it doesn't accumulate over millions of steps
    position_error: Vec<DVec3>,
    velocity_error: Vec<DVec3>,
    /// Scratch: positions and velocities predicted at a sample point, and the accelerations there
    pos: Vec<DVec3>,
    vel: Vec<DVec3>,
    acc: Vec<DVec3>,
    /// `conversion[j][k]`: how much b_k changes per change of g_j
    conversion: [[f64; 7]; 7],
}

impl Default for Ias15 {
    fn default() -> Self {
        Ias15 {
            dt: 0.0,
            dt_last: 0.0,
            b: Vec::new(),
            g: Vec::new(),
            e: Vec::new(),
            predicted: false,
            b_last: Vec::new(),
            e_last: Vec::new(),
            position_error: Vec::new(),
            velocity_error: Vec::new(),
            pos: Vec::new(),
            vel: Vec::new(),
            acc: Vec::new(),
            conversion: conversion(),
        }
    }
}

/// The g_j term of the polynomial is g_j * h (h - h_1) ... (h - h_j). `conversion()[j][k]` is the
/// coefficient of h^(k+1) in that product, i.e. the contribution of g_j to b_k.
fn conversion() -> [[f64; 7]; 7] {
    let mut table = [[0.0; 7]; 7];
    // Coefficients of (h - h_1) ... (h - h_j), lowest power first
    let mut product = vec![1.0];

    for (j, row) in table.iter_mut().enumerate() {
        if j > 0 {
            let mut next = vec![0.0; product.len() + 1];
            for (k, c) in product.iter().enumerate() {
                next[k + 1] += c;
                next[k] -= c * H[j];
            }
            product = next;
        }
        row[..product.len()].copy_from_slice(&product);
    }

    table
}

impl Ias15 {
    /// Advances by one step of at most `max_dt` and returns its length
    pub fn step(
        &mut self,
        forces: &mut Forces,
        bodies: &mut Bodies,
        max_dt: f64,
        merges: &mut Vec<Merge>,
    ) -> f64 {
        // Acceleration at the start of the step. Collisions are resolved here.
        let merged_before = merges.len();
        forces.update_with_merges(bodies, merges);
        if merges.len() != merged_before || self.b.len() != bodies.len() {
            // Bodies were added or removed, the history no longer applies
            self.reset(bodies.len());
        }
        if self.dt == 0.0 {
            self.dt = INITIAL_STEP;
        }

        loop {
            let dt = self.dt.min(max_dt);
            if dt != self.dt {
                self.rescale_prediction(dt / self.dt);
                self.dt = dt;
            }

            if let Some(next_dt) = self.try_step(forces, bodies, dt) {
                self.dt = next_dt;
                return dt;
            }
        }
    }

    fn reset(&mut self, n: usize) {
        for coefficients in [
            &mut self.b,
            &mut self.g,
            &mut self.e,
            &mut self.b_last,
            &mut self.e_last,
        ] {
            coefficients.clear();
            coefficients.resize(n, [DVec3::ZERO; 7]);
        }
        for vector in [
            &mut self.position_error,
            &mut self.velocity_error,
            &mut self.pos,
            &mut self.vel,
            &mut self.acc,
        ] {
            vector.clear();
            vector.resize(n, DVec3::ZERO);
        }
        self.dt_last = 0.0;
        self.predicted = false;
    }

    /// Takes a step of `dt`. Returns the length of the next step, or None if this one was too
    /// long and has to be redone shorter (`self.dt` has been updated for that).
    fn try_step(&mut self, forces: &mut Forces, bodies: &mut Bodies, dt: f64) -> Option<f64> {
        let n = bodies.len();

        for (g, b) in self.g.iter_mut().zip(&self.b) {
            *g = divided_differences(b, &self.conversion);
        }

        // Predictor-corrector: sample the forces at every spacing, refining b, until b settles
        let mut last_error = f64::INFINITY;
        for iteration in 0..MAX_ITERATIONS {
            let (mut max_change, mut max_acc) = (0.0f64, 0.0f64);

            for (sample, &h) in H.iter().enumerate().skip(1) {
                for i in 0..n {
                    (self.pos[i], self.vel[i]) = self.predict(bodies, i, dt, h);
                }
                forces.compute(bodies, &self.pos, &self.vel, &mut self.acc);

                for i in 0..n {
                    let g = &mut self.g[i];
                    // Newton's divided differences of the sampled accelerations
                    let mut value = (self.acc[i] - bodies.acceleration[i]) / h;
                    for m in 1..sample {
                        value = (value - g[m - 1]) / (h - H[m]);
                    }
                    let change = value - g[sample - 1];
                    g[sample - 1] = value;

                    for k in 0..sample {
                        self.b[i][k] += change * self.conversion[sample - 1][k];
                    }

                    if sample == 7 {
                        max_change = max_change.max(change.abs().max_element());
                        max_acc = max_acc.max(self.acc[i].abs().max_element());
                    }
                }
            }

            let error = max_change / max_acc;
            // Converged, or no longer improving because of rounding
            if error < 1e-16 || (iteration >= 2 && error >= last_error) {
                break;
            }
            last_error = error;
        }

        let next_dt = self.next_step_size(bodies, dt);

        if next_dt < SAFETY_FACTOR * dt {
            self.dt = next_dt;
            self.predict_from_last(next_dt);
            return None;
        }

        // Accepted: move every body to the end of the step
        for i in 0..n {
            let (b, a0, v0) = (&self.b[i], bodies.acceleration[i], bodies.velocity[i]);
            let dx = dt * v0 + dt * dt * (a0 / 2.0 + position_terms(b, 1.0));
            let dv = dt * (a0 + velocity_terms(b, 1.0));
            add_compensated(&mut bodies.position[i], &mut self.position_error[i], dx);
            add_compensated(&mut bodies.velocity[i], &mut self.velocity_error[i], dv);
        }

        let next_dt = next_dt.min(dt / SAFETY_FACTOR);
        self.dt_last = dt;
        self.b_last.clone_from(&self.b);
        // Without a prediction there's no correction to carry over
        self.e_last
            .clone_from(if self.predicted { &self.e } else { &self.b });
        self.predict_from_last(next_dt);

        Some(next_dt)
    }

    /// Position and velocity of body `i` at the fraction `h` of the step
    fn predict(&self, bodies: &Bodies, i: usize, dt: f64, h: f64) -> (DVec3, DVec3) {
        let (b, a0) = (&self.b[i], bodies.acceleration[i]);
        let t = dt * h;

        let dx = t * bodies.velocity[i] + t * t * (a0 / 2.0 + position_terms(b, h));
        let dv = t * (a0 + velocity_terms(b, h));
        (
            bodies.position[i] + (dx - self.position_error[i]),
            bodies.velocity[i] + (dv - self.velocity_error[i]),
        )
    }

    /// Step size criterion of Pham, Rein & Spiegel 2024: based on the shortest time scale
    /// on which any body's acceleration changes, from its derivatives at the end of the step.
    fn next_step_size(&self, bodies: &Bodies, dt: f64) -> f64 {
        let mut min_timescale_sq = f64::INFINITY;

        for (b, a0) in self.b.iter().zip(&bodies.acceleration) {
            let acc: DVec3 = *a0 + b.iter().sum::<DVec3>();
            let jerk: DVec3 = (0..7).map(|k| (k + 1) as f64 * b[k]).sum();
            let snap: DVec3 = (1..7).map(|k| ((k + 1) * k) as f64 * b[k]).sum();

            let (y2, y3, y4) = (
                acc.length_squared(),
                jerk.length_squared(),
                snap.length_squared(),
            );
            if !y2.is_normal() {
                continue;
            }
            let timescale_sq = 2.0 * y2 / (y3 + (y4 * y2).sqrt());
            if timescale_sq.is_normal() {
                min_timescale_sq = min_timescale_sq.min(timescale_sq);
            }
        }

        if min_timescale_sq.is_normal() {
            min_timescale_sq.sqrt() * dt * (EPSILON * 5040.0).powf(1.0 / 7.0)
        } else {
            // Nothing is accelerating
            dt / SAFETY_FACTOR
        }
    }

    /// Predicts b for a next step of `next_dt`, from the polynomial of the last accepted step.
    /// The correction the iterations made to the last prediction is carried over too.
    fn predict_from_last(&mut self, next_dt: f64) {
        let ratio = next_dt / self.dt_last;

        // Nothing to predict from, or so far ahead the prediction would be worthless
        if self.dt_last == 0.0 || ratio > 20.0 {
            for coefficients in [&mut self.b, &mut self.e] {
                coefficients.fill([DVec3::ZERO; 7]);
            }
            self.predicted = false;
            return;
        }

        let q: [f64; 7] = std::array::from_fn(|k| ratio.powi(k as i32 + 1));
        for i in 0..self.b.len() {
            let (b, e) = (&self.b_last[i], &self.e_last[i]);
            // The last polynomial continued past the end of its step, a(1 + ratio * h),
            // expanded in powers of h: binomial coefficients
            let prediction: Coefficients = std::array::from_fn(|k| {
                let shifted: DVec3 = (k..7).map(|m| binomial(m + 1, k + 1) * b[m]).sum();
                q[k] * shifted
            });

            for k in 0..7 {
                self.b[i][k] = prediction[k] + (b[k] - e[k]);
            }
            self.e[i] = prediction;
        }
        self.predicted = true;
    }

    /// The polynomial is in terms of the fraction of the step, so a step of a different
    /// length scales its terms: b_k by ratio^(k+1)
    fn rescale_prediction(&mut self, ratio: f64) {
        let q: [f64; 7] = std::array::from_fn(|k| ratio.powi(k as i32 + 1));
        for coefficients in [&mut self.b, &mut self.e] {
            for body in coefficients.iter_mut() {
                for k in 0..7 {
                    body[k] *= q[k];
                }
            }
        }
    }
}

/// g from b: the inverse of the conversion, which is triangular
fn divided_differences(b: &Coefficients, conversion: &[[f64; 7]; 7]) -> Coefficients {
    let mut g = [DVec3::ZERO; 7];
    for k in (0..7).rev() {
        g[k] = b[k] - (k + 1..7).map(|j| conversion[j][k] * g[j]).sum::<DVec3>();
    }
    g
}

/// sum of b_k h^(k+1) / ((k+2)(k+3)): the polynomial's part of the position, per dt^2 h^2
fn position_terms(b: &Coefficients, h: f64) -> DVec3 {
    (0..7).rev().fold(DVec3::ZERO, |sum, k| {
        (sum + b[k] / ((k + 2) * (k + 3)) as f64) * h
    })
}

/// sum of b_k h^(k+1) / (k+2): the polynomial's part of the velocity, per dt h
fn velocity_terms(b: &Coefficients, h: f64) -> DVec3 {
    (0..7)
        .rev()
        .fold(DVec3::ZERO, |sum, k| (sum + b[k] / (k + 2) as f64) * h)
}

fn binomial(n: usize, k: usize) -> f64 {
    (0..k).fold(1.0, |product, i| product * (n - i) as f64 / (i + 1) as f64)
}

/// sum += value, keeping the rounding error in `error` (Kahan)
fn add_compensated(sum: &mut DVec3, error: &mut DVec3, value: DVec3) {
    let corrected = value - *error;
    let new_sum = *sum + corrected;
    *error = (new_sum - *sum) - corrected;
    *sum = new_sum;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spacings are the Gauss-Radau nodes: 0 and the roots of P7(x) + P8(x), with
    /// x = 2h - 1 mapping them onto [-1, 1]
    #[test]
    fn spacings_are_radau_nodes() {
        let legendre = |n: usize, x: f64| {
            let (mut previous, mut current) = (1.0, x);
            for k in 1..n {
                let k = k as f64;
                (previous, current) = (
                    current,
                    ((2.0 * k + 1.0) * x * current - k * previous) / (k + 1.0),
                );
            }
            current
        };
        for &h in &H[1..] {
            let x = 2.0 * h - 1.0;
            assert!((legendre(7, x) + legendre(8, x)).abs() < 1e-14, "{h}");
        }
    }

    #[test]
    fn divided_differences_invert_conversion() {
        let conversion = conversion();
        let g: Coefficients =
            std::array::from_fn(|k| DVec3::new(k as f64 + 1.0, -2.0, 0.5 * k as f64));
        let b: Coefficients =
            std::array::from_fn(|k| (k..7).map(|j| conversion[j][k] * g[j]).sum());
        let back = divided_differences(&b, &conversion);
        for k in 0..7 {
            assert!(
                (back[k] - g[k]).length() < 1e-12,
                "{k}: {} vs {}",
                back[k],
                g[k]
            );
        }
        // Known value from REBOUND's table: the first term of the second product is -h_1
        assert_eq!(conversion[1][0], -H[1]);
    }

    #[test]
    fn polynomial_terms_match_their_integrals() {
        // With only b_k set, position_terms is b_k h^(k+1) / ((k+2)(k+3))
        for k in 0..7 {
            let mut b = [DVec3::ZERO; 7];
            b[k] = DVec3::ONE;
            let h: f64 = 0.7;
            let expected = h.powi(k as i32 + 1) / ((k + 2) * (k + 3)) as f64;
            assert!((position_terms(&b, h).x - expected).abs() < 1e-15);
            let expected = h.powi(k as i32 + 1) / (k + 2) as f64;
            assert!((velocity_terms(&b, h).x - expected).abs() < 1e-15);
        }
    }
}
