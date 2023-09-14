use std::sync::atomic::Ordering;
use std::sync::{RwLock, Arc, atomic::AtomicBool, Mutex};
use std::thread;
use std::time::Duration;

use crate::astronomy::AstronomicalObject;

pub struct Engine {
    objects: Arc<RwLock<Vec<AstronomicalObject>>>,
    framerate: u32
}

impl Engine {
    pub fn init() -> Engine {
        Engine {
            objects: Arc::new(RwLock::new(AstronomicalObject::default())),
            framerate: 60
        }
    }

    // Updater reads the astronomical objects between screen updated. This way we avoid choppy framerate
    pub fn get_updater(&self) -> (Arc<AtomicBool>, Arc<AtomicBool>, Arc<Mutex<Vec<AstronomicalObject>>>) {
        let kill = Arc::new(AtomicBool::new(false));
        let update_lock = Arc::new(AtomicBool::new(false));
        let object_buffer: Arc<Mutex<Vec<AstronomicalObject>>>;
        {
            let objects = self.objects.read().unwrap();
            let clone = objects.clone();
            object_buffer = Arc::new(Mutex::new(clone));
        }

        let kill_clone = kill.clone();
        let update_lock_clone = update_lock.clone();
        let object_buffer_clone = object_buffer.clone();
        let objects_clone = self.objects.clone();

        let framerate = self.framerate;
        thread::spawn(move || {
            let interval: Duration = Duration::from_millis((1000.0 / framerate as f64 / 3.0) as u64);
            while !kill_clone.load(Ordering::Relaxed) {
                let do_update = update_lock_clone.load(Ordering::Relaxed);
                if !do_update {
                    thread::sleep(interval);
                }

                let objects = objects_clone.read().unwrap();
                let mut buffer = object_buffer_clone.lock().unwrap();

                *buffer = objects.clone();

                update_lock_clone.store(false, Ordering::Relaxed);
                thread::sleep(interval);
            }
        });

        (kill, update_lock, object_buffer)
    }
}

