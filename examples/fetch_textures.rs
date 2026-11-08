//! Downloads the bodies' surface maps into `assets/textures/` (gitignored), for the 3D view:
//!
//! cargo run --release --example fetch_textures [-- --force] [Body...]
//!
//! Each source is checked against its SHA-256, scaled to its target size, shifted so 0° longitude
//! is in the middle and saved as `<body>.jpg`. Bodies that already have a file are skipped unless
//! `--force` is given or they're named. The downloads themselves aren't kept: a few are
//! uncompressed TIFFs of 100-200 MB, about 800 MB in all. Needs `curl` and `sha256sum`.
//!
//! Everything is real spacecraft data except the Sun, Venus's clouds and the gas giants, which
//! come from Solar System Scope. Unmapped areas (black in the sources: Pluto's and Charon's south,
//! the Uranian moons' and Triton's north, gaps at Ganymede's poles) are filled with the map's
//! average color, so they don't look like shadow.

use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use image::imageops::{self, FilterType};
use image::{ImageReader, RgbImage};

struct Source {
    body: &'static str,
    url: &'static str,
    sha256: &'static str,
    /// Target width; the height is half of it
    width: u32,
    /// East longitude at the middle of the source image (degrees)
    center_longitude: f64,
    credit: &'static str,
}

const SSS: &str = "Solar System Scope (solarsystemscope.com), CC BY 4.0";
const CASSINI: &str = "NASA/JPL-Caltech/Space Science Institute/Lunar and Planetary Institute";
const USGS_GALILEO: &str = "USGS Astrogeology, NASA/JPL (Galileo SSI, Voyager)";
const VOYAGER_URANUS: &str = "NASA/JPL-Caltech/USGS (Voyager 2)";

#[rustfmt::skip]
const SOURCES: &[Source] = &[
    // Gas giants, the Sun and Venus's clouds have no fixed surface, so their longitude is moot
    Source { body: "Sun", url: "https://www.solarsystemscope.com/textures/download/8k_sun.jpg",
        sha256: "f22b1cfb306ddce72a7e3b628668a0175b745038ce6268557cb2f7f1bdf98b9d", width: 4096, center_longitude: 0.0, credit: SSS },
    Source { body: "Mercury",
        url: "https://upload.wikimedia.org/wikipedia/commons/6/62/Mercury_MESSENGER_MDIS_Basemap_MD3Color_Mosaic_Global_32ppd.jpg",
        sha256: "67035c4fb6f6a33748580dd35ba3f97e88d7a57f1b1a45a116974a5a6ac18aa8", width: 4096, center_longitude: 0.0,
        credit: "NASA/JHUAPL/Carnegie Institution of Washington (MESSENGER MDIS)" },
    Source { body: "Venus", url: "https://www.solarsystemscope.com/textures/download/4k_venus_atmosphere.jpg",
        sha256: "a14aeba3c6ad7a56f989bb32008dec548077aeded420495b34df98f2dc14ba91", width: 4096, center_longitude: 0.0, credit: SSS },
    // January, since the simulation starts on 2026-01-01
    Source { body: "Earth",
        url: "https://assets.science.nasa.gov/content/dam/science/esd/eo/images/bmng/bmng-topography/january/world.topo.200401.3x5400x2700.jpg",
        sha256: "9743175942473681d78838ca1042553dab4779cf957fb6d8f22a81fb796f7797", width: 4096, center_longitude: 0.0,
        credit: "NASA Earth Observatory, Reto Stöckli (Blue Marble Next Generation)" },
    Source { body: "Moon", url: "https://svs.gsfc.nasa.gov/vis/a000000/a004700/a004720/lroc_color_poles_4k.tif",
        sha256: "918649a7f8ed2f1329b2cd95bb0d25483befdcb60ae1a66db681a637cc21344f", width: 4096, center_longitude: 0.0,
        credit: "NASA Scientific Visualization Studio (CGI Moon Kit), LRO data NASA/GSFC/ASU" },
    Source { body: "Mars",
        url: "https://upload.wikimedia.org/wikipedia/commons/1/1f/Mars_Viking_MDIM21_ClrMosaic_1km.jpg",
        sha256: "fdfcd335559c3dc67052b7e8a9565d850e336ac0d1f3ea7f5eb7826ffb44ecb2", width: 4096, center_longitude: 0.0, credit: "NASA/JPL/USGS (Viking MDIM 2.1)" },
    Source { body: "Phobos", url: "https://planetarymaps.usgs.gov/mosaic/Phobos_ME_SRC_Mosaic_Global_16ppd.tif",
        sha256: "4106b826ce0035c3d8e58c773deec5434cb2a642ddc6569131d9476a03074ff5", width: 2048, center_longitude: 0.0,
        credit: "ESA/DLR/FU Berlin (Mars Express SRC), USGS Astrogeology" },
    // Longitude unknown
    Source { body: "Deimos",
        url: "https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/Images%20and%20Textures/Mars%20-%20Deimos/Mars%20-%20Deimos.jpg",
        sha256: "204e89cc7dc128e9fa6ae5f14cc23dc7917a4a8ccab9feff1a9544d22b354dce", width: 1024, center_longitude: 0.0, credit: "NASA 3D Resources" },
    Source { body: "Jupiter", url: "https://www.solarsystemscope.com/textures/download/8k_jupiter.jpg",
        sha256: "0bd844bf20822c4e3e80882b077859833c0dac44c7e4e1e0cd63d1b1b6d43085", width: 4096, center_longitude: 0.0, credit: SSS },
    Source { body: "Io", url: "https://planetarymaps.usgs.gov/mosaic/Io_GalileoSSI-Voyager_Global_Mosaic_ClrMerge_1km.tif",
        sha256: "135f88cb645bfdb4e045b380971ab2ba14e230590c96c9c86cfe7310fda881ac", width: 4096, center_longitude: 0.0, credit: USGS_GALILEO },
    Source { body: "Europa", url: "https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_GalileoSSI_global_mosaic_500m.tif",
        sha256: "a323f0c9ccb47d5af9902ea8297fe81f9a9708795645b80801f103c3f7c9a624", width: 4096, center_longitude: 180.0, credit: USGS_GALILEO },
    Source { body: "Ganymede", url: "https://planetarymaps.usgs.gov/mosaic/Ganymede_Voyager_GalileoSSI_Global_ClrMosaic_1435m.tif",
        sha256: "da47087e75bc530456d8206d5100ad3c687706353700b0cb44a8400957005a33", width: 4096, center_longitude: 180.0, credit: USGS_GALILEO },
    Source { body: "Callisto", url: "https://planetarymaps.usgs.gov/mosaic/Callisto_Voyager_GalileoSSI_global_mosaic_1km.tif",
        sha256: "e1f0bd2e0e05de605d067d6b5f5ededddaf31ca6c064562a1ca770f15a7dbaa3", width: 4096, center_longitude: 180.0, credit: USGS_GALILEO },
    Source { body: "Saturn", url: "https://www.solarsystemscope.com/textures/download/8k_saturn.jpg",
        sha256: "0d39a4a490c87c3edabe00a3881a29bb3418364178c79c534fe0986e97e09853", width: 4096, center_longitude: 0.0, credit: SSS },
    // The Saturnian moons' maps have their dark trailing side (90° E) at a quarter of the width
    Source { body: "Mimas", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia18/pia18437/PIA18437.jpg",
        sha256: "303d5653875d507e8859a26d0afdedd0173c7d4e691f27c3266ca63d1aa4f97f", width: 2048, center_longitude: 180.0, credit: CASSINI },
    Source { body: "Enceladus", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia18/pia18435/PIA18435.jpg",
        sha256: "5b60d0161efd659f6d95281559ad4b30826fd02ee72944dbb3f8b7679f65ed4d", width: 2048, center_longitude: 180.0, credit: CASSINI },
    Source { body: "Tethys", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia18/pia18439/PIA18439.jpg",
        sha256: "aa3c249de2432699157638dae11c6f5c5e22c3dda94dd803af8666269f1a3da4", width: 2048, center_longitude: 180.0, credit: CASSINI },
    Source { body: "Dione", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia18/pia18434/PIA18434.jpg",
        sha256: "5e885d0e3bac1e1ee36934063d5c9cdf609dc10ad44677cba31bc9e703b674ff", width: 2048, center_longitude: 180.0, credit: CASSINI },
    Source { body: "Rhea", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia18/pia18438/PIA18438.jpg",
        sha256: "4a3f4c5cca6ec558a6dcd664ee818dd700df527fc07fe623d1c56d4e2ae81bc4", width: 2048, center_longitude: 180.0, credit: CASSINI },
    Source { body: "Titan", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia22/pia22770/PIA22770.jpg",
        sha256: "fb0bede43c59e34f313ba0306808ef60b10d2793673cf199c277a2f03432f07c", width: 4096, center_longitude: 180.0,
        credit: "NASA/JPL-Caltech/Space Science Institute (Cassini ISS)" },
    Source { body: "Iapetus", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia18/pia18436/PIA18436.jpg",
        sha256: "6e0e1d17a50343d9176912d047392c4f7cd67c48bdb30789918a4ff24d9f683b", width: 2048, center_longitude: 180.0, credit: CASSINI },
    Source { body: "Uranus", url: "https://www.solarsystemscope.com/textures/download/2k_uranus.jpg",
        sha256: "d15239d46f82d3ea13d2b260b5b29b2a382f42f2916dae0694d0387b1204a09d", width: 2048, center_longitude: 0.0, credit: SSS },
    // The Uranian moons: southern hemispheres only, longitude unknown
    Source { body: "Miranda",
        url: "https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/Images%20and%20Textures/Uranus%20-%20Miranda/Uranus%20-%20Miranda.jpg",
        sha256: "844b2deb88b8dbc93142bf5b85036ce7dd45db131e6251b4e6a95e179ae44c81", width: 1024, center_longitude: 0.0, credit: VOYAGER_URANUS },
    Source { body: "Ariel",
        url: "https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/Images%20and%20Textures/Uranus%20-%20Ariel/Uranus%20-%20Ariel.jpg",
        sha256: "fc0f8a59b5c29e44e4a3409e452f0427f5b53b8c9b41c89dc35d28babaae5cee", width: 1024, center_longitude: 0.0, credit: VOYAGER_URANUS },
    Source { body: "Umbriel",
        url: "https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/Images%20and%20Textures/Uranus%20-%20Umbriel/Uranus%20-%20Umbriel.jpg",
        sha256: "f1378bc2880bb0d4ddad50815083987aab38c9aae630cbd65465c6773495e511", width: 1024, center_longitude: 0.0, credit: VOYAGER_URANUS },
    Source { body: "Titania",
        url: "https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/Images%20and%20Textures/Uranus%20-%20Titania/Uranus%20-%20Titania.jpg",
        sha256: "087425752c8f5090965f5a4fac7d50a4c450563edd50b807bbc1d45ebeedd312", width: 1024, center_longitude: 0.0, credit: VOYAGER_URANUS },
    Source { body: "Oberon",
        url: "https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/Images%20and%20Textures/Uranus%20-%20Oberon/Uranus%20-%20Oberon.jpg",
        sha256: "ed29842e1b1fb96ee9626a62d4ee7bbf53756997ef33b1edda2dbead43383460", width: 1024, center_longitude: 0.0, credit: VOYAGER_URANUS },
    Source { body: "Neptune", url: "https://www.solarsystemscope.com/textures/download/2k_neptune.jpg",
        sha256: "cb42ea82709741d28b0af44d8b283cbc6dbd0c521a7f0e1e1e010ade00977df6", width: 2048, center_longitude: 0.0, credit: SSS },
    // Southern hemisphere only, longitude unknown
    Source { body: "Triton", url: "https://upload.wikimedia.org/wikipedia/commons/6/61/Triton_map_no_grid.jpg",
        sha256: "ff533af3163f53cc5560cd983cfe0f06b1c2cd0f3db36518d4c5cc5f5ddfeabb", width: 2048, center_longitude: 0.0,
        credit: "NASA/JPL-Caltech/Lunar and Planetary Institute (Voyager 2)" },
    // Sputnik Planitia (175° E) in the middle
    Source { body: "Pluto", url: "https://assets.science.nasa.gov/content/dam/science/psd/photojournal/pia/pia11/pia11707/PIA11707.jpg",
        sha256: "e36f3daa1b3b0e911418ff830ddccf8b63e10a082c6ccc96eb7d8170991962d6", width: 2048, center_longitude: 180.0, credit: "NASA/JHUAPL/SwRI (New Horizons)" },
    // The side facing Pluto in the middle
    Source { body: "Charon", url: "https://upload.wikimedia.org/wikipedia/commons/3/39/Charon_map_iau1803c.jpg",
        sha256: "a8c2ff578b810f3ec84a42751d3a00920fa4c610f5125e938e294aa4e0804405", width: 2048, center_longitude: 0.0, credit: "NASA/JHUAPL/SwRI (New Horizons)" },
    Source { body: "Ceres", url: "https://planetarymaps.usgs.gov/mosaic/Ceres_Dawn_FC_DLR_global_20ppd_Oct2015.tif",
        sha256: "920f65e7d0718c9f8d5e86783f35e0b466096251a8c1f2d01a7748a6d534c154", width: 2048, center_longitude: 180.0,
        credit: "NASA/JPL-Caltech/UCLA/MPS/DLR/IDA (Dawn), USGS Astrogeology" },
    // 330° at the left edge
    Source { body: "Vesta", url: "https://upload.wikimedia.org/wikipedia/commons/6/6c/Vesta_map_for_GeoHack.png",
        sha256: "f211e4b86601a22df9526cada4a68b3e5061a8a35038fca7e339e2db28edfffb", width: 2048, center_longitude: 150.0, credit: "NASA/JPL-Caltech/UCLA/MPS/DLR/IDA (Dawn)" },
];

/// Ring systems: one strip from the inner edge (left) to the outer edge (right), with the rings'
/// opacity in alpha. Saved as `<body>_ring.png`, one pixel high.
#[rustfmt::skip]
const RINGS: &[Source] = &[
    Source { body: "Saturn", url: "https://www.solarsystemscope.com/textures/download/8k_saturn_ring_alpha.png",
        sha256: "f1f826933c9ff87d64ecf0518d6256b8ed990b003722794f67e96e3d2b876ae4", width: 4096,
        center_longitude: 0.0, credit: SSS },
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Map,
    Ring,
}

/// Wikimedia asks automated clients to say who they are, and rate limits the rest
const USER_AGENT: &str = "solar_rust-fetch_textures/0.1 (hobby solar system simulator)";
const ATTEMPTS: u32 = 4;

fn main() {
    let force = std::env::args().any(|arg| arg == "--force");
    let named: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with("--"))
        .collect();
    if let Some(unknown) = named.iter().find(|n| !SOURCES.iter().any(|s| s.body == *n)) {
        eprintln!("No source for {unknown}");
        std::process::exit(1);
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/textures");
    std::fs::create_dir_all(&directory).expect("Failed to create assets/textures");

    let mut failed = Vec::new();
    let all = SOURCES
        .iter()
        .map(|source| (source, Kind::Map))
        .chain(RINGS.iter().map(|source| (source, Kind::Ring)));
    for (source, kind) in all {
        let is_named = named.iter().any(|n| n == source.body);
        if !named.is_empty() && !is_named {
            continue;
        }
        let (name, label) = match kind {
            Kind::Map => (file_name(source.body), source.body.to_string()),
            Kind::Ring => (
                ring_file_name(source.body),
                format!("{}'s rings", source.body),
            ),
        };
        let output = directory.join(&name);
        if output.exists() && !force && !is_named {
            println!("{label}: already there");
            continue;
        }
        let download = directory.join(format!("{name}.download"));
        let result = fetch(source, &download).and_then(|()| match kind {
            Kind::Map => convert(source, &download, &output),
            Kind::Ring => convert_ring(source, &download, &output),
        });
        let _ = std::fs::remove_file(&download);
        match result {
            Ok(()) => println!("{label}"),
            Err(error) => {
                eprintln!("{label}: {error}");
                failed.push(label);
            }
        }
    }

    println!("\nCredits:");
    for source in SOURCES.iter().chain(RINGS) {
        println!("  {}: {}", source.body, source.credit);
    }
    if !failed.is_empty() {
        eprintln!("\nFailed: {}", failed.join(", "));
        std::process::exit(1);
    }
}

/// Downloads the source to `path` and checks that it's the expected file
fn fetch(source: &Source, path: &Path) -> Result<(), String> {
    let mut attempt = 1;
    loop {
        let status = Command::new("curl")
            .args(["--silent", "--show-error", "--fail", "--location"])
            .args(["--user-agent", USER_AGENT, "--output"])
            .arg(path)
            .arg(source.url)
            .status()
            .map_err(|e| format!("Failed to run curl: {e}"))?;
        if status.success() {
            break;
        }
        if attempt == ATTEMPTS {
            return Err(format!("Download failed: {}", source.url));
        }
        // Wikimedia answers 429 when it's had enough for a while
        let wait = 15 * attempt as u64;
        eprintln!("{}: download failed, trying again in {wait} s", source.body);
        std::thread::sleep(Duration::from_secs(wait));
        attempt += 1;
    }

    // Some servers answer errors with a web page, so make sure it's an image
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|e| e.to_string())?;
    let image = magic.starts_with(&[0xFF, 0xD8, 0xFF])
        || magic == *b"\x89PNG"
        || magic == *b"II*\0"
        || magic == *b"MM\0*";
    if !image {
        return Err(format!("{} didn't return an image", source.url));
    }

    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| format!("Failed to run sha256sum: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let sum = text.split_whitespace().next().unwrap_or("");
    if source.sha256.is_empty() {
        println!(
            "{}: no checksum to compare with, it has SHA-256 {sum}",
            source.body
        );
    } else if sum != source.sha256 {
        return Err(format!(
            "{} has changed: SHA-256 {sum}, expected {}",
            source.url, source.sha256
        ));
    }
    Ok(())
}

/// Writes the source as an equirectangular map with 0° longitude in the middle, at its target size
fn convert(source: &Source, path: &Path, output: &Path) -> Result<(), String> {
    let mut reader = ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|e| e.to_string())?;
    // The biggest sources take a couple of GB decoded, more than the default limit
    reader.no_limits();
    let mut image = reader.decode().map_err(|e| e.to_string())?.into_rgb8();
    fill_unmapped(&mut image);
    let resized = imageops::resize(&image, source.width, source.width / 2, FilterType::Lanczos3);
    shift_longitude(&resized, source.center_longitude)
        .save_with_format(output, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())
}

/// Writes the middle row of a ring strip at its target width, keeping the opacity
fn convert_ring(source: &Source, path: &Path, output: &Path) -> Result<(), String> {
    let image = ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let row = imageops::crop_imm(&image, 0, image.height() / 2, image.width(), 1).to_image();
    imageops::resize(&row, source.width, 1, FilterType::Lanczos3)
        .save_with_format(output, image::ImageFormat::Png)
        .map_err(|e| e.to_string())
}

/// Pixels this dark are where the source has no data. JPEG noise keeps them from being exactly 0.
const UNMAPPED: u8 = 8;

/// Paints unmapped pixels in the average color of the mapped ones. Done before scaling, so the
/// black doesn't bleed into the edges of the mapped areas.
fn fill_unmapped(image: &mut RgbImage) {
    let unmapped = |p: &image::Rgb<u8>| p.0.iter().all(|&c| c <= UNMAPPED);
    let (mut sum, mut count) = ([0u64; 3], 0u64);
    for pixel in image.pixels().filter(|p| !unmapped(p)) {
        for c in 0..3 {
            sum[c] += pixel[c] as u64;
        }
        count += 1;
    }
    let Some(average) = (count > 0).then(|| sum.map(|s| (s / count) as u8)) else {
        return;
    };
    for pixel in image.pixels_mut().filter(|p| unmapped(p)) {
        pixel.0 = average;
    }
}

/// Rolls a map centered on `center_longitude` sideways so that 0° is in the middle. With east to
/// the right, 0° is L/360 of the width left of the middle, so everything moves right by that much.
fn shift_longitude(image: &RgbImage, center_longitude: f64) -> RgbImage {
    let width = image.width() as i64;
    let shift = ((center_longitude / 360.0 * width as f64).round() as i64).rem_euclid(width);
    let mut shifted = RgbImage::new(image.width(), image.height());
    for (x, y, pixel) in image.enumerate_pixels() {
        shifted.put_pixel(((x as i64 + shift) % width) as u32, y, *pixel);
    }
    shifted
}

/// Must match `render::textures::ring_file_name` in the app
fn ring_file_name(body: &str) -> String {
    file_name(body).replace(".jpg", "_ring.png")
}

/// Must match `render::textures::file_name` in the app
fn file_name(body: &str) -> String {
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
