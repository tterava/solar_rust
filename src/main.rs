mod astronomy;
mod camera;
mod engine;
mod events;
mod integration;
mod ui;

use std::time::Instant;

use eframe::egui;
use rand::SeedableRng;
use uuid::Uuid;

use crate::camera::Camera;
use crate::engine::Engine;
use crate::ui::TargetData;

const FRAMERATE: u32 = 60;

pub struct SolarApp {
    pub engine: Engine,
    pub camera: Camera,
    pub current_target: Option<Uuid>,
    pub targets: Vec<TargetData>,
    pub next_status_update: Instant,
    pub status_lines: Vec<String>,
    pub object_description: Vec<String>,
}

impl eframe::App for SolarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame = egui::Frame::default()
            .fill(egui::Color32::BLACK)
            .inner_margin(egui::Margin::same(0));

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            ui::paint(self, &painter, &response);
            events::handle_input(self, ui, &response);
        });

        ui.ctx().request_repaint();
    }
}

fn main() -> eframe::Result {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let engine = Engine::default(&mut rng);
    *engine.framerate.lock().unwrap() = FRAMERATE;
    engine.params.lock().unwrap().target_speed = 86400.0 * 1.0;

    let app = SolarApp {
        engine,
        camera: Camera::default(),
        current_target: None,
        targets: Vec::new(),
        next_status_update: Instant::now(),
        status_lines: Vec::new(),
        object_description: Vec::new(),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 540.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Solar system simulator",
        options,
        Box::new(move |_| Ok(Box::new(app))),
    )
}
