use std::sync::atomic::Ordering;
use std::sync::{RwLock, Arc, atomic::AtomicBool, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::astronomy::AstronomicalObject;

#[derive(Default)]
pub struct Engine {
    objects: Arc<RwLock<Vec<AstronomicalObject>>>,
    framerate: u64
}

impl Engine {
    pub fn init(framerate: u64) -> Engine {
        Engine {
            objects: Arc::new(RwLock::new(AstronomicalObject::default())),
            framerate
        }
    }

    // Updater reads the astronomical objects between screen updated. This way we avoid choppy framerate
    pub fn get_updater(&self, update_request: Arc<AtomicBool>, object_buffer: Arc<Mutex<Vec<AstronomicalObject>>>) -> (Arc<AtomicBool>, JoinHandle<()>) {
        let kill = Arc::new(AtomicBool::new(false));
        let kill_clone = kill.clone();
        let objects_clone = self.objects.clone();

        let framerate = self.framerate;
        let handle = thread::spawn(move || {
            println!("Starting updater thread");
            let interval: Duration = Duration::from_millis(1000 / (framerate * 3));
            while !kill_clone.load(Ordering::Relaxed) {
                let do_update = update_request.load(Ordering::Relaxed);
                if !do_update {
                    thread::sleep(interval);
                }

                let objects = objects_clone.read().unwrap();
                let mut buffer = object_buffer.lock().unwrap();

                *buffer = objects.clone();

                update_request.store(false, Ordering::Relaxed);
                thread::sleep(interval);
            }

            println!("Killing updater thread");
        });

        (kill, handle)
    }
}

