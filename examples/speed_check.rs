//! How fast the real solar system runs with IAS15 and full physics, in simulated days per
//! second, optionally without some bodies:
//!
//! cargo run --release --example speed_check -- [threads] ["Body name,Other body"]

use std::time::Instant;

use solar_rust::ephemeris::solar_system;
use solar_rust::forces::Physics;
use solar_rust::integration::{IntegrationMethod, Integrator};

const DAYS: f64 = 10.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let threads: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);

    let mut bodies = solar_system();
    for name in args.next().iter().flat_map(|names| names.split(',')) {
        let Some(i) = bodies.info.iter().position(|b| &*b.name == name) else {
            panic!("No body called {name}");
        };
        bodies.remove(i);
    }

    let mut integrator = Integrator::new(IntegrationMethod::Ias15, threads, Physics::FULL);
    let mut merges = Vec::new();
    let (start, mut time, mut steps) = (Instant::now(), 0.0, 0);
    while time < DAYS * 86400.0 {
        time += integrator.step(&mut bodies, f64::INFINITY, &mut merges);
        steps += 1;
    }

    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{} bodies, {threads} threads: {:.2} days/s, mean step {:.0} s, {:.0} steps/s",
        bodies.len(),
        DAYS / seconds,
        time / steps as f64,
        steps as f64 / seconds
    );
}
