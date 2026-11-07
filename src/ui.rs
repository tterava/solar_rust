use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use solar_rust::astronomy::{Bodies, BodyId};
use solar_rust::engine::{Snapshot, Status};
use solar_rust::integration::IntegrationMethod;

use crate::camera::Camera;

const TEXT_REFRESH: Duration = Duration::from_millis(500);
const LINE_HEIGHT: f32 = 16.0;
const MARGIN: f32 = 5.0;

/// A body as drawn on screen, used for click hit-testing
pub struct Target {
    pub id: BodyId,
    pub center: Pos2,
    pub radius: f32,
}

/// Returns the nearest (topmost) body under `pos`. `targets` are in drawing order, farthest first.
pub fn hit_test(targets: &[Target], pos: Pos2) -> Option<BodyId> {
    targets
        .iter()
        .rev()
        .find(|t| t.center.distance_sq(pos) <= t.radius * t.radius)
        .map(|t| t.id)
}

/// Draws all bodies farthest to nearest, and records where each one ended up in `targets`.
pub fn paint_bodies(
    painter: &Painter,
    rect: Rect,
    bodies: &Bodies,
    camera: &Camera,
    targets: &mut Vec<Target>,
) {
    const MAX_MAGNIFIED_RADIUS: f64 = 10.0;
    const MIN_RADIUS: f64 = 3.0;

    let screen_center = rect.center();
    let screen_scalar = rect.width() as f64 / 2.0 / (camera.fov / 2.0).to_radians().tan();
    let transform = camera.view_transform();

    // Painter's algorithm: sort by distance so nearer bodies are drawn over farther ones
    let camera_position = camera.position();
    let mut draw_order: Vec<(f64, usize)> = bodies
        .position
        .iter()
        .enumerate()
        .map(|(i, p)| (p.distance_squared(camera_position), i))
        .collect();
    draw_order.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));

    targets.clear();

    for (_, i) in draw_order {
        let pos = transform.transform_point3(bodies.position[i]);

        // Behind the camera
        if pos.z >= 1.0 {
            continue;
        }

        let distance_scalar = 1.0 - pos.z;
        let center = Pos2::new(
            screen_center.x + (pos.x / distance_scalar * screen_scalar) as f32,
            screen_center.y - (pos.y / distance_scalar * screen_scalar) as f32,
        );

        // Small bodies are magnified to stay visible, until they're big enough on their own
        let info = &bodies.info[i];
        let true_radius = bodies.radius[i] / camera.distance / distance_scalar * screen_scalar;
        let radius = if true_radius < MAX_MAGNIFIED_RADIUS {
            (true_radius * info.magnification.cbrt()).min(MAX_MAGNIFIED_RADIUS)
        } else {
            true_radius
        }
        .max(MIN_RADIUS) as f32;

        if !rect.expand(radius).contains(center) {
            continue;
        }

        let [r, g, b] = info.color;
        painter.add(Shape::circle_filled(
            center,
            radius,
            Color32::from_rgb(r, g, b),
        ));

        targets.push(Target {
            id: info.id,
            center,
            radius,
        });
    }

    painter.add(Shape::rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgb(100, 100, 255)),
        egui::StrokeKind::Inside,
    ));
}

/// Status (bottom left) and selected body (top left) text. Refreshed twice a second so the
/// numbers stay readable, and immediately when a setting or the selection changes.
pub struct Overlay {
    status_lines: Vec<String>,
    description: Vec<String>,
    next_refresh: Instant,
    shown_for: Option<(Controls, Option<BodyId>)>,
}

/// The status values the user controls directly
type Controls = (bool, IntegrationMethod, usize, bool, f64);

fn controls(status: &Status) -> Controls {
    let speed_setting = if status.use_target_speed {
        status.target_speed
    } else {
        status.time_step
    };

    (
        status.running,
        status.method,
        status.threads,
        status.use_target_speed,
        speed_setting,
    )
}

impl Default for Overlay {
    fn default() -> Self {
        Overlay {
            status_lines: Vec::new(),
            description: Vec::new(),
            next_refresh: Instant::now(),
            shown_for: None,
        }
    }
}

impl Overlay {
    pub fn update(&mut self, view: &Snapshot, selected: Option<BodyId>) {
        let now = Instant::now();
        let key = (controls(&view.status), selected);

        if now < self.next_refresh && self.shown_for == Some(key) {
            return;
        }

        self.status_lines = status_text(&view.status, view.bodies.len());
        self.description = selected
            .and_then(|id| view.bodies.index_of(id))
            .map(|i| description_text(&view.bodies, i))
            .unwrap_or_default();
        self.next_refresh = now + TEXT_REFRESH;
        self.shown_for = Some(key);
    }

    pub fn paint(&self, painter: &Painter, rect: Rect, use_target_speed: bool) {
        let font = FontId::monospace(13.0);
        let white = Color32::WHITE;

        // The setting currently under control is drawn "bold": twice, with a small offset,
        // since the monospace font has no bold face.
        let bold_line = if use_target_speed { 0 } else { 1 };
        let status_top = rect.max.y - self.status_lines.len() as f32 * LINE_HEIGHT - MARGIN;

        for (i, line) in self.status_lines.iter().enumerate() {
            let pos = Pos2::new(rect.min.x + MARGIN, status_top + i as f32 * LINE_HEIGHT);
            if i == bold_line {
                painter.text(
                    pos + egui::vec2(0.5, 0.0),
                    Align2::LEFT_TOP,
                    line,
                    font.clone(),
                    white,
                );
            }
            painter.text(pos, Align2::LEFT_TOP, line, font.clone(), white);
        }

        for (i, line) in self.description.iter().enumerate() {
            let pos = Pos2::new(
                rect.min.x + MARGIN,
                rect.min.y + MARGIN + i as f32 * LINE_HEIGHT,
            );
            painter.text(pos, Align2::LEFT_TOP, line, font.clone(), white);
        }
    }
}

fn status_text(status: &Status, body_count: usize) -> Vec<String> {
    let method = match status.method {
        IntegrationMethod::Symplectic(k) => {
            let order = match k {
                1 => "1st",
                2 => "2nd",
                3 => "3rd",
                4 => "4th",
                _ => "??",
            };
            format!("Symplectic - {order} order")
        }
        IntegrationMethod::RK4 => "Runge-Kutta 4".into(),
    };

    let speed = status.target_speed;
    let speed_detail = if speed >= 86400.0 * 365.0 {
        format!("({:.2} y/s)", speed / 86400.0 / 365.0)
    } else if speed <= 3600.0 {
        format!("({:.2} m/s)", speed / 60.0)
    } else if speed <= 86400.0 {
        format!("({:.2} h/s)", speed / 3600.0)
    } else {
        String::new()
    };

    let step = status.time_step;
    let step_detail = if step >= 86400.0 {
        format!("({:.2} d)", step / 86400.0)
    } else if step >= 3600.0 {
        format!("({:.2} h)", step / 3600.0)
    } else if step >= 60.0 {
        format!("({:.2} m)", step / 60.0)
    } else {
        String::new()
    };

    vec![
        format!("Target speed: {:.1} d/s {speed_detail}", speed / 86400.0),
        format!("Timestep: {step:.3} s {step_detail}"),
        format!(
            "Simulation time: {:.2} y",
            status.time_elapsed / (86400.0 * 365.0)
        ),
        format!("Objects: {body_count}"),
        format!("Method: {method}"),
        format!("Threads: {}", status.threads),
        format!("Speed: {:.0} n/s", status.steps_per_second),
    ]
}

fn description_text(bodies: &Bodies, i: usize) -> Vec<String> {
    let (mass, velocity, acceleration) =
        (bodies.mass[i], bodies.velocity[i], bodies.acceleration[i]);
    let position = bodies.position[i];

    // Values relative to the body it orbits, if any
    let [relative_speed, relative_acceleration, relative_energy] = match bodies.orbital_parent(i) {
        Some(p) => {
            let parent = &bodies.info[p].name;
            let relative_velocity = velocity - bodies.velocity[p];
            [
                format!(
                    " - {:.4e} m/s compared to {parent}",
                    relative_velocity.length()
                ),
                format!(
                    " - {:.4e} m/s^2 compared to {parent}",
                    (acceleration - bodies.acceleration[p]).length()
                ),
                format!(
                    " - {:.4e} J compared to {parent}",
                    0.5 * mass * relative_velocity.length_squared()
                ),
            ]
        }
        None => Default::default(),
    };

    vec![
        format!("Name: {}", bodies.info[i].name),
        format!("Mass: {mass:.4e} kg"),
        format!("Radius: {:.4e} m", bodies.radius[i]),
        format!("Speed: {:.4e} m/s{relative_speed}", velocity.length()),
        format!(
            "Acceleration magnitude: {:.4e} m/s^2{relative_acceleration}",
            acceleration.length()
        ),
        format!(
            "Kinetic energy: {:.4e} J{relative_energy}",
            0.5 * mass * velocity.length_squared()
        ),
        String::new(),
        format!(
            "Position: [{:.4e}, {:.4e}, {:.4e}]",
            position.x, position.y, position.z
        ),
        format!(
            "Velocity: [{:.4e}, {:.4e}, {:.4e}]",
            velocity.x, velocity.y, velocity.z
        ),
        format!(
            "Acceleration: [{:.4e}, {:.4e}, {:.4e}]",
            acceleration.x, acceleration.y, acceleration.z
        ),
    ]
}
