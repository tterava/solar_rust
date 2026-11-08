//! The orbit a body is on right now: the two-body (osculating) ellipse its current position and
//! velocity relative to its parent describe. It's worked out again every frame from the
//! simulation's state, so it never drifts from the simulation: when something pulls a body off
//! course, its orbit changes with it.

use std::f64::consts::TAU;

use glam::DVec3;

/// Points around the orbit of a body at `position` moving at `velocity` relative to its parent
/// (m, m/s), where `gm` is G times their combined mass. The points are relative to the parent,
/// start at the body, and the last one connects back to the first. `None` if the body isn't bound
/// to its parent, or falls straight at it.
///
/// `pieces` says how many straight pieces the stretch between two points (relative to the parent)
/// needs, given how far the true orbit strays from the line between them: more where the camera is
/// close (see [`super::curves::pieces`]).
pub fn ellipse(
    position: DVec3,
    velocity: DVec3,
    gm: f64,
    pieces: impl Fn(DVec3, DVec3, f64) -> usize,
) -> Option<Vec<DVec3>> {
    let distance = position.length();
    let momentum = position.cross(velocity);
    let energy = 0.5 * velocity.length_squared() - gm / distance;
    if energy >= 0.0 || momentum.length_squared() == 0.0 {
        return None;
    }

    let semi_major = -gm / (2.0 * energy);
    // Points from the parent towards the closest approach, as long as the eccentricity
    let eccentricity_vector = velocity.cross(momentum) / gm - position / distance;
    let eccentricity = eccentricity_vector.length().min(1.0 - 1e-12);
    let towards_periapsis = if eccentricity > 1e-9 {
        eccentricity_vector / eccentricity
    } else {
        position / distance
    };
    let sideways = momentum.normalize().cross(towards_periapsis);
    let semi_minor = semi_major * (1.0 - eccentricity * eccentricity).sqrt();
    let at = |anomaly: f64| {
        let (sin, cos) = anomaly.sin_cos();
        towards_periapsis * semi_major * (cos - eccentricity) + sideways * semi_minor * sin
    };
    // The body's own eccentric anomaly, so that the first point is exactly where it is
    let start = (position.dot(sideways) / semi_minor)
        .atan2(position.dot(towards_periapsis) / semi_major + eccentricity);

    // Evenly spaced in eccentric anomaly, with more points for eccentric orbits, whose closest
    // approach is a tight turn
    let segments = (128.0 / (1.0 - eccentricity).sqrt()).clamp(128.0, 1024.0) as usize;
    let step = TAU / segments as f64;
    let mut points = Vec::with_capacity(segments);
    for k in 0..segments {
        let anomaly = start + step * k as f64;
        let (a, b) = (at(anomaly), at(anomaly + step));
        let deviation = at(anomaly + step / 2.0).distance((a + b) / 2.0);
        let n = pieces(a, b, deviation).max(1);
        points.push(a);
        points.extend((1..n).map(|j| at(anomaly + step * j as f64 / n as f64)));
    }
    Some(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_rust::astronomy::AU;
    use solar_rust::ephemeris::solar_system;
    use solar_rust::forces::G;

    #[test]
    fn earths_orbit() {
        let bodies = solar_system();
        let index = |name| bodies.info.iter().position(|b| &*b.name == name).unwrap();
        let (sun, earth) = (index("Sun"), index("Earth"));
        let position = bodies.position[earth] - bodies.position[sun];
        let velocity = bodies.velocity[earth] - bodies.velocity[sun];
        let gm = G * (bodies.mass[sun] + bodies.mass[earth]);
        let points = ellipse(position, velocity, gm, |_, _, _| 1).unwrap();
        // It starts at the Earth
        assert!(
            points[0].distance(position) < 1.0,
            "{}",
            points[0].distance(position)
        );

        // From 0.983 to 1.017 AU (the Moon makes the Earth's own orbit wobble a little)
        let distances = points.iter().map(|p| p.length() / AU);
        let (closest, farthest) =
            distances.fold((f64::MAX, 0.0f64), |(lo, hi), d| (lo.min(d), hi.max(d)));
        assert!((closest - 0.983).abs() < 0.002, "{closest}");
        assert!((farthest - 1.017).abs() < 0.002, "{farthest}");

        // The Earth is on it (within the sagitta of its segments), and it lies in the ecliptic
        let to_segment = |a: DVec3, b: DVec3| {
            let t = ((position - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            position.distance(a + (b - a) * t)
        };
        let nearest = (0..points.len())
            .map(|k| to_segment(points[k], points[(k + 1) % points.len()]))
            .fold(f64::MAX, f64::min);
        assert!(nearest < 0.002 * AU, "{}", nearest / AU);
        assert!(points.iter().all(|p| p.z.abs() < 1e-3 * AU));
    }

    #[test]
    fn unbound_or_falling_has_no_orbit() {
        let gm = 1.327E20;
        let r = DVec3::X * AU;
        let escape = (2.0 * gm / AU).sqrt();
        let one = |_, _, _| 1;
        assert!(ellipse(r, DVec3::Y * escape * 1.01, gm, one).is_none());
        assert!(ellipse(r, -DVec3::X * 1000.0, gm, one).is_none());
        // A circle stays at its radius
        let circle = ellipse(r, DVec3::Y * (gm / AU).sqrt(), gm, one).unwrap();
        assert!(circle.iter().all(|p| (p.length() / AU - 1.0).abs() < 1e-9));
    }

    /// Seen from close to the body, the stretch near it is cut finer: it stays within the
    /// tolerance of the true circle, here 1e-4 radians from 1e-3 AU away
    #[test]
    fn finer_near_the_camera() {
        let gm = 1.327E20;
        let r = DVec3::new(0.8, 0.6, 0.0) * AU;
        let v = DVec3::new(-0.6, 0.8, 0.0) * (gm / AU).sqrt();
        let camera = r + DVec3::Z * 1e-3 * AU;
        let tolerance = 1e-4;
        let points = ellipse(r, v, gm, |a, b, deviation| {
            super::super::curves::pieces(a - camera, b - camera, deviation, tolerance)
        })
        .unwrap();
        let coarse = ellipse(r, v, gm, |_, _, _| 1).unwrap();
        assert!(points.len() > coarse.len());
        // The first stretch, from the body on, is nearly straight as seen from the camera
        let (a, b) = (points[0], points[1]);
        let mid = (a + b) / 2.0;
        assert!(coarse[0].distance(points[0]) < 1.0);
        let sagitta = AU - mid.length();
        assert!(
            sagitta.abs() / (mid - camera).length() < tolerance,
            "{sagitta}"
        );
    }
}
