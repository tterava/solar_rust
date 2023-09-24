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

use crate::engine::Engine;
use crate::vector::Vector4d;
use crate::camera::Camera;

extern crate native_windows_gui as nwg;
extern crate native_windows_derive as nwd;

use nwd::NwgUi;
use nwg::NativeUi;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{mem, thread};
use std::cell::RefCell;
use std::sync::{Mutex, Arc};
use winapi::shared::windef::{HBRUSH, HPEN};
use winapi::um::wingdi::{CreateSolidBrush, CreatePen, Ellipse, SelectObject, RGB, PS_SOLID};

const FRAMERATE: u32 = 170;

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

struct TargetData {
    name: String,
    x: i32,
    y: i32,
    radius: i32
}

impl Default for PaintData {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

#[derive(Default, NwgUi)]
pub struct DrawingApp {
    #[nwg_control(size: (960, 540), position: (200, 200), title: "Solar system simulator", flags: "WINDOW|VISIBLE|RESIZABLE")]
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
    current_target: Arc<Mutex<String>>,
    targets: Arc<Mutex<Vec<TargetData>>>,
    
    colors: Arc<Mutex<Vec<Color>>>,

    #[nwg_control(parent: window, interval: Duration::from_micros(1_000_000 / FRAMERATE as u64))]
    #[nwg_events( OnTimerTick: [DrawingApp::inv] )]
    animation_timer: nwg::AnimationTimer,
}

impl DrawingApp {
    fn setup(&self) {
        let mut data = self.paint_data.borrow_mut();

        unsafe {
            data.background = CreateSolidBrush(RGB(0, 0, 0));
            data.border = CreateSolidBrush(RGB(100, 100, 255));
            data.pen = CreatePen(PS_SOLID as _, 1, RGB(35, 35, 35));
        }
    }

    fn inv(&self) {
        self.canvas.invalidate();
    }

    fn events(&self, evt: nwg::Event, evt_data: &nwg::EventData) {
        use nwg::Event as E;
        use nwg::MousePressEvent as M;
        use nwg::EventData::OnMouseWheel as MW;
        use nwg::EventData::OnKey as K;

        match evt {
            E::OnMousePress(M::MousePressRightUp) => { self.is_dragging.store(false, Ordering::Relaxed) },
            E::OnMousePress(M::MousePressRightDown) => { 
                self.is_dragging.store(true, Ordering::Relaxed);
                input::input_listener(self.is_dragging.clone(), self.camera.clone());
            },
            E::OnMousePress(M::MousePressLeftDown) => {
                let (w_x, w_y) = self.window.position();               
                let (m_x, m_y) = winput::Mouse::position().unwrap();
                let (_, size_y) = self.canvas.size();

                let (x, y) = (m_x - w_x - 8, size_y as i32 - (m_y - w_y - 31));  // Offset x: 8, y: 31 works for Windows 11. TODO: Figure a better way for this
                let targets = self.targets.lock().unwrap();

                println!("{}, {}", m_x - w_x, m_y - w_y);
                
                for target in targets.iter().rev() {
                    let tx = target.x as i64;
                    let ty = target.y as i64;
                    let r = target.radius as i64;

                    if (tx - x as i64).pow(2) + (ty - y as i64).pow(2) <= r.pow(2) {
                        *self.current_target.lock().unwrap() = target.name.clone();
                        break;
                    }
                }
            }
            E::OnMouseWheel => {
                if let MW(amount) = evt_data {
                    self.zoom(*amount);
                }
            },
            E::OnKeyPress => {
                if let K(key) = evt_data {
                    match key {
                        107 => { *self.engine.target_speed.lock().unwrap() *= 1.2 },
                        109 => { *self.engine.target_speed.lock().unwrap() /= 1.2 },
                        32 => { 
                            if *self.engine.is_running.lock().unwrap() {
                                self.engine.stop();
                            } else {
                                // self.engine.start(engine::IntegrationMethod::ImplicitEuler);
                                self.engine.start_mt(engine::IntegrationMethod::ImplicitEuler, 1);
                            }
                        },
                        49..=57 => {
                            let threads = key - 48;
                            if *self.engine.is_running.lock().unwrap() {
                                self.engine.stop();
                                thread::sleep(Duration::from_millis(200));
                            } 
                                // self.engine.start(engine::IntegrationMethod::ImplicitEuler);
                            self.engine.start_mt(engine::IntegrationMethod::ImplicitEuler, threads as usize);
                        }
                        key => { println!("Key: {}", key) }
                    }
                }
            }
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
        }

        paint.end_paint(&ps);
    }

    fn get_paint_objects(&self) -> Vec<(i32, i32, i32, HBRUSH)> {
        let bodies = self.engine.objects.lock().unwrap().clone();
        let mut camera = self.camera.lock().unwrap();
        let mut target = self.current_target.lock().unwrap();

        let (screen_width_pix, screen_height_pix) = self.window.size();
        let w_128 = screen_width_pix as i128;
        let h_128 = screen_height_pix as i128;

        let screen_scalar = screen_width_pix as f64 / 2.0 / (camera.fov / 2.0).to_radians().tan();

        if !(*target).is_empty() {
            camera.target = match bodies.iter().find(|x| x.name == *target) {
                Some(b) => b.position.clone(),
                None => {
                    (*target).clear();
                    Vector4d::default()
                }
            };
        }
        
        let transform = camera.get_full_transformation();

        let mut output: Vec<(i32, i32, i32, HBRUSH)> = Vec::new();

        let mut sorted_indices: Vec<usize> = (0..bodies.len()).collect();
        sorted_indices.sort_by(|a, b| bodies[*b].cmp(&bodies[*a], &camera.get_position()));

        let mut targets= self.targets.lock().unwrap();
        targets.clear();

        for i in sorted_indices {
            let body = &bodies[i];
            let pos = transform.multiply_vec(&body.position);

            if pos.data[2] >= 1.0 {
                continue;
            }

            let distance_scalar = 1.0 - pos.data[2];
            
            let center_x = pos.data[0] / distance_scalar * screen_scalar;
            let center_y = pos.data[1] / distance_scalar * screen_scalar;

            let radius_without_mag = body.radius / camera.distance / distance_scalar * screen_scalar;
            let radius_with_mag = radius_without_mag * body.magnification.powf(1.0 / 3.0);

            let res_x: i32 = match (center_x.round() as i128 + w_128 / 2).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let res_y: i32 = match (center_y.round() as i128 + h_128 / 2).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let res_radius_without_mag: i32 = match (radius_without_mag.round() as u128).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let res_radius_with_mag: i32 = match (radius_with_mag.round() as u128).try_into() {
                Ok(num) => num,
                Err(_) => continue
            };

            let max_magnification = 8;
            let mut res_radius = if res_radius_without_mag < max_magnification { res_radius_with_mag.min(max_magnification) } else { res_radius_without_mag };
            res_radius = res_radius.max(3);

            let [r, g, b] = body.color;
            output.push(
                (
                    res_x,
                    res_y,
                    res_radius,
                    self.get_brush(r, g, b)
                )
            );
            targets.push(TargetData { name: body.name.clone(), x: res_x, y: res_y, radius: res_radius });
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
            colors.push(new_color);
            brush
        }        
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let mut _app = DrawingApp::build_ui(Default::default()).expect("Failed to build UI");
    *_app.engine.framerate.lock().unwrap() = FRAMERATE;
    *_app.engine.target_speed.lock().unwrap() = 86400.0 * 1.0;
    for _ in 0..500 {
        let mut objects = _app.engine.objects.lock().unwrap();
        let new_object = astronomy::AstronomicalObject::place_on_orbit(
            astronomy::AstronomicalObject::get_random_planet(), 
            &objects[0]
        );

        objects.push(
            new_object
        );
    }
    
    _app.animation_timer.start();        
    nwg::dispatch_thread_events();

    // Make sure extra threads end cleanly
    _app.engine.stop();
}
