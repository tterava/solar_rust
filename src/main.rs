mod camera;
mod input;
mod render;
mod ui;

use std::time::{Duration, Instant};

use eframe::egui;
use solar_rust::astronomy::BodyId;
use solar_rust::engine::{Command, Engine, Snapshot, Status};
use solar_rust::ephemeris;

use crate::camera::Camera;

/// How many times per second the simulation publishes its state for drawing
const PUBLISH_RATE: f64 = 60.0;

pub struct SolarApp {
    engine: Engine,
    /// Local copy of the latest simulation state, so drawing never holds the engine's lock
    view: Snapshot,
    camera: Camera,
    selected: Option<BodyId>,
    /// Bodies as drawn last frame, for clicking on them
    targets: Vec<ui::Target>,
    scene: render::scene::Scene,
    /// What the view shows: labels, traces, orbits, scale
    options: render::scene::ViewOptions,
    /// Where bodies have been
    traces: render::traces::Traces,
    /// Where to save the 3D view once the camera has settled, from SOLAR_CAPTURE
    capture: Option<render::Capture>,
    /// Not before this, from SOLAR_CAPTURE_AFTER
    capture_after: Instant,
    /// How far from the followed body to look from once there (its radii), from SOLAR_DISTANCE
    distance_radii: Option<f64>,
    overlay: ui::Overlay,
    /// Whether the controls are shown (F1), from SOLAR_HELP at start
    help: bool,
    /// When the app started, for timing traces
    started: Instant,
}

impl SolarApp {
    fn new(creation: &eframe::CreationContext) -> Self {
        let render_state = creation
            .wgpu_render_state
            .as_ref()
            .expect("The 3D view needs eframe's wgpu renderer");
        let bodies = ephemeris::solar_system();
        render::init(
            render_state,
            bodies.info.iter().map(|b| b.name.clone()).collect(),
        );

        let mut app = SolarApp {
            engine: Engine::new(bodies, Status::default(), PUBLISH_RATE),
            view: Snapshot::default(),
            camera: Camera::default(),
            selected: None,
            targets: Vec::new(),
            scene: render::scene::Scene::default(),
            options: render::scene::ViewOptions {
                traces: match std::env::var("SOLAR_TRACES").as_deref() {
                    Ok("relative") => render::scene::TraceMode::Relative,
                    Ok("off") => render::scene::TraceMode::Off,
                    _ => render::scene::TraceMode::Absolute,
                },
                ..Default::default()
            },
            traces: Default::default(),
            capture: std::env::var_os("SOLAR_CAPTURE").map(|path| render::Capture {
                path: path.into(),
                done: Default::default(),
            }),
            capture_after: Instant::now()
                + Duration::from_secs_f64(
                    std::env::var("SOLAR_CAPTURE_AFTER")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                ),
            distance_radii: std::env::var("SOLAR_DISTANCE")
                .ok()
                .and_then(|s| s.parse().ok()),
            overlay: ui::Overlay::default(),
            help: std::env::var_os("SOLAR_HELP").is_some(),
            started: Instant::now(),
        };
        app.overlay.credits = render::textures::any_present();
        app.engine.read_latest(&mut app.view);
        app.follow_from_environment();
        app.run_from_environment();
        app
    }

    /// With SOLAR_SPEED=<days per second>, the simulation starts running at that speed.
    /// SOLAR_TRACES=relative|absolute|off picks how traces are drawn.
    fn run_from_environment(&mut self) {
        let Some(days) = std::env::var("SOLAR_SPEED")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
        else {
            return;
        };
        let speed = self.view.status.target_speed;
        self.engine
            .send(Command::ScaleSpeed(days * 86400.0 / speed));
        self.engine.send(Command::TogglePause);
    }

    /// Selects the body named in the SOLAR_FOLLOW environment variable, if any, and looks at it
    /// from SOLAR_DISTANCE of its radii. With SOLAR_CAPTURE=file.png, the view is then saved to
    /// that file and the app closes: handy for checking the view without a screenshot. The capture
    /// waits for the camera to settle, the textures to load, and SOLAR_CAPTURE_AFTER seconds.
    fn follow_from_environment(&mut self) {
        let Ok(name) = std::env::var("SOLAR_FOLLOW") else {
            return;
        };
        if let Some(info) = self.view.bodies.info.iter().find(|i| *i.name == *name) {
            self.select(info.id);
        }
    }

    fn select(&mut self, id: BodyId) {
        if self.selected == Some(id) {
            return;
        }

        if let Some(i) = self.view.bodies.index_of(id) {
            self.selected = Some(id);
            self.camera.fly_to(self.view.bodies.position[i]);
        }
    }

    fn update_camera(&mut self) {
        let Some(id) = self.selected else {
            return;
        };

        match self.view.bodies.index_of(id) {
            Some(i) => {
                let radius = self.view.bodies.radius[i];
                self.camera.follow(self.view.bodies.position[i], radius);
                if !self.camera.is_animating() {
                    if let Some(radii) = self.distance_radii.take() {
                        self.camera.distance = radii * radius;
                    }
                }
            }
            // Merged into another body in a collision
            None => {
                self.selected = None;
                self.camera.release();
            }
        }
    }
}

impl eframe::App for SolarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.engine.read_latest(&mut self.view);

        let frame = egui::Frame::default()
            .fill(egui::Color32::BLACK)
            .inner_margin(egui::Margin::same(0));

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
            let rect = response.rect;

            input::handle_input(self, ui, &response);
            self.update_camera();

            let time = self.view.status.time_elapsed;
            let parents = self.view.bodies.orbital_parents();
            let real = self.started.elapsed().as_secs_f64();
            self.traces.update(&self.view.bodies, &parents, time, real);
            let view = render::scene::View {
                bodies: &self.view.bodies,
                camera: &self.camera,
                viewport: rect,
                days: render::rotation::days_since_j2000(time),
                options: self.options,
                selected: self.selected,
                traces: &self.traces,
                parents: &parents,
            };
            render::scene::build(&view, &mut self.scene, &mut self.targets);
            let settled = !self.camera.is_animating()
                && render::textures::all_loaded()
                && Instant::now() >= self.capture_after;
            let capture = self.capture.clone().filter(|_| settled);
            render::paint(&painter, rect, &self.scene, capture);
            ui::paint_labels(&painter, rect, &self.scene.labels);
            ui::paint_border(&painter, rect);
            self.overlay.update(&self.view, self.selected, self.options);
            self.overlay
                .paint(&painter, rect, self.view.status.use_target_speed);
            if self.help {
                ui::paint_help(&painter, rect);
            }
        });

        if let Some(capture) = &self.capture {
            if capture.done.load(std::sync::atomic::Ordering::Relaxed) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.ctx().request_repaint();
        }

        // Redraw continuously while something moves (smooth scrolling included, egui doesn't
        // schedule those frames itself); otherwise just check back now and then
        let scrolling = ui.input(|i| i.is_scrolling());
        if self.view.status.running || self.camera.is_animating() || scrolling {
            ui.ctx().request_repaint();
        } else {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Solar system simulator",
        options,
        Box::new(|creation| Ok(Box::new(SolarApp::new(creation)))),
    )
}
