use std::ops::DivAssign;

use glam::DVec3;

use crate::astronomy::AstronomicalObject;

pub const G: f64 = 6.6743E-11;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IntegrationMethod {
    Symplectic(u8),
    RK4,
}

// c and d coefficients for symplectic integrator
impl IntegrationMethod {
    pub fn get_coefficients(&self) -> Vec<(f64, f64)> {
        match &self {
            IntegrationMethod::Symplectic(k) => match k {
                1 => vec![(1.0, 1.0)],
                2 => vec![(0.0, 0.5), (1.0, 0.5)],
                3 => vec![
                    (1.0, -1.0 / 24.0),
                    (-2.0 / 3.0, 3.0 / 4.0),
                    (2.0 / 3.0, 7.0 / 24.0),
                ],
                4 => vec![
                    (
                        1.0 / (4.0 - 2.0f64.powf(4.0 / 3.0)),
                        1.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                    ),
                    (
                        (1.0 - 2.0f64.powf(1.0 / 3.0)) / (4.0 - 2.0f64.powf(4.0 / 3.0)),
                        -(2.0f64.powf(1.0 / 3.0) / (2.0 - 2.0f64.powf(1.0 / 3.0))),
                    ),
                    (
                        (1.0 - 2.0f64.powf(1.0 / 3.0)) / (4.0 - 2.0f64.powf(4.0 / 3.0)),
                        1.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                    ),
                    (1.0 / (4.0 - 2.0f64.powf(4.0 / 3.0)), 0.0),
                ],
                _ => vec![],
            },
            _ => vec![],
        }
    }
}

#[derive(Default)]
struct IntermediateState {
    velocity: DVec3,
    dv: DVec3,
}

pub fn runge_kutta_4(
    local_bodies: &mut [AstronomicalObject],
    time_step: f64,
) -> Option<(usize, usize)> {
    let mut dt = 0.5f64 * time_step;
    let num_bodies = local_bodies.len();

    let mut s: [Vec<IntermediateState>; 4] = [
        Vec::with_capacity(num_bodies),
        Vec::with_capacity(num_bodies),
        Vec::with_capacity(num_bodies),
        Vec::with_capacity(num_bodies),
    ];

    for state in 0..4 {
        if state == 3 {
            dt += 0.5 * time_step;
        }

        let mut positions: Vec<_> = local_bodies.iter().map(|x| x.position).collect();
        let mut dv = vec![DVec3::ZERO; num_bodies];

        if state >= 1 {
            for (position, prev_state) in positions.iter_mut().zip(&s[state - 1]) {
                *position += prev_state.velocity * dt;
            }
        }

        for i in 0..num_bodies - 1 {
            for j in i + 1..num_bodies {
                let difference = positions[j] - positions[i];
                let distance = difference.length();

                if state == 0 && distance <= local_bodies[i].radius + local_bodies[j].radius {
                    return Some((i, j)); // Process collisions before update results
                }
                let grav_modifier = G / (difference.length().powi(3));

                dv[i] += grav_modifier * local_bodies[j].mass * difference;
                dv[j] += -grav_modifier * local_bodies[i].mass * difference;
            }
        }

        s[state] = dv
            .into_iter()
            .enumerate()
            .map(|(i, accel)| IntermediateState {
                velocity: if state == 0 {
                    local_bodies[i].velocity
                } else {
                    local_bodies[i].velocity + s[state - 1][i].dv * dt
                },
                dv: accel,
            })
            .collect();
    }

    for (i, body) in local_bodies.iter_mut().enumerate() {
        let mut dxdt =
            s[0][i].velocity + (s[1][i].velocity + s[2][i].velocity) * 2.0 + s[3][i].velocity;

        dxdt.div_assign(6.0);

        let mut dvdt = s[0][i].dv + (s[1][i].dv + s[2][i].dv) * 2.0 + s[3][i].dv;
        dvdt.div_assign(6.0);

        body.position += dxdt * time_step;
        body.velocity += dvdt * time_step;

        body.acceleration = dvdt;
    }

    None
}

// https://en.wikipedia.org/wiki/Symplectic_integrator
pub fn symplectic_mt(
    local_bodies: &[AstronomicalObject],
    start: (usize, usize),
    end: (usize, usize),
) -> Result<Vec<DVec3>, (usize, usize)> {
    let num_bodies = local_bodies.len();
    let mut acceleration_vectors = vec![DVec3::ZERO; num_bodies];

    let start_i = start.0;
    let start_j = start.1;

    let end_i = end.0;
    let end_j = end.1;

    'outer_loop: for first in start_i..=end_i {
        for second in first + 1..num_bodies {
            if first == start_i && second < start_j {
                continue;
            }
            if first == end_i && second > end_j {
                break 'outer_loop;
            }

            let (a, b) = (&local_bodies[first], &local_bodies[second]);

            let difference = b.position - a.position;
            let distance = difference.length();

            if distance <= a.radius + b.radius {
                return Err((first, second)); // Process collisions before update results
            }
            let grav_mult = G / (distance.powi(3)); // Divide by r^3 to get a unit vector out of difference

            acceleration_vectors[first] += grav_mult * b.mass * difference;
            acceleration_vectors[second] += -grav_mult * a.mass * difference;
        }
    }

    Ok(acceleration_vectors)
}

pub fn symplectic(local_bodies: &[AstronomicalObject]) -> Result<Vec<DVec3>, (usize, usize)> {
    let num_bodies = local_bodies.len();
    let mut acceleration_vectors = vec![DVec3::ZERO; num_bodies];

    for first in 0..local_bodies.len() - 1 {
        for second in first + 1..local_bodies.len() {
            let (a, b) = (&local_bodies[first], &local_bodies[second]);

            let difference = b.position - a.position;
            let distance = difference.length();

            if distance <= a.radius + b.radius {
                return Err((first, second)); // Process collisions before update results
            }
            let grav_mult = G / (distance.powi(3)); // Divide by r^3 to get a unit vector out of difference

            acceleration_vectors[first] += grav_mult * b.mass * difference;
            acceleration_vectors[second] += -grav_mult * a.mass * difference;
        }
    }

    Ok(acceleration_vectors)
}

pub fn collide_objects(
    local_objects: &mut Vec<AstronomicalObject>,
    (first, second): &(usize, usize),
) {
    let obs = local_objects;
    let (h, l) = if obs[*first].mass >= obs[*second].mass {
        (*first, *second)
    } else {
        (*second, *first)
    };

    let total_mass = obs[h].mass + obs[l].mass;

    obs[h].velocity = (obs[h].velocity * obs[h].mass + obs[l].velocity * obs[l].mass) / total_mass;
    obs[h].position =
        obs[h].position + (obs[l].position - obs[h].position) * (obs[l].mass / total_mass);

    obs[h].mass += obs[l].mass;
    obs[h].radius *= (total_mass / obs[h].mass).powf(3.0_f64.recip());

    println!("{} collided into {}!", obs[l].name, obs[h].name);
    obs.remove(l);
}
