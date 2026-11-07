mod camera;
mod input;
mod ui;

use std::time::Duration;

use eframe::egui;
use rand::SeedableRng;

use solar_rust::astronomy::{self, BodyId};
use solar_rust::engine::{Engine, Snapshot, Status};

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
    overlay: ui::Overlay,
}

impl SolarApp {
    fn new() -> Self {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let bodies = astronomy::solar_system(&mut rng);

        SolarApp {
            engine: Engine::new(bodies, Status::default(), PUBLISH_RATE),
            view: Snapshot::default(),
            camera: Camera::default(),
            selected: None,
            targets: Vec::new(),
            overlay: ui::Overlay::default(),
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

            ui::paint_bodies(
                &painter,
                rect,
                &self.view.bodies,
                &self.camera,
                &mut self.targets,
            );
            self.overlay.update(&self.view, self.selected);
            self.overlay
                .paint(&painter, rect, self.view.status.use_target_speed);
        });

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
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 540.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Solar system simulator",
        options,
        Box::new(|_| Ok(Box::new(SolarApp::new()))),
    )
}
