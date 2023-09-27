use crate::{astronomy::AstronomicalObject, vector::Vector3d};

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
                        1.0 / 2.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                        1.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                    ),
                    (
                        (1.0 - 2.0f64.powf(1.0 / 3.0)) / 2.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                        -(2.0f64.powf(1.0 / 3.0) / (2.0 - 2.0f64.powf(1.0 / 3.0))),
                    ),
                    (
                        (1.0 - 2.0f64.powf(1.0 / 3.0)) / 2.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                        1.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)),
                    ),
                    (1.0 / 2.0 / (2.0 - 2.0f64.powf(1.0 / 3.0)), 0.0),
                ],
                _ => vec![],
            },
            _ => vec![],
        }
    }
}

#[derive(Default)]
struct IntermediateState {
    velocity: Vector3d,
    dv: Vector3d,
}

pub fn runge_kutta_4(
    local_bodies: &mut Vec<AstronomicalObject>,
    time_step: f64,
) -> Result<(), (usize, usize)> {
    let mut dt = 0.0f64;
    let num_bodies = local_bodies.len();

    let mut s: [Vec<IntermediateState>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    for state in 0..4 {
        if state == 1 || state == 3 {
            dt += 0.5 * time_step;
        }

        let mut positions: Vec<_> = local_bodies.iter().map(|x| x.position.clone()).collect();
        let mut dv = vec![Vector3d::default(); num_bodies];

        if state >= 1 {
            for (position, prev_state) in positions.iter_mut().zip(&s[state - 1]) {
                position.add_mut(&prev_state.velocity.multiply(dt));
            }
        }

        for i in 0..num_bodies - 1 {
            for j in i + 1..num_bodies {
                let difference = positions[j].substract(&positions[i]);
                let distance = difference.length();

                if state == 0 && distance <= local_bodies[i].radius + local_bodies[j].radius {
                    return Err((i, j)); // Process collisions before update results
                }
                let grav_modifier = G / (difference.length().powi(3));

                dv[i].add_mut(&difference.multiply(grav_modifier * local_bodies[j].mass));
                dv[j].add_mut(&difference.multiply(-grav_modifier * local_bodies[i].mass));
            }
        }

        s[state] = (0..num_bodies)
            .map(|i| IntermediateState {
                velocity: if state == 0 {
                    local_bodies[i].velocity.clone()
                } else {
                    local_bodies[i]
                        .velocity
                        .add(&s[state - 1][i].dv.multiply(dt))
                },
                dv: dv[i].clone(),
            })
            .collect();
    }

    for (i, body) in local_bodies.iter_mut().enumerate() {
        let mut dxdt = s[0][i]
            .velocity
            .add(s[1][i].velocity.add(&s[2][i].velocity).multiply_mut(2.0));

        dxdt.add_mut(&s[3][i].velocity).multiply_mut(1.0 / 6.0);

        let mut dvdt = s[0][i]
            .dv
            .add(s[1][i].dv.add(&s[2][i].dv).multiply_mut(2.0));

        dvdt.add_mut(&s[3][i].dv).multiply_mut(1.0 / 6.0);

        body.velocity.add_mut(dvdt.multiply_mut(time_step));
        body.position.add_mut(dxdt.multiply_mut(time_step));
    }

    Ok(())
}

// https://en.wikipedia.org/wiki/Symplectic_integrator
pub fn symplectic(
    local_bodies: &Vec<AstronomicalObject>,
    start: (usize, usize),
    end: (usize, usize),
) -> Result<Vec<Vector3d>, (usize, usize)> {
    let num_bodies = local_bodies.len();
    let mut acceleration_vectors = vec![Vector3d::default(); num_bodies];

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

            let difference = b.position.substract(&a.position);
            let distance = difference.length();

            if distance <= a.radius + b.radius {
                return Err((first, second)); // Process collisions before update results
            }
            let grav_mult = G / (distance.powi(3)); // Divide by r^3 to get a unit vector out of difference

            acceleration_vectors[first].add_mut(&difference.multiply(grav_mult * b.mass));
            acceleration_vectors[second].add_mut(&difference.multiply(-grav_mult * a.mass));
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
    obs[h].velocity = obs[h]
        .velocity
        .multiply(obs[h].mass)
        .add(&obs[l].velocity.multiply(obs[l].mass))
        .multiply(1.0 / total_mass);

    obs[h].position = obs[h].position.add(
        &obs[l]
            .position
            .substract(&obs[h].position)
            .multiply(obs[l].mass / total_mass),
    );

    obs[h].radius *= (total_mass / obs[h].mass).powf(1.0 / 3.0);

    println!("{} collided into {}!", obs[l].name, obs[h].name);
    obs.remove(l);
}