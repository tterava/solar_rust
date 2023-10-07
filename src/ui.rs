use std::time::{Duration, Instant};

use glam::DVec3;
use uuid::Uuid;
use winapi::{
    shared::windef::HBRUSH,
    um::wingdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, Ellipse,
        SelectObject, SetBkMode, SetTextColor, TextOutW, RGB, SRCCOPY, TRANSPARENT,
    },
};

use crate::{engine, integration::IntegrationMethod, DrawingApp};
pub struct TargetData {
    pub uuid: Uuid,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

pub fn paint(app: &DrawingApp, data: &nwg::EventData) {
    let paint_objects = get_paint_objects(app);
    let now = Instant::now();

    let mut status_lines = app.status_lines.borrow_mut();
    let mut object_description = app.object_description.borrow_mut();

    if *app.next_status_update.borrow() <= now {
        *status_lines = get_status_text(app);
        *object_description = get_object_description_text(app);
        *app.next_status_update.borrow_mut() = Instant::now() + Duration::from_millis(500);
    }

    use winapi::um::winuser::{FillRect, FrameRect};

    let paint = data.on_paint();
    let ps = paint.begin_paint();

    unsafe {
        let p = app.paint_data.borrow();
        let hdc = ps.hdc;
        let rc = &ps.rcPaint;

        // Use mem_dc to avoid flickering
        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bitmap = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
        let prev_bitmap = SelectObject(mem_dc, mem_bitmap as _);

        FillRect(mem_dc, rc, p.background as _);
        SelectObject(mem_dc, p.pen as _);

        for (left_x, right_x, top_y, bottom_y, brush) in paint_objects.iter() {
            SelectObject(mem_dc, *brush as _);
            Ellipse(mem_dc, *left_x, *top_y, *right_x, *bottom_y);
        }

        FrameRect(mem_dc, rc, p.border as _);

        SetTextColor(mem_dc, RGB(255, 255, 255));
        SetBkMode(mem_dc, TRANSPARENT as i32);
        let size = app.canvas.size();
        let line_height = 18;
        let text_start_y = size.1 as i32 - status_lines.len() as i32 * line_height - 5;

        let params = app.engine.params.lock().unwrap();
        for (i, text_str) in status_lines.iter().enumerate() {
            let text = text_str.encode_utf16().collect::<Vec<u16>>();
            if params.use_target_speed && i == 0 || !params.use_target_speed && i == 1 {
                SelectObject(mem_dc, p.font_bold as _);
            } else {
                SelectObject(mem_dc, p.font as _);
            }
            TextOutW(
                mem_dc,
                5,
                text_start_y + i as i32 * 18,
                text.as_ptr(),
                text.len() as i32,
            );
        }

        SelectObject(mem_dc, p.font as _);
        for (i, text_str) in object_description.iter().enumerate() {
            let text = text_str.encode_utf16().collect::<Vec<u16>>();
            TextOutW(
                mem_dc,
                5,
                5 + i as i32 * 18,
                text.as_ptr(),
                text.len() as i32,
            );
        }

        BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);

        SelectObject(mem_dc, prev_bitmap);
        DeleteObject(mem_bitmap as _);
        DeleteDC(mem_dc);
    }

    paint.end_paint(&ps);
}

pub fn get_status_text(app: &DrawingApp) -> Vec<String> {
    let objects_len;
    {
        objects_len = app.engine.objects.lock().unwrap().len(); // get len early to avoid holding two locks at once
    }

    let params = app.engine.params.lock().unwrap();

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
                format!("({:.2} min/s)", params.target_speed / 60.0)
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
                format!("({:.2} d/s)", params.time_step / 86400.0)
            } else if params.time_step >= 3600.0 {
                format!("({:.2} h/s)", params.time_step / 3600.0)
            } else if params.time_step >= 60.0 {
                format!("({:.2} min/s)", params.time_step / 60.0)
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

pub fn get_object_description_text(app: &DrawingApp) -> Vec<String> {
    let obj;
    let objects = app.engine.objects.lock().unwrap();
    if let Some(target) = *app.current_target.borrow() {
        if let Some(object) = objects.iter().find(|x| x.uuid == target) {
            obj = object;
        } else {
            return vec![];
        }
    } else {
        return vec![];
    }

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

pub fn get_paint_objects(app: &DrawingApp) -> Vec<(i32, i32, i32, i32, HBRUSH)> {
    let bodies = app.engine.objects.lock().unwrap().clone();
    let mut camera = app.camera.lock().unwrap();
    let mut target_opt = app.current_target.borrow_mut();

    let (screen_width_pix, screen_height_pix) = app.window.size();
    let screen_scalar = screen_width_pix as f64 / 2.0 / (camera.fov / 2.0).to_radians().tan();

    if let Some(target) = *target_opt {
        camera.target = match bodies.iter().find(|x| x.uuid == target) {
            Some(b) => match camera.get_animation_position(b.position, b.radius) {
                Some((target, distance)) => {
                    camera.distance = distance;

                    if target == b.position {
                        camera.animation_start = None;
                    }

                    target
                }
                None => b.position,
            },
            None => {
                *target_opt = None;
                DVec3::ZERO
            }
        }
    }

    let transform = camera.get_full_transformation();

    let mut output: Vec<(i32, i32, i32, i32, HBRUSH)> = Vec::new();

    let mut sorted_indices: Vec<usize> = (0..bodies.len()).collect();
    sorted_indices.sort_by(|a, b| bodies[*b].cmp(&bodies[*a], camera.get_position()));

    let mut targets = app.targets.borrow_mut();
    targets.clear();

    for i in sorted_indices {
        let body = &bodies[i];
        let pos = transform.transform_point3(body.position);

        if pos.z >= 1.0 {
            continue;
        }

        let distance_scalar = 1.0 - pos.z;

        // Transfer to screen coordinates
        let center_x = pos.x / distance_scalar * screen_scalar + screen_width_pix as f64 / 2.0;
        let center_y = screen_height_pix as f64 / 2.0 - pos.y / distance_scalar * screen_scalar;

        let radius_without_mag = body.radius / camera.distance / distance_scalar * screen_scalar;
        let radius_with_mag = radius_without_mag * body.magnification.powf(3.0_f64.recip());

        let max_magnification = 10.0;
        let mut radius = if radius_without_mag < max_magnification {
            radius_with_mag.min(max_magnification)
        } else {
            radius_without_mag
        };
        radius = radius.max(3.0);

        let left_x = center_x - radius;
        let right_x = center_x + radius;

        let top_y = center_y - radius;
        let bottom_y = center_y + radius;

        let res_left_x: i32 = left_x.round() as i32;
        let res_right_x: i32 = right_x.round() as i32;
        let res_top_y: i32 = top_y.round() as i32;
        let res_bottom_y: i32 = bottom_y.round() as i32;

        if res_right_x < 0
            || res_bottom_y < 0
            || res_left_x > screen_width_pix as i32
            || res_top_y > screen_height_pix as i32
        {
            continue;
        }

        let [r, g, b] = body.color;
        output.push((
            res_left_x,
            res_right_x,
            res_top_y,
            res_bottom_y,
            app.get_brush(r, g, b),
        ));
        targets.push(TargetData {
            uuid: body.uuid,
            x: center_x,
            y: center_y,
            radius,
        });
    }

    output
}
