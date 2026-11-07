use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::astronomy::{Bodies, BodyId, Merge, OrbitalObject};
use crate::ephemeris::format_time;
use crate::forces::Physics;
use crate::integration::{IntegrationMethod, Integrator};

/// Requests from the UI to the simulation thread
pub enum Command {
    TogglePause,
    /// Multiplies the time step, or the target speed when in target speed mode
    ScaleSpeed(f64),
    ToggleTargetSpeed,
    SetThreads(usize),
    AdjustThreads(isize),
    CycleMethod,
    ToggleOblateness,
    ToggleRelativity,
    /// Puts a body on an orbit around `parent`
    AddOrbiting {
        object: OrbitalObject,
        parent: BodyId,
    },
    Quit,
}

/// Settings and measurements of the simulation
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub running: bool,
    pub method: IntegrationMethod,
    pub physics: Physics,
    pub threads: usize,
    /// Simulated seconds per step
    pub time_step: f64,
    /// Simulated seconds per real second
    pub target_speed: f64,
    /// When set, the time step is adjusted to reach `target_speed`.
    /// Otherwise the time step is fixed and `target_speed` reports the resulting speed.
    pub use_target_speed: bool,
    /// Simulated seconds since the start
    pub time_elapsed: f64,
    pub steps_per_second: f64,
    /// Average step actually taken in the latest batch
    pub mean_step: f64,
    /// Simulated seconds per real second actually reached
    pub speed: f64,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            running: false,
            method: IntegrationMethod::Ias15,
            physics: Physics::FULL,
            threads: 1,
            time_step: 60.0,
            target_speed: 86400.0,
            use_target_speed: true,
            time_elapsed: 0.0,
            steps_per_second: 0.0,
            mean_step: 0.0,
            speed: 0.0,
        }
    }
}

/// The simulation state as last published by the engine
#[derive(Default)]
pub struct Snapshot {
    pub bodies: Bodies,
    pub status: Status,
}

/// Runs the simulation on its own thread. The UI talks to it with [`Command`]s and reads
/// back [`Snapshot`]s, so neither side ever waits for the other for long.
pub struct Engine {
    commands: Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the simulation thread, paused. State is published `publish_rate` times per second.
    pub fn new(bodies: Bodies, status: Status, publish_rate: f64) -> Self {
        let snapshot = Arc::new(Mutex::new(Snapshot {
            bodies: bodies.clone(),
            status,
        }));
        let (commands, receiver) = mpsc::channel();

        let simulation = Simulation {
            integrator: Integrator::new(status.method, status.threads, status.physics),
            bodies,
            status,
            publish_rate,
            batch_steps: 1,
            merges: Vec::new(),
            rng: StdRng::from_rng(&mut rand::rng()),
        };

        let shared = snapshot.clone();
        let thread = thread::spawn(move || simulation.run(receiver, &shared));

        Engine {
            commands,
            snapshot,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) {
        // Only fails if the simulation thread has died, which will already have been reported
        let _ = self.commands.send(command);
    }

    /// Copies the latest published state into `view`, reusing its allocations
    pub fn read_latest(&self, view: &mut Snapshot) {
        let snapshot = self.snapshot.lock().unwrap();
        view.bodies.clone_from(&snapshot.bodies);
        view.status = snapshot.status;
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Simulation {
    bodies: Bodies,
    status: Status,
    integrator: Integrator,
    publish_rate: f64,
    /// Steps to run before publishing, sized to take about 1 / publish_rate seconds
    batch_steps: u64,
    merges: Vec<Merge>,
    rng: StdRng,
}

impl Simulation {
    fn run(mut self, commands: Receiver<Command>, shared: &Mutex<Snapshot>) {
        loop {
            // While paused, sleep until a command arrives
            let first = if self.status.running {
                None
            } else {
                match commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                }
            };

            for command in first.into_iter().chain(commands.try_iter()) {
                match command {
                    Command::Quit => return,
                    command => self.apply(command),
                }
            }

            if self.status.running {
                self.run_batch();
            }

            let mut snapshot = shared.lock().unwrap();
            snapshot.bodies.clone_from(&self.bodies);
            snapshot.status = self.status;
        }
    }

    fn apply(&mut self, command: Command) {
        let Status {
            method,
            threads,
            physics,
            ..
        } = self.status;

        match command {
            Command::TogglePause => {
                self.status.running = !self.status.running && self.bodies.len() >= 2
            }
            Command::ScaleSpeed(factor) => {
                if self.status.use_target_speed {
                    self.status.target_speed *= factor;
                } else {
                    self.status.time_step *= factor;
                }
            }
            Command::ToggleTargetSpeed => {
                self.status.use_target_speed = !self.status.use_target_speed
            }
            Command::SetThreads(n) => self.set_integrator(method, n, physics),
            Command::AdjustThreads(change) => {
                self.set_integrator(method, threads.saturating_add_signed(change), physics)
            }
            Command::CycleMethod => self.set_integrator(method.next(), threads, physics),
            Command::ToggleOblateness => {
                let physics = Physics {
                    oblateness: !physics.oblateness,
                    ..physics
                };
                self.set_integrator(method, threads, physics)
            }
            Command::ToggleRelativity => {
                let physics = Physics {
                    relativity: !physics.relativity,
                    ..physics
                };
                self.set_integrator(method, threads, physics)
            }
            Command::AddOrbiting { object, parent } => {
                if let Some(parent) = self.bodies.index_of(parent) {
                    self.bodies.push_orbiting(object, parent, &mut self.rng);
                }
            }
            Command::Quit => {}
        }
    }

    fn set_integrator(&mut self, method: IntegrationMethod, threads: usize, physics: Physics) {
        let threads = threads.max(1);
        let current = (self.status.method, self.status.threads, self.status.physics);

        if (method, threads, physics) != current {
            self.integrator = Integrator::new(method, threads, physics);
            self.status.method = method;
            self.status.threads = threads;
            self.status.physics = physics;
        }
    }

    fn run_batch(&mut self) {
        let started = Instant::now();
        let max_dt = self.status.time_step;
        let (mut steps, mut advanced) = (0, 0.0);

        while steps < self.batch_steps {
            // Summed per batch and added to the total once, to avoid accumulating rounding errors
            advanced += self
                .integrator
                .step(&mut self.bodies, max_dt, &mut self.merges);
            steps += 1;

            for merge in self.merges.drain(..) {
                let time = format_time(self.status.time_elapsed + advanced);
                println!("{time}: {} collided into {}!", merge.absorbed, merge.into);
            }

            if self.bodies.len() < 2 {
                self.status.running = false;
                break;
            }
        }

        self.status.time_elapsed += advanced;

        let elapsed = started.elapsed().as_secs_f64();
        let steps_per_second = if elapsed > 0.0 {
            steps as f64 / elapsed
        } else {
            // Too fast to measure: double the batch until it isn't
            2.0 * steps as f64 * self.publish_rate
        };

        self.batch_steps = ((steps_per_second / self.publish_rate).round() as u64).max(1);
        self.status.steps_per_second = steps_per_second;
        self.status.mean_step = advanced / steps as f64;
        self.status.speed = self.status.mean_step * steps_per_second;

        // Adaptive methods may take shorter steps than the setting, so the time step setting
        // is a maximum, and the speed is measured from the steps actually taken
        if self.status.use_target_speed {
            self.status.time_step = self.status.target_speed / steps_per_second;
        } else {
            self.status.target_speed = self.status.speed;
        }
    }
}
