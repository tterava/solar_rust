mod camera;
mod input;
mod render;
mod ui;

use std::time::Duration;

use eframe::egui;
use solar_rust::astronomy::BodyId;
use solar_rust::engine::{Engine, Snapshot, Status};
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
    /// Where to save the 3D view once the camera has settled, from SOLAR_CAPTURE
    capture: Option<render::Capture>,
    overlay: ui::Overlay,
}

impl SolarApp {
    fn new(creation: &eframe::CreationContext) -> Self {
        let render_state = creation
            .wgpu_render_state
            .as_ref()
            .expect("The 3D view needs eframe's wgpu renderer");
        render::init(render_state);
        let bodies = ephemeris::solar_system();

        let mut app = SolarApp {
            engine: Engine::new(bodies, Status::default(), PUBLISH_RATE),
            view: Snapshot::default(),
            camera: Camera::default(),
            selected: None,
            targets: Vec::new(),
            scene: render::scene::Scene::default(),
            capture: std::env::var_os("SOLAR_CAPTURE").map(|path| render::Capture {
                path: path.into(),
                done: Default::default(),
            }),
            overlay: ui::Overlay::default(),
        };
        app.engine.read_latest(&mut app.view);
        app.follow_from_environment();
        app
    }

    /// Selects the body named in the SOLAR_FOLLOW environment variable, if any. With
    /// SOLAR_CAPTURE=file.png, the view is then saved to that file and the app closes: handy for
    /// checking the view without a screenshot.
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
            Some(i) => self
                .camera
                .follow(self.view.bodies.position[i], self.view.bodies.radius[i]),
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

            render::scene::build(
                &self.view.bodies,
                &self.camera,
                rect,
                &mut self.scene,
                &mut self.targets,
            );
            // Captured once the fly-to animation is over
            let capture = self.capture.clone().filter(|_| !self.camera.is_animating());
            render::paint(&painter, rect, &self.scene, capture);
            ui::paint_border(&painter, rect);
            self.overlay.update(&self.view, self.selected);
            self.overlay
                .paint(&painter, rect, self.view.status.use_target_speed);
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
