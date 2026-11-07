//! How well each integration method predicts reality: runs the solar system from the epoch to
//! a later date and compares every body's position with JPL Horizons.
//!
//! cargo run --release --example accuracy -- [end date] [--all] [only configs whose label contains ...]
//!
//! The end date needs a states file, see `fetch_horizons`. Defaults to 2027-01-01.

use std::time::Instant;

use solar_rust::astronomy::Bodies;
use solar_rust::ephemeris::{self, Date, State, KNOWN_BODIES};
use solar_rust::forces::Physics;
use solar_rust::integration::{IntegrationMethod, Integrator};

/// Bodies shown in the table, the summary rows cover all of them
const SHOWN: &[&str] = &[
    "Mercury", "Venus", "Earth", "Moon", "Mars", "Phobos", "Jupiter", "Io", "Saturn", "Titan",
    "Uranus", "Neptune", "Triton", "Pluto", "Ceres",
];

struct Config {
    label: String,
    method: IntegrationMethod,
    physics: Physics,
    max_dt: f64,
}

fn configs() -> Vec<Config> {
    let mut configs = Vec::new();
    let mut add = |label: &str, method, physics, max_dt| {
        configs.push(Config {
            label: label.to_string(),
            method,
            physics,
            max_dt,
        })
    };
    use IntegrationMethod::*;
    let (newton, full) = (Physics::NEWTONIAN, Physics::FULL);
    let oblate = Physics {
        oblateness: true,
        relativity: false,
    };

    add("sym4 600s", Symplectic(4), newton, 600.0);
    add("sym4 60s", Symplectic(4), newton, 60.0);
    add("rk4 60s", RK4, newton, 60.0);
    add("rk4 60s J", RK4, oblate, 60.0);
    add("rk4 60s J+GR", RK4, full, 60.0);
    add("ias15", Ias15, newton, f64::INFINITY);
    add("ias15 J", Ias15, oblate, f64::INFINITY);
    add("ias15 J+GR", Ias15, full, f64::INFINITY);
    configs
}

fn main() {
    let mut args = std::env::args().skip(1);
    let end = args
        .next()
        .map(|arg| Date::parse(&arg).expect("End date must be YYYY-MM-DD"))
        .unwrap_or(Date {
            year: 2027,
            month: 1,
            day: 1,
        });
    let mut filters: Vec<String> = args.collect();
    // --all: show every body, not just the main ones
    let show_all = filters.iter().any(|f| f == "--all");
    filters.retain(|f| f != "--all");

    let path = format!("{}/data/horizons_{end}.csv", env!("CARGO_MANIFEST_DIR"));
    let csv = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("No {path}, run: cargo run --example fetch_horizons -- {end}"));
    let reference = ephemeris::parse_states(&csv);
    let duration = ephemeris::EPOCH.seconds_until(end);

    let configs: Vec<Config> = configs()
        .into_iter()
        .filter(|c| filters.is_empty() || filters.iter().any(|f| c.label.contains(f.as_str())))
        .collect();

    println!(
        "Position error in km after {:.1} days ({} -> {end})\n",
        duration / 86400.0,
        ephemeris::EPOCH
    );

    // errors[config][body], NaN when there's no reference for the body
    let mut errors = Vec::new();
    let mut timings = Vec::new();
    for config in &configs {
        let started = Instant::now();
        let bodies = run(config, duration, &reference);
        timings.push((started.elapsed().as_secs_f64(), config));

        let row: Vec<f64> = KNOWN_BODIES
            .iter()
            .map(|known| {
                let reference = reference.iter().find(|(id, _)| *id == known.horizons_id);
                let index = bodies.info.iter().position(|i| &*i.name == known.name);
                match (reference, index) {
                    (Some((_, state)), Some(i)) => {
                        (bodies.position[i] - state.position).length() / 1000.0
                    }
                    _ => f64::NAN,
                }
            })
            .collect();
        errors.push(row);
    }

    print!("{:<10}", "");
    for config in &configs {
        print!("{:>13}", config.label);
    }
    println!();

    for (b, known) in KNOWN_BODIES.iter().enumerate() {
        if show_all || SHOWN.contains(&known.name) {
            print!("{:<10}", known.name);
            for row in &errors {
                print!("{:>13}", format_km(row[b]));
            }
            println!();
        }
    }

    type Summary = fn(&[f64]) -> f64;
    let summaries: [(&str, Summary); 2] = [("median", median), ("worst", worst)];
    for (label, summary) in summaries {
        print!("{label:<10}");
        for row in &errors {
            let known: Vec<f64> = row.iter().copied().filter(|e| !e.is_nan()).collect();
            print!("{:>13}", format_km(summary(&known)));
        }
        println!();
    }

    println!();
    for (seconds, config) in timings {
        println!("{:<14} {seconds:6.2} s", config.label);
    }
}

/// Spacecraft are left out: drag and engine burns make them unpredictable, and Horizons only
/// covers them for limited periods. Their ids are negative.
fn run(config: &Config, duration: f64, reference: &[(&str, State)]) -> Bodies {
    let mut bodies = ephemeris::solar_system();
    for known in KNOWN_BODIES {
        let spacecraft = known.horizons_id.starts_with('-');
        if spacecraft || !reference.iter().any(|(id, _)| *id == known.horizons_id) {
            let i = bodies
                .info
                .iter()
                .position(|i| &*i.name == known.name)
                .unwrap();
            bodies.remove(i);
        }
    }
    let mut integrator = Integrator::new(config.method, 1, config.physics);
    let mut merges = Vec::new();

    // Fixed step methods: shorten the step slightly so that a whole number of them ends
    // exactly at the end date
    let steps = (duration / config.max_dt).ceil();
    let max_dt = duration / steps;

    let mut time = 0.0;
    while time < duration {
        let remaining = duration - time;
        // Within rounding of the end: done
        if remaining < 1e-9 * duration {
            break;
        }
        time += integrator.step(&mut bodies, max_dt.min(remaining), &mut merges);
    }

    assert!(merges.is_empty(), "Unexpected collisions: {merges:?}");
    bodies
}

fn format_km(km: f64) -> String {
    if km.is_nan() {
        "-".into()
    } else if km < 10.0 {
        format!("{km:.3}")
    } else if km < 1.0E6 {
        format!("{km:.0}")
    } else {
        format!("{km:.2e}")
    }
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted.get(sorted.len() / 2).copied().unwrap_or(f64::NAN)
}

fn worst(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NAN, f64::max)
}
