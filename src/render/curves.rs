//! Smooth lines from a few points. Traces and orbits are stored coarsely (a point per degree of
//! turning, or a hundred-odd points around an orbit), which is plenty seen from afar but shows as
//! straight chords up close: a degree of Jupiter's orbit is 13 million km, and its middle is 0.4
//! Jupiter radii off the true path. So each piece is cut finer only as far as its distance from the
//! camera needs, keeping it within a fraction of a pixel of the true curve.

use glam::DVec3;

/// Most pieces a segment is cut into
const MAX_PIECES: usize = 1024;

/// A point on a path with the velocity there, at a time (m, m/s, s)
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub position: DVec3,
    pub velocity: DVec3,
    pub time: f64,
}

/// The cubic from `a` to `b` that leaves and arrives with their velocities, at `s` (0 to 1) of the
/// way. Around a circle it stays within 1e-9 of the radius of the true path with points a degree
/// apart, and 0.3% with points 60 degrees apart.
pub fn hermite(a: &Point, b: &Point, s: f64) -> DVec3 {
    let dt = b.time - a.time;
    let (s2, s3) = (s * s, s * s * s);
    a.position * (2.0 * s3 - 3.0 * s2 + 1.0)
        + a.velocity * dt * (s3 - 2.0 * s2 + s)
        + b.position * (-2.0 * s3 + 3.0 * s2)
        + b.velocity * dt * (s3 - s2)
}

/// How far the cubic from `a` to `b` strays from the straight line between them, at most
pub fn hermite_deviation(a: &Point, b: &Point) -> f64 {
    let dt = b.time - a.time;
    if dt <= 0.0 {
        return 0.0;
    }
    let chord = (b.position - a.position) / dt;
    // Both velocity terms peak at 4/27 of dt
    dt * ((a.velocity - chord).length() + (b.velocity - chord).length()) * 4.0 / 27.0
}

/// How many straight pieces a curve between `a` and `b` (camera-relative) that strays up to
/// `deviation` from the line between them needs, for each to stay within `tolerance` (radians, as
/// seen from the camera) of it. Halving a piece quarters how far it strays.
pub fn pieces(a: DVec3, b: DVec3, deviation: f64, tolerance: f64) -> usize {
    if deviation <= 0.0 {
        return 1;
    }
    // Distance from the camera to the nearest point of the segment
    let along = b - a;
    let t = if along.length_squared() > 0.0 {
        (-a.dot(along) / along.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let distance = (a + along * t).length().max(1.0);
    ((deviation / (tolerance * distance)).sqrt().ceil() as usize).clamp(1, MAX_PIECES)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point on a unit circle, going around once per TAU seconds
    fn on_circle(angle: f64) -> Point {
        Point {
            position: DVec3::new(angle.cos(), angle.sin(), 0.0),
            velocity: DVec3::new(-angle.sin(), angle.cos(), 0.0),
            time: angle,
        }
    }

    #[test]
    fn follows_a_circle() {
        for (step_degrees, error) in [(1.0, 1e-9), (10.0, 1e-5), (30.0, 2e-4), (60.0, 4e-3)] {
            let (a, b) = (on_circle(0.0), on_circle(f64::to_radians(step_degrees)));
            let worst = (0..=100)
                .map(|k| (hermite(&a, &b, k as f64 / 100.0).length() - 1.0).abs())
                .fold(0.0, f64::max);
            assert!(worst < error, "{step_degrees}: {worst}");
            // The bound covers the true sagitta
            let sagitta = 1.0 - f64::to_radians(step_degrees / 2.0).cos();
            assert!(hermite_deviation(&a, &b) >= sagitta * 0.99);
        }
    }

    #[test]
    fn finer_only_near_the_camera() {
        let (a, b) = (DVec3::new(-1.0, 0.0, -10.0), DVec3::new(1.0, 0.0, -10.0));
        assert_eq!(pieces(a, b, 0.0, 1e-3), 1);
        let near = pieces(a, b, 0.1, 1e-3);
        let far = pieces(a * 100.0, b * 100.0, 0.1, 1e-3);
        assert!(near > far && far == 1, "{near} {far}");
        // Each piece then strays deviation / pieces² from its own line: within tolerance
        assert!(0.1 / (near * near) as f64 / 10.0 <= 1e-3);
    }
}
