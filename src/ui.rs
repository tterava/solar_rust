use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use solar_rust::astronomy::{Bodies, BodyId};
use solar_rust::engine::{Snapshot, Status};
use solar_rust::ephemeris;
use solar_rust::forces::Physics;
use solar_rust::integration::IntegrationMethod;

use crate::render::scene::{Label, TraceMode, ViewOptions};

const TEXT_REFRESH: Duration = Duration::from_millis(500);
const LINE_HEIGHT: f32 = 16.0;
const MARGIN: f32 = 5.0;
/// Shown while any surface maps are in use. Solar System Scope's license (CC BY 4.0) asks for it.
const TEXTURE_CREDITS: &str =
    "Surface maps: NASA, JPL, USGS, ESA/DLR/FU Berlin, Solar System Scope (CC BY 4.0)";

/// Mouse and keys, shown with F1: sections of (control, what it does)
const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Mouse",
        &[
            ("Left click", "Select a body and follow it"),
            ("Right drag", "Turn the camera around"),
            ("Scroll", "Zoom (with Shift: 5x faster)"),
        ],
    ),
    (
        "Simulation",
        &[
            ("Space", "Run / pause"),
            (
                "+  -",
                "Faster / slower: the target speed, or the time step",
            ),
            ("T", "Target speed mode on / off"),
            ("M", "Next integration method"),
            ("O", "Planets' oblateness on / off"),
            ("G", "General relativity on / off"),
            ("R", "Add a random planet around the Sun"),
            ("1 - 9", "Number of threads"),
            ("Q  W", "One thread fewer / more"),
        ],
    ),
    (
        "View",
        &[
            ("L", "Labels on / off"),
            (
                "P",
                "Traces: absolute (through space), relative (around what each body orbits), off",
            ),
            (
                "E",
                "Orbits on / off: the faint lines, where each body is headed",
            ),
            ("S", "True scale / small bodies magnified"),
        ],
    ),
    ("", &[("F1  Esc", "Show / hide this help")]),
];

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

/// Space between a body and its label (points)
const LABEL_GAP: f32 = 4.0;
/// Labels keep off bodies drawn at least this big (radius in points)
const LABEL_OBSTACLE: f32 = 5.0;

/// Body names next to the bodies, most important first, leaving out any that would cover a name
/// already placed or a body big enough to see, except the selected body's
pub fn paint_labels(painter: &Painter, rect: Rect, labels: &[Label]) {
    let mut placed: Vec<Rect> = labels
        .iter()
        .filter(|label| label.radius >= LABEL_OBSTACLE)
        .map(|label| Rect::from_center_size(label.center, egui::Vec2::splat(2.0 * label.radius)))
        .collect();
    for label in labels {
        let color = if label.selected {
            Color32::WHITE
        } else {
            Color32::from_gray(170)
        };
        let galley =
            painter.layout_no_wrap(label.name.to_string(), FontId::proportional(12.0), color);
        let size = galley.size();
        let corner = label.center + egui::vec2(label.radius + LABEL_GAP, -size.y / 2.0);
        let area = Rect::from_min_size(corner, size).expand(1.0);
        // The selected body is always named
        let blocked = !label.selected && placed.iter().any(|other| other.intersects(area));
        if blocked || !rect.contains_rect(area) {
            continue;
        }
        placed.push(area);
        painter.galley(corner, galley, color);
    }
}

/// A thin frame around the view
pub fn paint_border(painter: &Painter, rect: Rect) {
    painter.add(Shape::rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgb(100, 100, 255)),
        egui::StrokeKind::Inside,
    ));
}

/// The controls, in a panel over the middle of the view
pub fn paint_help(painter: &Painter, rect: Rect) {
    let key_font = FontId::monospace(14.0);
    let text_font = FontId::proportional(14.0);
    let heading_font = FontId::proportional(15.0);
    let (row, padding, gap) = (20.0, 18.0, 16.0);
    let lay_out = |text: &str, font: &FontId, color| {
        painter.layout_no_wrap(text.to_owned(), font.clone(), color)
    };

    let keys: Vec<_> = HELP
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(key, _)| lay_out(key, &key_font, Color32::WHITE))
        .collect();
    let texts: Vec<_> = HELP
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(_, text)| lay_out(text, &text_font, Color32::from_gray(200)))
        .collect();
    let key_width = keys.iter().map(|g| g.size().x).fold(0.0, f32::max);
    let text_width = texts.iter().map(|g| g.size().x).fold(0.0, f32::max);
    let headings = HELP.iter().filter(|(h, _)| !h.is_empty()).count();
    let width = padding * 2.0 + key_width + gap + text_width;
    let height =
        padding * 2.0 + row * 1.5 + (keys.len() + headings) as f32 * row + HELP.len() as f32 * 6.0;

    let panel = Rect::from_center_size(rect.center(), egui::vec2(width, height));
    painter.rect(
        panel,
        8.0,
        Color32::from_black_alpha(240),
        Stroke::new(1.0, Color32::from_gray(90)),
        egui::StrokeKind::Inside,
    );

    let left = panel.min.x + padding;
    let mut y = panel.min.y + padding;
    painter.text(
        Pos2::new(panel.center().x, y),
        Align2::CENTER_TOP,
        "Controls",
        FontId::proportional(18.0),
        Color32::WHITE,
    );
    y += row * 1.5;

    let mut rows = keys.into_iter().zip(texts);
    for (heading, entries) in HELP {
        y += 6.0;
        if !heading.is_empty() {
            painter.text(
                Pos2::new(left, y),
                Align2::LEFT_TOP,
                heading,
                heading_font.clone(),
                Color32::from_rgb(150, 190, 255),
            );
            y += row;
        }
        for _ in entries.iter() {
            let (key, text) = rows.next().unwrap();
            painter.galley(Pos2::new(left, y), key, Color32::WHITE);
            painter.galley(Pos2::new(left + key_width + gap, y), text, Color32::WHITE);
            y += row;
        }
    }
}

/// Status (bottom left) and selected body (top left) text. Refreshed twice a second so the
/// numbers stay readable, and immediately when a setting or the selection changes.
pub struct Overlay {
    status_lines: Vec<String>,
    description: Vec<String>,
    next_refresh: Instant,
    shown_for: Option<(Controls, Option<BodyId>, ViewOptions)>,
    /// Whether to credit the surface maps
    pub credits: bool,
}

/// The status values the user controls directly
type Controls = (bool, IntegrationMethod, Physics, usize, bool, f64);

fn controls(status: &Status) -> Controls {
    let speed_setting = if status.use_target_speed {
        status.target_speed
    } else {
        status.time_step
    };

    (
        status.running,
        status.method,
        status.physics,
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
            credits: false,
        }
    }
}

impl Overlay {
    pub fn update(&mut self, view: &Snapshot, selected: Option<BodyId>, options: ViewOptions) {
        let now = Instant::now();
        let key = (controls(&view.status), selected, options);

        if now < self.next_refresh && self.shown_for == Some(key) {
            return;
        }

        self.status_lines = status_text(&view.status, view.bodies.len(), options);
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

        painter.text(
            Pos2::new(rect.max.x - MARGIN, rect.min.y + MARGIN),
            Align2::RIGHT_TOP,
            "F1: help",
            FontId::proportional(12.0),
            Color32::from_gray(140),
        );

        if self.credits {
            painter.text(
                rect.max - egui::vec2(MARGIN, MARGIN),
                Align2::RIGHT_BOTTOM,
                TEXTURE_CREDITS,
                FontId::proportional(11.0),
                Color32::from_gray(110),
            );
        }
    }
}

fn status_text(status: &Status, body_count: usize, options: ViewOptions) -> Vec<String> {
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
        IntegrationMethod::Ias15 => "IAS15 - 15th order, adaptive".into(),
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

    // Adaptive methods choose their own steps, up to the setting
    let step = status.time_step;
    let step_line = if status.method.is_adaptive() {
        format!(
            "Max timestep: {step:.3} s {}, average {:.3} s {}",
            duration_detail(step),
            status.mean_step,
            duration_detail(status.mean_step)
        )
    } else {
        format!("Timestep: {step:.3} s {}", duration_detail(step))
    };

    let on_off = |on: bool| if on { "on" } else { "off" };
    let physics = format!(
        "Oblateness (O): {}, relativity (G): {}",
        on_off(status.physics.oblateness),
        on_off(status.physics.relativity)
    );

    // An adaptive method may not be able to keep up with the target
    let reached = if status.use_target_speed && status.running && status.speed < 0.95 * speed {
        format!(", reaching {:.1} d/s", status.speed / 86400.0)
    } else {
        String::new()
    };

    vec![
        format!(
            "Target speed: {:.1} d/s {speed_detail}{reached}",
            speed / 86400.0
        ),
        step_line,
        format!("Date: {} TDB", ephemeris::format_time(status.time_elapsed)),
        format!("Objects: {body_count}"),
        format!("Method: {method}"),
        physics,
        format!(
            "Labels (L): {}, traces (P): {}, orbits (E): {}, scale (S): {}",
            on_off(options.labels),
            match options.traces {
                TraceMode::Absolute => "absolute",
                TraceMode::Relative => "relative",
                TraceMode::Off => "off",
            },
            on_off(options.orbits),
            if options.true_scale {
                "true"
            } else {
                "magnified"
            }
        ),
        format!("Threads: {}", status.threads),
        format!("Speed: {:.0} n/s", status.steps_per_second),
    ]
}

/// "(2.50 h)" and the like for durations over a minute
fn duration_detail(seconds: f64) -> String {
    if seconds >= 86400.0 {
        format!("({:.2} d)", seconds / 86400.0)
    } else if seconds >= 3600.0 {
        format!("({:.2} h)", seconds / 3600.0)
    } else if seconds >= 60.0 {
        format!("({:.2} min)", seconds / 60.0)
    } else {
        String::new()
    }
}

fn description_text(bodies: &Bodies, i: usize) -> Vec<String> {
    let (mass, velocity, acceleration) =
        (bodies.mass[i], bodies.velocity[i], bodies.acceleration[i]);
    let position = bodies.position[i];

    // Values relative to the body it orbits, if any
    let [relative_speed, relative_acceleration, relative_energy] = match bodies.orbital_parents()[i]
    {
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
