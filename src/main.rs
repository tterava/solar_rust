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

use crate::engine::Engine;
use crate::matrix::Matrix4d;
use crate::vector::Vector4d;
use crate::camera::Camera;

extern crate native_windows_gui as nwg;
extern crate native_windows_derive as nwd;

use astronomy::AstronomicalObject;
use input::input_listener;
use nwd::NwgUi;
use nwg::NativeUi;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{mem, thread, time};
use std::cell::{RefCell, Cell};
use std::sync::{Mutex, Arc};
use winapi::shared::windef::{HBRUSH, HPEN};
use winapi::um::wingdi::{CreateSolidBrush, CreatePen, Ellipse, Polygon, SelectObject, RGB, PS_SOLID};

const FRAMERATE: u64 = 170;

pub struct PaintData {
    background: HBRUSH,
    border: HBRUSH,
    pen: HPEN
}

#[derive(Debug)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    brush: HBRUSH
}

impl Default for PaintData {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

#[derive(Default, NwgUi)]
pub struct DrawingApp {
    #[nwg_control(size: (960, 540), position: (200, 200), title: "Solar system simulator", flags: "WINDOW|VISIBLE|RESIZABLE")]
    #[nwg_events( OnWindowClose: [nwg::stop_thread_dispatch()], OnInit: [DrawingApp::setup], OnResize: [DrawingApp::update_size], OnWindowMaximize: [DrawingApp::update_size])]
    window: nwg::Window,

    // By default ExternCanvas is a window so we must specify the parent here
    #[nwg_control(parent: Some(&data.window), position: (0,0), size: (960, 540))]
    #[nwg_events( 
        OnPaint: [DrawingApp::paint(SELF, EVT_DATA)],
        OnMousePress: [DrawingApp::events(SELF, EVT, EVT_DATA)],
        OnMouseWheel: [DrawingApp::events(SELF, EVT, EVT_DATA)]
    )]
    canvas: nwg::ExternCanvas,

    paint_data: RefCell<PaintData>,

    clicked: Arc<AtomicBool>,
    bodies: Arc<Mutex<Vec<AstronomicalObject>>>,
    colors: Arc<Mutex<Vec<Color>>>,
    camera: Arc<Mutex<Camera>>,

    #[nwg_control(parent: window, interval: Duration::from_millis(1000 / FRAMERATE))]
    #[nwg_events( OnTimerTick: [DrawingApp::inv] )]
    animation_timer: nwg::AnimationTimer,
}

impl DrawingApp {
    fn setup(&self) {
        let mut data = self.paint_data.borrow_mut();

        unsafe {
            data.background = CreateSolidBrush(RGB(0, 0, 0));
            data.border = CreateSolidBrush(RGB(100, 100, 255));
            data.pen = CreatePen(PS_SOLID as _, 2, RGB(20, 20, 20));
        }
    }

    fn inv(&self) {
        self.canvas.invalidate();
    }

    fn events(&self, evt: nwg::Event, evt_data: &nwg::EventData) {
        use nwg::Event as E;
        use nwg::MousePressEvent as M;
        use nwg::EventData::OnMouseWheel as MW;

        match evt {
            E::OnMousePress(M::MousePressLeftUp) => { self.clicked.store(false, Ordering::Relaxed) },
            E::OnMousePress(M::MousePressLeftDown) => { 
                self.clicked.store(true, Ordering::Relaxed);
                input::input_listener(self.clicked.clone(), self.camera.clone());
            },
            E::OnMouseWheel => {
                if let MW(amount) = evt_data {
                    self.zoom(*amount);
                }
            },
            _ => { },
        }

        self.canvas.invalidate();
    }

    fn paint(&self, data: &nwg::EventData) {
        let paint_objects = self.get_paint_objects();        
        let (_, screen_height) = self.window.size();

        use winapi::um::winuser::{FillRect, FrameRect};
        
        let paint = data.on_paint();
        let ps = paint.begin_paint();
 
        unsafe {
            let p = self.paint_data.borrow();
            let hdc = ps.hdc;
            let rc = &ps.rcPaint;

            FillRect(hdc, rc, p.background as _);

            SelectObject(hdc, p.pen as _);

            for (x, y, r, brush) in paint_objects.iter() {
                let left: i32 = match x.checked_sub(*r) {
                    Some(x) => x,
                    None => continue
                };

                let right: i32 = match x.checked_add(*r) {
                    Some(x) => x,
                    None => continue
                };

                let top: i32 = match (screen_height as i64 - (*y as i64 + *r as i64)).try_into() {
                    Ok(x) => x,
                    Err(_) => continue
                };

                let bottom: i32 = match (screen_height as i64 - (*y as i64 - *r as i64)).try_into() {
                    Ok(x) => x,
                    Err(_) => continue
                };

                SelectObject(hdc, *brush as _);
                Ellipse(hdc, left, top, right, bottom);
            }

            FrameRect(hdc, rc, p.border as _);

            
            // SelectObject(hdc, p.pen as _);
            // SelectObject(hdc, p.yellow as _);
            // Ellipse(hdc, rc.left + 20, rc.top + 20, rc.right - 20, rc.bottom - 20);

            // SelectObject(hdc, p.white as _);
            // Ellipse(hdc, 60, 60, 130, 130);
            // Ellipse(hdc, 150, 60, 220, 130);

            // if self.clicked.get() {
            //     SelectObject(hdc, p.red as _);
            // } else {
            //     SelectObject(hdc, p.black as _);
            // }
            
            // Ellipse(hdc, 80, 80, 110, 110);
            // Ellipse(hdc, 170, 80, 200, 110);

            // SelectObject(hdc, p.red as _);
            // let pts = &[P{x: 60, y: 150}, P{x: 220, y: 150}, P{x: 140, y: 220}];
            // Polygon(hdc, pts.as_ptr(), pts.len() as _);

            // let (x, y) = self.get_coords_from_timer();
            // Ellipse(hdc, x - 10 + 140, y - 10 + 140, x + 10 + 140, y + 10 + 140);
        }

        paint.end_paint(&ps);
    }

    fn get_paint_objects(&self) -> Vec<(i32, i32, i32, HBRUSH)> {
        let bodies = self.bodies.lock().unwrap();
        let camera = self.camera.lock().unwrap();

        let (screen_width_pix, screen_height_pix) = self.window.size();
        let w_128 = screen_width_pix as i128;
        let h_128 = screen_height_pix as i128;

        let screen_scalar = screen_width_pix as f64 / 2.0 / (camera.fov / 2.0).to_radians().tan();

        let transform = camera.get_full_transformation();

        let mut output: Vec<(i32, i32, i32, HBRUSH)> = Vec::new();

        let camera_distances: Vec<_> = bodies.iter().map(|x| (x.name.clone(), x.position.distance(&camera.pos))).collect();

        let mut sorted_indices: Vec<usize> = (0..bodies.len()).collect();
        sorted_indices.sort_by(|a, b| bodies[*b].cmp(&bodies[*a], &camera.pos));

        for i in sorted_indices {
            let body = &bodies[i];
            let pos = transform.multiply_vec(&body.position);

            if pos.data[2] >= 1.0 {
                continue;
            }

            let distance_scalar = 1.0 - pos.data[2];
            
            let center_x = pos.data[0] / distance_scalar * screen_scalar;
            let center_y = pos.data[1] / distance_scalar * screen_scalar;
            let radius = body.radius * body.magnification.powf(1.0 / 3.0) / camera.scale() / distance_scalar * screen_scalar;

            let res_x: i32 = match (center_x.round() as i128 + w_128 / 2).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let res_y: i32 = match (center_y.round() as i128 + h_128 / 2).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let res_radius: i32 = match (radius.round() as u128).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let [r, g, b] = body.color;
            output.push(
                (
                    res_x,
                    res_y,
                    res_radius,
                    self.get_brush(r, g, b)
                )
            );
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
        let mut colors = self.colors.lock().unwrap();    
        for c in colors.iter_mut() {
            if r == c.r && g == c.g && b == c.b {
                return c.brush;
            }
        }
        
        unsafe {
            let brush = CreateSolidBrush(RGB(r, g, b));
            let new_color = Color {r, g, b, brush};
            println!("Adding brush: {:?}", new_color);
            colors.push(new_color);
            brush
        }        
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let mut _app = DrawingApp::build_ui(Default::default()).expect("Failed to build UI");
    let engine = Engine::init(FRAMERATE);

    let (engine_handle, kill_request) = engine.start_multithread(_app.bodies.clone());

    _app.animation_timer.start();        
    nwg::dispatch_thread_events();

    kill_request.store(true, Ordering::Relaxed);

    // Make sure extra threads end cleanly
    engine_handle.join().unwrap();
}
