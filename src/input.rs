use egui::{Key, PointerButton, Response, Ui};

use solar_rust::astronomy::OrbitalObject;
use solar_rust::engine::Command;

use crate::camera::FAST_ZOOM_NOTCHES;
use crate::{ui, SolarApp};

const THREAD_KEYS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

pub fn handle_input(app: &mut SolarApp, ui: &Ui, response: &Response) {
    // Rotate camera by dragging with the right mouse button
    if response.dragged_by(PointerButton::Secondary) {
        let delta = response.drag_delta();
        app.camera.rotate(delta.x, delta.y);
    }

    // Select an object with left click
    if response.clicked_by(PointerButton::Primary) {
        if let Some(id) = response
            .interact_pointer_pos()
            .and_then(|pos| ui::hit_test(&app.targets, pos))
        {
            app.select(id);
        }
    }

    // Zoom with mouse wheel, faster while holding shift
    if response.contains_pointer() {
        let (delta, shift) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
        // egui turns scrolling into horizontal scrolling while shift is down
        let scroll = if shift {
            (delta.x + delta.y) * FAST_ZOOM_NOTCHES
        } else {
            delta.y
        };
        if scroll != 0.0 {
            app.camera.zoom(scroll);
        }
    }

    let pressed = |key| ui.input(|i| i.key_pressed(key));
    let engine = &app.engine;

    if pressed(Key::Space) {
        engine.send(Command::TogglePause);
    }
    // Numpad +/-: change the time step, or the target speed in target speed mode
    if pressed(Key::Plus) {
        engine.send(Command::ScaleSpeed(1.2));
    }
    if pressed(Key::Minus) {
        engine.send(Command::ScaleSpeed(1.0 / 1.2));
    }
    // 1-9: thread count, q/w: one less/more thread
    for (threads, key) in (1..).zip(THREAD_KEYS) {
        if pressed(key) {
            engine.send(Command::SetThreads(threads));
        }
    }
    if pressed(Key::Q) {
        engine.send(Command::AdjustThreads(-1));
    }
    if pressed(Key::W) {
        engine.send(Command::AdjustThreads(1));
    }
    // m: cycle integration method
    if pressed(Key::M) {
        engine.send(Command::CycleMethod);
    }
    // r: add a random planet around the first body (the Sun)
    if pressed(Key::R) {
        if let Some(sun) = app.view.bodies.info.first() {
            engine.send(Command::AddOrbiting {
                object: OrbitalObject::random_planet(&mut rand::rng()),
                parent: sun.id,
            });
        }
    }
    // t: toggle target speed mode
    if pressed(Key::T) {
        engine.send(Command::ToggleTargetSpeed);
    }
    // o / g: toggle the planets' oblateness and general relativity
    if pressed(Key::O) {
        engine.send(Command::ToggleOblateness);
    }
    if pressed(Key::G) {
        engine.send(Command::ToggleRelativity);
    }
}
