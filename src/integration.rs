use crate::{astronomy::AstronomicalObject, vector::Vector4d};

pub const G: f64 = 6.6743E-11;

#[derive(Default)]
struct IntermediateState {
    velocity: Vector4d,
    dv: Vector4d,
}

pub fn runge_kutta_4(local_bodies: &mut Vec<AstronomicalObject>, time_step: f64) -> bool {
    let mut dt = 0.0f64;
    let num_bodies = local_bodies.len();

    if num_bodies == 0 {
        return false;
    }

    let mut s: [Vec<IntermediateState>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    for state in 0..4 {
        if state == 1 || state == 3 {
            dt += 0.5 * time_step;
        }

        let mut positions: Vec<_> = local_bodies.iter().map(|x| x.position.clone()).collect();
        let mut dv = vec![Vector4d::default(); num_bodies];

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
                    println!("Found collision, ending integration early");
                    return true;  // Process collisions before update results
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
            .add(&s[1][i].velocity.add(&s[2][i].velocity).multiply(2.0))
            .add(&s[3][i].velocity)
            .multiply(1.0 / 6.0);

        let mut dvdt = s[0][i]
            .dv
            .add(&s[1][i].dv.add(&s[2][i].dv).multiply(2.0))
            .add(&s[3][i].dv)
            .multiply(1.0 / 6.0);

        body.velocity.add_mut(dvdt.multiply_mut(time_step));
        body.position.add_mut(dxdt.multiply_mut(time_step));
    }

    false
}


pub fn semi_implicit_euler(local_bodies: &Vec<AstronomicalObject>, start: (usize, usize), end: (usize, usize)) -> (Vec<Vector4d>, bool) {
    let num_bodies = local_bodies.len();

    if num_bodies == 0 {
        return (Vec::new(), false);
    }

    let mut acceleration_vectors = vec![Vector4d::default(); num_bodies];
    
    let start_i = start.0;
    let start_j = start.1;

    let end_i = end.0;
    let end_j = end.1;

    'outer_loop: for first in start_i ..= end_i {
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
                println!("Found collision, ending integration early");
                return (acceleration_vectors, true);  // Process collisions before update results
            }
            let grav_mult = G / (distance.powi(3)); // Divide by r^3 to get a unit vector out of difference

            acceleration_vectors[first].add_mut(&difference.multiply(grav_mult * b.mass));
            acceleration_vectors[second].add_mut(&difference.multiply(-grav_mult * a.mass));
        }
    }

    (acceleration_vectors, false)
}


pub fn process_collisions(local_objects: &mut Vec<AstronomicalObject>) {
    let obs = local_objects;
    'collision_loop: loop {
        let len = obs.len();
        if len == 0 {
            return;
        }

        for i in 0..len - 1 {
            'inner_loop: for j in i + 1..len {
                let combined_radius = obs[i].radius + obs[j].radius;
                let (pos1, pos2) = (obs[i].position.data, obs[j].position.data);

                // Quick bounding box check
                for k in 0..3 {
                    if (pos1[k] - pos2[k]).abs() > combined_radius {
                        continue 'inner_loop;
                    }
                }

                // Slower exact check
                if obs[i].position.distance(&obs[j].position) > combined_radius {
                    continue;
                }

                let (heavy, light) = if obs[i].mass >= obs[j].mass {
                    (i, j)
                } else {
                    (j, i)
                };
                let total_mass = obs[i].mass + obs[j].mass;

                obs[heavy].velocity = obs[heavy]
                    .velocity
                    .multiply(obs[heavy].mass)
                    .add(&obs[light].velocity.multiply(obs[light].mass))
                    .multiply(1.0 / total_mass);
                obs[heavy].position = obs[heavy].position.add(
                    &obs[light]
                        .position
                        .substract(&obs[heavy].position)
                        .multiply(obs[light].mass / total_mass),
                );

                obs[heavy].radius *= (total_mass / obs[heavy].mass).powf(1.0 / 3.0);

                println!("{} collided into {}!", obs[light].name, obs[heavy].name);

                obs.remove(light);
                continue 'collision_loop;
            }
        }
        break;
    }
}