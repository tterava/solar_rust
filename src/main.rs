mod camera;
mod engine;
mod astronomy;
mod input;
mod integration;
mod events;
mod ui;

use crate::engine::Engine;
use crate::camera::Camera;

extern crate native_windows_gui as nwg;
extern crate native_windows_derive as nwd;

use astronomy::AstronomicalObject;
use nwd::NwgUi;
use nwg::{NativeUi, ExternCanvas, Window};
use rand::SeedableRng;
use ui::TargetData;
use uuid::Uuid;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use std::mem;
use std::cell::RefCell;
use std::sync::{Mutex, Arc};
use winapi::shared::windef::{HBRUSH, HPEN, HFONT};
use winapi::um::wingdi::{CreateSolidBrush, CreatePen, PS_SOLID, CreateFontW, FW_NORMAL, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, DEFAULT_QUALITY, DEFAULT_PITCH, FF_DONTCARE, FW_BOLD, RGB};

const FRAMERATE: u32 = 100;

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
        ui::paint(self, data);
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
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    nwg::init().expect("Failed to init Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let app = DrawingApp {
        animation_timer: nwg::AnimationTimer::default(),
        camera: Arc::new(Mutex::new(Camera::default())),
        window: Window::default(),
        canvas: ExternCanvas::default(),
        paint_data: RefCell::new(PaintData::default()),
        engine: Engine::default(&mut rng),
        is_dragging: Arc::new(AtomicBool::new(false)),
        current_target: RefCell::new(None),
        targets: RefCell::new(Vec::new()),
        colors: RefCell::new(Vec::new()),
        next_status_update: RefCell::new(Instant::now()),
        status_lines: RefCell::new(Vec::new()),
        object_description: RefCell::new(Vec::new())
    };

    let app_ui = DrawingApp::build_ui(app).expect("Failed to build UI");

    *app_ui.engine.framerate.lock().unwrap() = FRAMERATE;
    app_ui.engine.params.lock().unwrap().target_speed = 86400.0 * 1.0;

    for _ in 0..2000 {
        let mut objects = app_ui.engine.objects.lock().unwrap();
        let orbital = AstronomicalObject::get_random_planet(&mut rng);
        let object = AstronomicalObject::place_on_orbit(orbital, &objects[0], &mut rng);

        objects.push(object);
    }
    
    app_ui.animation_timer.start();        
    nwg::dispatch_thread_events();

    // Make sure extra threads end cleanly
    app_ui.engine.stop();
}
