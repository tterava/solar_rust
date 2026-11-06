use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Painter, Pos2, Response, Shape, Stroke};
use glam::DVec3;
use uuid::Uuid;

use crate::engine;
use crate::integration::IntegrationMethod;
use crate::SolarApp;

pub struct TargetData {
    pub uuid: Uuid,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

pub fn paint(app: &mut SolarApp, painter: &Painter, response: &Response) {
    let now = Instant::now();

    if app.next_status_update <= now {
        app.status_lines = get_status_text(app);
        app.object_description = get_object_description_text(app);
        app.next_status_update = Instant::now() + Duration::from_millis(500);
    }

    let rect = response.rect;
    let screen_width = rect.width() as f64;
    let screen_height = rect.height() as f64;

    let bodies = app.engine.objects.lock().unwrap();
    let camera = &mut app.camera;

    let screen_scalar = screen_width / 2.0 / (camera.fov / 2.0).to_radians().tan();

    if let Some(target) = app.current_target {
        if let Some(b) = bodies.iter().find(|x| x.uuid == target) {
            camera.target = match camera.get_animation_position(b.position, b.radius) {
                Some((target, distance)) => {
                    camera.distance = distance;

                    if target == b.position {
                        camera.animation_start = None;
                    }

                    target
                }
                None => b.position,
            };
        } else {
            app.current_target = None;
            camera.target = DVec3::ZERO;
        }
    }

    let transform = camera.get_full_transformation();

    // Sort farthest-to-nearest. Distance keys are precomputed so the comparator does no math,
    // and camera position is computed once instead of once per comparison.
    let camera_position = camera.get_position();
    let mut sorted_indices: Vec<(f64, usize)> = bodies
        .iter()
        .enumerate()
        .map(|(i, body)| (body.position.distance_squared(camera_position), i))
        .collect();
    sorted_indices.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    app.targets.clear();
    app.targets.reserve(bodies.len());

    for (_, i) in sorted_indices {
        let body = &bodies[i];
        let pos = transform.transform_point3(body.position);

        if pos.z >= 1.0 {
            continue;
        }

        let distance_scalar = 1.0 - pos.z;

        // Transfer to screen coordinates
        let center_x = (pos.x / distance_scalar * screen_scalar + screen_width / 2.0) as f32;
        let center_y = (screen_height / 2.0 - pos.y / distance_scalar * screen_scalar) as f32;

        let radius_without_mag = body.radius / camera.distance / distance_scalar * screen_scalar;
        let radius_with_mag = radius_without_mag * body.magnification.cbrt();

        let max_magnification = 10.0;
        let mut radius = if radius_without_mag < max_magnification {
            radius_with_mag.min(max_magnification)
        } else {
            radius_without_mag
        };
        radius = radius.max(3.0);

        let left_x = center_x - radius as f32;
        let right_x = center_x + radius as f32;

        let top_y = center_y - radius as f32;
        let bottom_y = center_y + radius as f32;

        if right_x < rect.min.x || bottom_y < rect.min.y || left_x > rect.max.x || top_y > rect.max.y
        {
            continue;
        }

        let [r, g, b] = body.color;
        painter.add(Shape::circle_filled(
            Pos2::new(center_x, center_y),
            radius as f32,
            Color32::from_rgb(r, g, b),
        ));

        app.targets.push(TargetData {
            uuid: body.uuid,
            x: center_x,
            y: center_y,
            radius: radius as f32,
        });
    }

    painter.add(Shape::rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgb(100, 100, 255)),
        egui::StrokeKind::Inside,
    ));

    let font = FontId::monospace(13.0);
    let white = Color32::from_rgb(255, 255, 255);
    let line_height = 16.0f32;

    let use_target_speed = app.engine.params.lock().unwrap().use_target_speed;

    // Status lines in the bottom-left corner. "Bold" lines are drawn twice with a small x offset,
    // since the monospace font has no separate bold face.
    let num_lines = app.status_lines.len();
    let text_start_y = rect.max.y - num_lines as f32 * line_height - 5.0;

    for (i, text_str) in app.status_lines.iter().enumerate() {
        let mut pos = Pos2::new(rect.min.x + 5.0, text_start_y + i as f32 * line_height);

        if use_target_speed && i == 0 || !use_target_speed && i == 1 {
            let bold_pos = pos;
            pos.x += 0.5;
            painter.text(bold_pos, Align2::LEFT_TOP, text_str, font.clone(), white);
        }

        painter.text(pos, Align2::LEFT_TOP, text_str, font.clone(), white);
    }

    for (i, text_str) in app.object_description.iter().enumerate() {
        let pos = Pos2::new(rect.min.x + 5.0, rect.min.y + 5.0 + i as f32 * line_height);
        painter.text(pos, Align2::LEFT_TOP, text_str, font.clone(), white);
    }
}

pub fn get_status_text(app: &SolarApp) -> Vec<String> {
    let objects_len = app.engine.objects.lock().unwrap().len();
    let params = app.engine.params.lock().unwrap().clone();

    let method = match params.method {
        IntegrationMethod::Symplectic(k) => {
            format!(
                "Symplectic - {} order",
                match k {
                    1 => "1st",
                    2 => "2nd",
                    3 => "3rd",
                    4 => "4th",
                    _ => "??",
                }
            )
        }
        IntegrationMethod::RK4 => "Runge-Kutta 4".into(),
    };

    let lines = vec![
        format!(
            "Target speed: {:.1} d/s {}",
            params.target_speed / 86400.0,
            if params.target_speed >= 86400.0 * 365.0 {
                format!("({:.2} y/s)", params.target_speed / 86400.0 / 365.0)
            } else if params.target_speed <= 3600.0 {
                format!("({:.2} m/s)", params.target_speed / 60.0)
            } else if params.target_speed <= 3600.0 * 24.0 {
                format!("({:.2} h/s)", params.target_speed / 3600.0)
            } else {
                "".into()
            }
        ),
        format!(
            "Timestep: {:.3} s {}",
            params.time_step,
            if params.time_step >= 86400.0 {
                format!("({:.2} d)", params.time_step / 86400.0)
            } else if params.time_step >= 3600.0 {
                format!("({:.2} h)", params.time_step / 3600.0)
            } else if params.time_step >= 60.0 {
                format!("({:.2} m)", params.time_step / 60.0)
            } else {
                "".into()
            }
        ),
        format!(
            "Simulation time: {:.2} y",
            params.time_elapsed / (60.0 * 60.0 * 24.0 * 365.0)
        ),
        format!("Objects: {}", objects_len),
        format!("Method: {}", method),
        format!("Threads: {}", params.num_threads),
        format!("Speed: {:.0} n/s", params.iteration_speed),
    ];

    lines
}

pub fn get_object_description_text(app: &SolarApp) -> Vec<String> {
    let objects = app.engine.objects.lock().unwrap();
    let obj = if let Some(target) = app.current_target {
        match objects.iter().find(|x| x.uuid == target) {
            Some(object) => object,
            None => return vec![],
        }
    } else {
        return vec![];
    };

    let mut parent_info: Vec<String> = vec!["".into(), "".into(), "".into()];
    if let Some(parent) = engine::Engine::find_orbital_parent(obj, &objects) {
        parent_info = vec![
            format!(
                " - {:.4e} m/s compared to {}",
                (obj.velocity - parent.velocity).length(),
                parent.name
            ),
            format!(
                " - {:.4e} m/s^2 compared to {}",
                (obj.acceleration - parent.acceleration).length(),
                parent.name
            ),
            format!(
                " - {:.4e} J compared to {}",
                0.5 * (obj.velocity - parent.velocity).length_squared() * obj.mass,
                parent.name
            ),
        ]
    }

    vec![
        format!("Name: {}", obj.name),
        format!("Mass: {:.4e} kg", obj.mass),
        format!("Radius: {:.4e} m", obj.radius),
        format!("Speed: {:.4e} m/s{}", obj.velocity.length(), parent_info[0]),
        format!(
            "Acceleration magnitude: {:.4e} m/s^2{}",
            obj.acceleration.length(),
            parent_info[1]
        ),
        format!(
            "Kinetic energy: {:.4e} J{}",
            0.5 * obj.mass * obj.velocity.length_squared(),
            parent_info[2]
        ),
        "".into(),
        format!(
            "Position: [{:.4e}, {:.4e}, {:.4e}]",
            obj.position.x, obj.position.y, obj.position.z
        ),
        format!(
            "Velocity: [{:.4e}, {:.4e}, {:.4e}]",
            obj.velocity.x, obj.velocity.y, obj.velocity.z
        ),
        format!(
            "Acceleration: [{:.4e}, {:.4e}, {:.4e}]",
            obj.acceleration.x, obj.acceleration.y, obj.acceleration.z
        ),
    ]
}
