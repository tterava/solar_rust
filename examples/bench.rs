//! Headless throughput benchmark of the engine, as reported by "Speed: n/s" in the UI.
//!
//! cargo run --release --example bench [body counts...]

use std::thread;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::SeedableRng;

use solar_rust::astronomy::{solar_system, OrbitalObject};
use solar_rust::engine::{Command, Engine, Snapshot, Status};
use solar_rust::integration::IntegrationMethod;

const TIME_STEP: f64 = 0.01; // Small enough that nothing collides during the run

fn steps_per_second(bodies: usize, threads: usize, method: IntegrationMethod) -> f64 {
    let mut rng = StdRng::seed_from_u64(42);
    let mut system = solar_system(&mut rng);
    while system.len() < bodies {
        system.push_orbiting(OrbitalObject::random_planet(&mut rng), 0, &mut rng);
    }

    let status = Status {
        method,
        threads,
        time_step: TIME_STEP,
        ..Status::default()
    };
    let engine = Engine::new(system, status, 60.0);
    engine.send(Command::TogglePause);

    let mut view = Snapshot::default();
    let mut elapsed = || {
        engine.read_latest(&mut view);
        (view.status.time_elapsed, Instant::now())
    };

    // Warm up until the batch size has settled
    while elapsed().0 == 0.0 {
        thread::sleep(Duration::from_millis(50));
    }
    thread::sleep(Duration::from_millis(500));

    let (sim_start, wall_start) = elapsed();
    thread::sleep(Duration::from_secs(3));
    let (sim_end, wall_end) = elapsed();

    (sim_end - sim_start) / TIME_STEP / (wall_end - wall_start).as_secs_f64()
}

fn main() {
    let counts: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().unwrap())
        .collect();
    let counts = if counts.is_empty() {
        vec![13, 100, 500, 2000]
    } else {
        counts
    };

    for n in counts {
        for threads in [1, 2, 4, 8, 12] {
            let speed = steps_per_second(n, threads, IntegrationMethod::Symplectic(4));
            println!("n={n:5} sym4 threads={threads:2}: {speed:>12.0} steps/s");
        }
        let speed = steps_per_second(n, 1, IntegrationMethod::RK4);
        println!("n={n:5} rk4  threads= 1: {speed:>12.0} steps/s");
    }
}
