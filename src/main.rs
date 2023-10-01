/*!
    Small custom control example using GDI. NWG has no drawing API, so raw winapi must be used.

    Requires the following features: `cargo run --example basic_drawing_d --features "extern-canvas"`
*/
mod matrix;
mod vector;
mod camera;
mod engine;
mod astronomy;
mod input;
mod integration;
mod events;

use crate::engine::Engine;
use crate::vector::Vector4d;
use crate::camera::Camera;
use crate::integration::IntegrationMethod;

extern crate native_windows_gui as nwg;
extern crate native_windows_derive as nwd;

use astronomy::AstronomicalObject;
use nwd::NwgUi;
use nwg::{NativeUi, ExternCanvas, Window};
use uuid::Uuid;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use std::mem;
use std::cell::RefCell;
use std::sync::{Mutex, Arc};
use winapi::shared::windef::{HBRUSH, HPEN, HFONT};
use winapi::um::wingdi::{CreateSolidBrush, CreatePen, Ellipse, SelectObject, RGB, PS_SOLID, CreateFontW, FW_NORMAL, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, DEFAULT_QUALITY, DEFAULT_PITCH, FF_DONTCARE, TextOutW, SetBkMode, TRANSPARENT, SetTextColor, FW_BOLD, CreateCompatibleDC, CreateCompatibleBitmap, BitBlt, SRCCOPY, DeleteObject, DeleteDC};

const FRAMERATE: u32 = 170;

pub struct PaintData {
    background: HBRUSH,
    border: HBRUSH,
    pen: HPEN,
    font: HFONT,
    font_bold: HFONT
}

#[derive(Debug)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    brush: HBRUSH
}

struct TargetData {
    uuid: Uuid,
    x: f64,
    y: f64,
    radius: f64
}

impl Default for PaintData {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

#[derive(NwgUi)]
pub struct DrawingApp {
    #[nwg_control(size: (960, 540), position: (200, 200), title: "Solar system simulator", flags: "MAIN_WINDOW|VISIBLE")]
    #[nwg_events( 
        OnWindowClose: [nwg::stop_thread_dispatch()], 
        OnInit: [DrawingApp::setup], 
        OnResize: [DrawingApp::update_size], 
        OnWindowMaximize: [DrawingApp::update_size],
        OnKeyPress: [DrawingApp::events(SELF, EVT, EVT_DATA)]
    )]
    window: nwg::Window,

    // By default ExternCanvas is a window so we must specify the parent here
    #[nwg_control(parent: Some(&data.window), position: (0,0), size: (960, 540))]
    #[nwg_events( 
        OnPaint: [DrawingApp::paint(SELF, EVT_DATA)],
        OnMousePress: [DrawingApp::events(SELF, EVT, EVT_DATA)],
        OnMouseWheel: [DrawingApp::events(SELF, EVT, EVT_DATA)],
    )]
    canvas: nwg::ExternCanvas,
    paint_data: RefCell<PaintData>,

    engine: Engine,

    camera: Arc<Mutex<Camera>>,
    is_dragging: Arc<AtomicBool>,
    current_target: RefCell<Option<Uuid>>,
    targets: RefCell<Vec<TargetData>>,
    colors: RefCell<Vec<Color>>,

    #[nwg_control(parent: window, interval: Duration::from_micros(1_000_000 / FRAMERATE as u64))]
    #[nwg_events( OnTimerTick: [DrawingApp::inv] )]
    animation_timer: nwg::AnimationTimer,
    next_status_update: RefCell<Instant>,
    status_lines: RefCell<Vec<String>>,
    object_description: RefCell<Vec<String>>
}

impl DrawingApp {
    fn setup(&self) {
        let mut data = self.paint_data.borrow_mut();

        unsafe {
            data.background = CreateSolidBrush(RGB(0, 0, 0));
            data.border = CreateSolidBrush(RGB(100, 100, 255));
            data.pen = CreatePen(PS_SOLID as _, 1, RGB(35, 35, 35));
            data.font = CreateFontW(
                18,
                0, 
                0, 
                0, 
                FW_NORMAL, 
                0, 
                0, 
                0, 
                DEFAULT_CHARSET, 
                OUT_DEFAULT_PRECIS, 
                CLIP_DEFAULT_PRECIS, 
                DEFAULT_QUALITY, 
                DEFAULT_PITCH | FF_DONTCARE, 
                "Courier New\0".encode_utf16().collect::<Vec<u16>>().as_ptr()
            );
            data.font_bold = CreateFontW(
                18,
                0, 
                0, 
                0, 
                FW_BOLD, 
                0, 
                1, 
                0, 
                DEFAULT_CHARSET, 
                OUT_DEFAULT_PRECIS, 
                CLIP_DEFAULT_PRECIS, 
                DEFAULT_QUALITY, 
                DEFAULT_PITCH | FF_DONTCARE, 
                "Courier New\0".encode_utf16().collect::<Vec<u16>>().as_ptr()
            );
        }
    }

    fn inv(&self) {
        self.canvas.invalidate();
    }

    fn events(&self, evt: nwg::Event, evt_data: &nwg::EventData) {
        events::handle_event(self, evt, evt_data);
    }

    fn paint(&self, data: &nwg::EventData) {
        let paint_objects = self.get_paint_objects();
        let now = Instant::now();

        let mut status_lines = self.status_lines.borrow_mut();
        let mut object_description = self.object_description.borrow_mut();

        if *self.next_status_update.borrow() <= now {
            *status_lines = self.get_status_text();
            *object_description = self.get_object_description_text();
            *self.next_status_update.borrow_mut() = Instant::now() + Duration::from_millis(500);
        }

        use winapi::um::winuser::{FillRect, FrameRect};
        
        let paint = data.on_paint();
        let ps = paint.begin_paint();

        unsafe {
            let p = self.paint_data.borrow();
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
            let size = self.canvas.size();
            let line_height = 18;
            let text_start_y = size.1 as i32 - status_lines.len() as i32 * line_height - 5;

            let params = self.engine.params.lock().unwrap();
            for (i, text_str) in status_lines.iter().enumerate() {
                let text = text_str.encode_utf16().collect::<Vec<u16>>();
                if params.use_target_speed && i == 0 || !params.use_target_speed && i == 1 {
                    SelectObject(mem_dc, p.font_bold as _);
                } else {
                    SelectObject(mem_dc, p.font as _);
                }
                TextOutW(mem_dc, 5, text_start_y + i as i32 * 18 , text.as_ptr(), text.len() as i32);
            }

            SelectObject(mem_dc, p.font as _);
            for (i, text_str) in object_description.iter().enumerate() {
                let text = text_str.encode_utf16().collect::<Vec<u16>>();
                TextOutW(mem_dc, 5, 5 + i as i32 * 18 , text.as_ptr(), text.len() as i32);
            }

            BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, prev_bitmap);
            DeleteObject(mem_bitmap as _);
            DeleteDC(mem_dc);
        }

        paint.end_paint(&ps);
    }

    fn get_status_text(&self) -> Vec<String> {
        let objects_len;
        {
            objects_len = self.engine.objects.lock().unwrap().len();  // get len early to avoid holding two locks at once
        }

        let params = self.engine.params.lock().unwrap();

        let method = match params.method {
            IntegrationMethod::Symplectic(k) => {
                format!("Symplectic - {} order", match k {
                    1 => "1st",
                    2 => "2nd",
                    3 => "3rd",
                    4 => "4th",
                    _ => "??"
                })
            },
            IntegrationMethod::RK4 => "Runge-Kutta 4".into()
        };

        let lines = vec![
            format!("Target speed: {:.1} d/s {}", 
            params.target_speed / 86400.0,
                if params.target_speed >= 86400.0 * 365.0 {
                    format!("({:.2} y/s)", params.target_speed / 86400.0 / 365.0) 
                } else if params.target_speed <= 3600.0 {
                    format!("({:.2} min/s)", params.target_speed / 60.0) 
                }
                else if params.target_speed <= 3600.0 * 24.0 {
                    format!("({:.2} h/s)", params.target_speed / 3600.0) 
                } else { "". into() }
            ),
            format!("Time step: {:.3} s {}", 
                params.time_step,
                if params.time_step >= 86400.0 {
                    format!("({:.2} d/s)", params.time_step / 86400.0)
                } 
                else if params.time_step >= 3600.0 {
                    format!("({:.2} h/s)", params.time_step / 3600.0)
                }
                else if params.time_step >= 60.0 {
                    format!("({:.2} min/s)", params.time_step / 60.0)
                } else { "".into() }
            ),
            format!("Objects: {}", objects_len),
            format!("Method: {}", method),
            format!("Threads: {}", params.num_threads),
            format!("Speed: {:.0} n/s", params.iteration_speed)
        ];

        lines
    }

    fn get_object_description_text(&self) -> Vec<String> {
        let obj;
        let objects = self.engine.objects.lock().unwrap();
        if let Some(target) = *self.current_target.borrow() {   
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
                format!(" - {:.4e} m/s compared to {}", obj.velocity.substract(&parent.velocity).length(), parent.name),
                format!(" - {:.4e} m/s^2 compared to {}", obj.acceleration.substract(&parent.acceleration).length(), parent.name),
                format!(" - {:.4e} J compared to {}", 0.5 * obj.velocity.substract(&parent.velocity).length().powi(2) * obj.mass, parent.name),
            ]
        }

        vec![
            format!("Name: {}", obj.name),
            format!("Mass: {:.4e} kg", obj.mass),
            format!("Radius: {:.4e} m", obj.radius),
            format!("Speed: {:.4e} m/s{}", obj.velocity.length(), parent_info[0]),
            format!("Acceleration magnitude: {:.4e} m/s^2{}", obj.acceleration.length(), parent_info[1]),
            format!("Kinetic energy: {:.4e} J{}", 0.5 * obj.mass * obj.velocity.length().powi(2), parent_info[2]),
            "".into(),
            format!("Position: [{:.4e}, {:.4e}, {:.4e}]", obj.position.data[0], obj.position.data[1], obj.position.data[2]),
            format!("Velocity: [{:.4e}, {:.4e}, {:.4e}]", obj.velocity.data[0], obj.velocity.data[1], obj.velocity.data[2]),
            format!("Acceleration: [{:.4e}, {:.4e}, {:.4e}]", obj.acceleration.data[0], obj.acceleration.data[1], obj.acceleration.data[2]),
        ]
    }

    fn get_paint_objects(&self) -> Vec<(i32, i32, i32, i32, HBRUSH)> {
        let bodies = self.engine.objects.lock().unwrap().clone();
        let mut camera = self.camera.lock().unwrap();
        let mut target_opt = self.current_target.borrow_mut();

        let (screen_width_pix, screen_height_pix) = self.window.size();
        let screen_scalar = screen_width_pix as f64 / 2.0 / (camera.fov / 2.0).to_radians().tan();

        if let Some(target) = *target_opt {
            camera.target = match bodies.iter().find(|x| x.uuid == target) {
                Some(b) => b.position.to_4d(),
                None => {
                    *target_opt = None;
                    Vector4d::default()
                }
            }
        }

        let transform = camera.get_full_transformation();

        let mut output: Vec<(i32, i32, i32, i32, HBRUSH)> = Vec::new();

        let mut sorted_indices: Vec<usize> = (0..bodies.len()).collect();
        sorted_indices.sort_by(|a, b| bodies[*b].cmp(&bodies[*a], &camera.get_position().to_3d()));

        let mut targets = self.targets.borrow_mut();
        targets.clear();

        for i in sorted_indices {
            let body = &bodies[i];
            let pos = transform.multiply_vec(&body.position.to_4d());

            if pos.data[2] >= 1.0 {
                continue;
            }

            let distance_scalar = 1.0 - pos.data[2];
            
            // Transfer to screen coordinates
            let center_x = pos.data[0] / distance_scalar * screen_scalar + screen_width_pix as f64 / 2.0;
            let center_y = screen_height_pix as f64 / 2.0 - pos.data[1] / distance_scalar * screen_scalar;

            let radius_without_mag = body.radius / camera.distance / distance_scalar * screen_scalar;
            let radius_with_mag = radius_without_mag * body.magnification.powf(1.0 / 3.0);

            let max_magnification = 14.0;
            let mut radius = if radius_without_mag < max_magnification { radius_with_mag.min(max_magnification) } else { radius_without_mag };
            radius = radius.max(3.0);

            let left_x = center_x - radius;
            let right_x = center_x + radius;

            let top_y = center_y - radius;
            let bottom_y = center_y + radius;

            let res_left_x: i32 = left_x.round() as i32;
            let res_right_x: i32 = right_x.round() as i32;
            let res_top_y: i32 = top_y.round() as i32;
            let res_bottom_y: i32 = bottom_y.round() as i32;

            if res_right_x < 0 || res_bottom_y < 0 || res_left_x > screen_width_pix as i32 || res_top_y > screen_height_pix as i32 {
                continue;
            }

            let [r, g, b] = body.color;
            output.push(
                (
                    res_left_x,
                    res_right_x,
                    res_top_y,
                    res_bottom_y,
                    self.get_brush(r, g, b)
                )
            );
            targets.push(TargetData { uuid: body.uuid, x: center_x, y: center_y, radius });
        }

        output

    }

    fn update_size(&self) {
        let (x, y) = self.window.size();
        self.canvas.set_size(x, y);
    }

    fn zoom(&self, amount: i32) {
        let mut camera = self.camera.lock().unwrap();
        camera.zoom(amount);
    }

    fn get_brush(&self, r: u8, g: u8, b: u8) -> HBRUSH {    
        let mut colors = self.colors.borrow_mut();    
        for c in colors.iter_mut() {
            if r == c.r && g == c.g && b == c.b {
                return c.brush;
            }
        }
        
        unsafe {
            let brush = CreateSolidBrush(RGB(r, g, b));
            
            let new_color = Color {r, g, b, brush};
            colors.push(new_color);
            brush
        }        
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let app = DrawingApp {
        animation_timer: nwg::AnimationTimer::default(),
        camera: Arc::new(Mutex::new(Camera::default())),
        window: Window::default(),
        canvas: ExternCanvas::default(),
        paint_data: RefCell::new(PaintData::default()),
        engine: Engine::default(),
        is_dragging: Arc::new(AtomicBool::new(false)),
        current_target: RefCell::new(None),
        targets: RefCell::new(Vec::new()),
        colors: RefCell::new(Vec::new()),
        next_status_update: RefCell::new(Instant::now()),
        status_lines: RefCell::new(Vec::new()),
        object_description: RefCell::new(Vec::new())
    };

    // let mut _app = DrawingApp::build_ui(Default::default()).expect("Failed to build UI");
    let app_ui = DrawingApp::build_ui(app).expect("Failed to build UI");

    *app_ui.engine.framerate.lock().unwrap() = FRAMERATE;
    app_ui.engine.params.lock().unwrap().target_speed = 86400.0 * 1.0;

    // for _ in 0..2000 {
    //     let mut objects = app_ui.engine.objects.lock().unwrap();
    //     let orbital = AstronomicalObject::get_random_planet();
    //     let object = AstronomicalObject::place_on_orbit(orbital, &objects[0]);

    //     objects.push(object);
    // }
    
    app_ui.animation_timer.start();        
    nwg::dispatch_thread_events();

    // Make sure extra threads end cleanly
    app_ui.engine.stop();
}
