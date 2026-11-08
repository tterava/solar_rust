//! Surface maps for the bodies, loaded from `assets/textures/` on background threads so the app
//! starts right away and textures appear as they're ready. A body without a file is drawn in its
//! plain color.
//!
//! The files are written by `cargo run --example fetch_textures`: equirectangular maps named
//! after the body (`earth.jpg`, `52_europa.jpg`), with east to the right, the north pole at the
//! top and 0° longitude in the middle, and ring strips (`saturn_ring.png`) running from the inner
//! edge to the outer edge, with the rings' opacity in alpha.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

/// Textures being loaded or waiting to be uploaded
static PENDING: AtomicUsize = AtomicUsize::new(0);

/// Whether all textures are loaded and uploaded
pub fn all_loaded() -> bool {
    PENDING.load(Ordering::Relaxed) == 0
}

/// Called once a loaded texture is uploaded
pub fn uploaded() {
    PENDING.fetch_sub(1, Ordering::Relaxed);
}

/// One level of a texture: RGBA, 8 bits per channel, sRGB encoded
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// What a texture is for
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// A body's map
    Body(Arc<str>),
    /// A body's rings
    Rings(Arc<str>),
}

/// A texture with all its mip levels, largest first
pub struct Loaded {
    pub surface: Surface,
    pub mips: Vec<Image>,
}

pub fn directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/textures")
}

/// Whether any body has a texture file
pub fn any_present() -> bool {
    std::fs::read_dir(directory()).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|e| e == "jpg"))
    })
}

/// The texture file for a body: its name in lower case, with anything but letters and digits
/// turned into underscores
pub fn file_name(body: &str) -> String {
    let stem: String = body
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{stem}.jpg")
}

/// The ring texture file for a body, like `saturn_ring.png`
pub fn ring_file_name(body: &str) -> String {
    file_name(body).replace(".jpg", "_ring.png")
}

/// Starts loading the textures of `bodies` that have a file, in order. They arrive on the returned
/// channel as they're ready.
pub fn spawn_loader(bodies: Vec<Arc<str>>) -> Receiver<Loaded> {
    let directory = directory();
    let queue: Vec<(Surface, PathBuf)> = bodies
        .into_iter()
        .flat_map(|body| {
            [
                (
                    directory.join(file_name(&body)),
                    Surface::Body(body.clone()),
                ),
                (directory.join(ring_file_name(&body)), Surface::Rings(body)),
            ]
        })
        .filter(|(path, _)| path.exists())
        .map(|(path, surface)| (surface, path))
        .rev()
        .collect();
    PENDING.store(queue.len(), Ordering::Relaxed);
    let queue = Arc::new(Mutex::new(queue));
    let (sender, receiver) = mpsc::channel();

    // Leave most of the cores to the simulation. Each worker holds one big image at a time.
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    for _ in 0..(cores / 2).clamp(1, 4) {
        let (queue, sender) = (queue.clone(), sender.clone());
        std::thread::spawn(move || loop {
            let Some((surface, path)) = queue.lock().unwrap().pop() else {
                return;
            };
            match load(&path) {
                Ok(mips) => {
                    if sender.send(Loaded { surface, mips }).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    eprintln!("Failed to load {}: {error}", path.display());
                    PENDING.fetch_sub(1, Ordering::Relaxed);
                }
            }
        });
    }
    receiver
}

fn load(path: &Path) -> Result<Vec<Image>, String> {
    let decoded = image::open(path).map_err(|e| e.to_string())?.into_rgba8();
    let (width, height) = decoded.dimensions();
    Ok(mip_chain(Image {
        width,
        height,
        pixels: decoded.into_raw(),
    }))
}

/// `image` followed by its mip levels down to 1x1. Each level averages 2x2 pixels of the one
/// before in linear light, so dark and bright details blend to the right brightness.
pub fn mip_chain(image: Image) -> Vec<Image> {
    let decode: Vec<f32> = (0..=255u8).map(srgb_to_linear).collect();
    let mut levels = vec![image];
    loop {
        let previous = levels.last().unwrap();
        if previous.width == 1 && previous.height == 1 {
            return levels;
        }
        let (width, height) = ((previous.width / 2).max(1), (previous.height / 2).max(1));
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let rows = [2 * y, (2 * y + 1).min(previous.height - 1)];
            for x in 0..width {
                let columns = [2 * x, (2 * x + 1).min(previous.width - 1)];
                let mut sum = [0.0f32; 4];
                for row in rows {
                    for column in columns {
                        let i = ((row * previous.width + column) * 4) as usize;
                        let pixel = &previous.pixels[i..i + 4];
                        for (total, &value) in sum.iter_mut().zip(&pixel[..3]) {
                            *total += decode[value as usize];
                        }
                        sum[3] += pixel[3] as f32;
                    }
                }
                pixels.extend(sum[..3].iter().map(|total| linear_to_srgb(total / 4.0)));
                pixels.push((sum[3] / 4.0).round() as u8);
            }
        }
        levels.push(Image {
            width,
            height,
            pixels,
        });
    }
}

pub fn srgb_to_linear(value: u8) -> f32 {
    let c = value as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> u8 {
    let c = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(file_name("Earth"), "earth.jpg");
        assert_eq!(file_name("52 Europa"), "52_europa.jpg");
        assert_eq!(file_name("Psyche (spacecraft)"), "psyche__spacecraft_.jpg");
        assert_eq!(ring_file_name("Saturn"), "saturn_ring.png");
    }

    #[test]
    fn srgb_round_trips() {
        for value in 0..=255u8 {
            assert_eq!(linear_to_srgb(srgb_to_linear(value)), value);
        }
    }

    #[test]
    fn mips_average_in_linear_light() {
        // A 4x2 checkerboard of black and white
        let pixels = (0..8)
            .flat_map(|i| {
                let v = if (i + i / 4) % 2 == 0 { 255 } else { 0 };
                [v, v, v, 255]
            })
            .collect();
        let mips = mip_chain(Image {
            width: 4,
            height: 2,
            pixels,
        });
        let sizes: Vec<_> = mips.iter().map(|m| (m.width, m.height)).collect();
        assert_eq!(sizes, [(4, 2), (2, 1), (1, 1)]);
        // Half white is 0.5 in linear light, which is 188 in sRGB, not 128
        assert_eq!(mips[1].pixels, [188, 188, 188, 255, 188, 188, 188, 255]);
        assert_eq!(mips[2].pixels, [188, 188, 188, 255]);
    }
}
