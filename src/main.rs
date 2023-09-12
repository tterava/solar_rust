/*!
    Small custom control example using GDI. NWG has no drawing API, so raw winapi must be used.

    Requires the following features: `cargo run --example basic_drawing_d --features "extern-canvas"`
*/
mod matrix;
use crate::matrix::Matrix4d;

extern crate native_windows_gui as nwg;
extern crate native_windows_derive as nwd;

use nwd::NwgUi;
use nwg::NativeUi;
use std::f64::consts::PI;
use std::time::Duration;
use std::{mem, thread, time};
use std::cell::{RefCell, Cell};
use std::sync::{Mutex, Arc};
use winapi::shared::windef::{HBRUSH, HPEN};
use winapi::um::wingdi::{CreateSolidBrush, CreatePen, Ellipse, Polygon, SelectObject, RGB, PS_SOLID};


pub struct PaintData {
    background: HBRUSH,
    border: HBRUSH,
    pen: HPEN,
    yellow: HBRUSH,
    white: HBRUSH,
    black: HBRUSH,
    red: HBRUSH
}

impl Default for PaintData {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

#[derive(Default, NwgUi)]
pub struct DrawingApp {
    #[nwg_control(size: (300, 300), position: (300, 300), title: "Drawing (that's a duck)", flags: "WINDOW|VISIBLE")]
    #[nwg_events( OnWindowClose: [nwg::stop_thread_dispatch()], OnInit: [DrawingApp::setup])]
    window: nwg::Window,

    // By default ExternCanvas is a window so we must specify the parent here
    #[nwg_control(parent: Some(&data.window), position: (10, 10), size: (280, 280))]
    #[nwg_events( 
        OnPaint: [DrawingApp::paint(SELF, EVT_DATA)],
        OnMousePress: [DrawingApp::events(SELF, EVT)],
    )]
    canvas: nwg::ExternCanvas,

    paint_data: RefCell<PaintData>,
    clicked: Cell<bool>,
    time: Arc<Mutex<f64>>,

    #[nwg_control(parent: window, interval: Duration::from_millis(1000/170))]
    #[nwg_events( OnTimerTick: [DrawingApp::inv] )]
    animation_timer: nwg::AnimationTimer,
}

impl DrawingApp {
    fn setup(&self) {
        let mut data = self.paint_data.borrow_mut();

        unsafe {
            data.background = CreateSolidBrush(RGB(190, 190, 255));
            data.border = CreateSolidBrush(RGB(100, 100, 255));
            data.pen = CreatePen(PS_SOLID as _, 2, RGB(20, 20, 20));
            data.yellow = CreateSolidBrush(RGB(255, 255, 0));
            data.white = CreateSolidBrush(RGB(255, 255, 255));
            data.black = CreateSolidBrush(RGB(10, 10, 10));
            data.red = CreateSolidBrush(RGB(255, 10, 0));
        }
        
    }

    fn inv(&self) {
        self.canvas.invalidate();
    }

    fn events(&self, evt: nwg::Event) {
        use nwg::Event as E;
        use nwg::MousePressEvent as M;

        match evt {
            E::OnMousePress(M::MousePressLeftUp) => { self.clicked.set(false); },
            E::OnMousePress(M::MousePressLeftDown) => { self.clicked.set(true); },
            E::OnTimerTick => { println!("Timer!") },
            _ => { },
        }

        self.canvas.invalidate();
    }

    fn get_coords_from_timer(&self) -> (i32, i32) {
        let time = *self.time.lock().unwrap();
        let x = (time.cos() * 100.0) as i32;
        let y = ((4.0 * time).sin() * 40.0) as i32;

        (x, y * -1)
    }

    fn paint(&self, data: &nwg::EventData) {
        

        use winapi::um::winuser::{FillRect, FrameRect};
        use winapi::shared::windef::POINT as P;
        
        let paint = data.on_paint();
        let ps = paint.begin_paint();
 
        unsafe {
            let p = self.paint_data.borrow();
            let hdc = ps.hdc;
            let rc = &ps.rcPaint;

            FillRect(hdc, rc, p.background as _);
            FrameRect(hdc, rc, p.border as _);

            SelectObject(hdc, p.pen as _);
            SelectObject(hdc, p.yellow as _);
            Ellipse(hdc, rc.left + 20, rc.top + 20, rc.right - 20, rc.bottom - 20);

            SelectObject(hdc, p.white as _);
            Ellipse(hdc, 60, 60, 130, 130);
            Ellipse(hdc, 150, 60, 220, 130);

            if self.clicked.get() {
                SelectObject(hdc, p.red as _);
            } else {
                SelectObject(hdc, p.black as _);
            }
            
            Ellipse(hdc, 80, 80, 110, 110);
            Ellipse(hdc, 170, 80, 200, 110);

            SelectObject(hdc, p.red as _);
            let pts = &[P{x: 60, y: 150}, P{x: 220, y: 150}, P{x: 140, y: 220}];
            Polygon(hdc, pts.as_ptr(), pts.len() as _);

            let (x, y) = self.get_coords_from_timer();
            Ellipse(hdc, x - 10 + 140, y - 10 + 140, x + 10 + 140, y + 10 + 140);
        }

        paint.end_paint(&ps);
    }

}

fn main() {
    // nwg::init().expect("Failed to init Native Windows GUI");
    // nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    // let mut _app = DrawingApp::build_ui(Default::default()).expect("Failed to build UI");
    // _app.animation_timer.start();
        
    // let time_clone = _app.time.clone();

    // let should_exit = Arc::new(Mutex::new(false));
    // let thread_should_exit = Arc::clone(&should_exit);

    // // Create a vector of thread handles
    // let handle =             
    //     thread::spawn(move || {
    //         let ten_millis = time::Duration::from_millis(1000/170);
    //         loop {
    //             thread::sleep(ten_millis);

    //             {
    //                 let should_exit = *thread_should_exit.lock().unwrap();
    //                 if should_exit {
    //                     println!("Exiting thread");
    //                     return;
    //                 }
    //             }

    //             let circle = 2.0 * std::f64::consts::PI;
    //             let mut time = time_clone.lock().unwrap();
    //             *time += circle / 100.0;
    //             if *time > circle {
    //                 *time -= circle;
    //             }
    //         }
    // });
        
    // nwg::dispatch_thread_events();

    // {
    //     let mut exit = should_exit.lock().unwrap();
    //     *exit = true;
    // }

    // handle.join().unwrap();

    let matrix: Matrix4d = Matrix4d {
        data: [[4.0, 3.2, 1.1, 1.0], [4.0, 3.2, 1.1, 1.0],[4.0, 3.2, 1.1, 1.0],[4.0, 3.2, 1.1, 1.0]]
    };
    let matrix2: Matrix4d = Matrix4d {
        data: [[5.0, 3.2, 4.1, 2.0], [4.0, 3.5, 1.1, 33.0],[4.0, 3.2, 1.7, 1.0],[4.0, 1.2, 1.1, 1.3]]
    };

    let matrix3 = matrix * matrix2;
    println!("{:#?}", matrix3);

    use crate::Matrix4d as M;
    let vec = [1.0, 0.0, 0.0, 1.0];
    let angle = std::f64::consts::PI / 2.0;
    let rotation = Matrix4d::rot_x(angle) * Matrix4d::rot_y(angle);
    let translation = Matrix4d::trans([10.0, 0.0, 0.0]);

    // let rotated = rotation * translation * vec;
    let rotated = M::trans([0.0, -5.0, 0.0]) * M::rot_y(PI * 3.0 / 2.0) * M::trans([9.0, 0.0, 0.0]);
    println!("{:?}", rotated);

    // let rotated = Matrix4d::trans([10.0, 0.0, 0.0]).mul(&Matrix4d::rot_x(angle)).mul(&Matrix4d::rot_y(angle)).mul_vec(vec);

    println!("{:?}", rotated * vec);

    let a = 10;
    let b = a + 18;

    println!("{}", a);
    

}

