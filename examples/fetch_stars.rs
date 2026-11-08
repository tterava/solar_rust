//! Downloads the Yale Bright Star Catalogue (5th revised edition, Hoffleit & Warren 1991, public
//! domain) and writes the stars' positions, brightness and colors to `data/stars.csv`, for the
//! sky behind the 3D view:
//!
//! cargo run --example fetch_stars
//!
//! About 9100 stars, everything brighter than magnitude 6.5. Needs `curl`, `gzip` and `sha256sum`.

use std::process::Command;

/// Harvard's copy only answers over plain HTTP, so the checksum is what vouches for it. CDS has
/// the same file.
const URLS: &[&str] = &[
    "http://tdc-www.harvard.edu/catalogs/bsc5.dat.gz",
    "https://cdsarc.cds.unistra.fr/ftp/V/50/catalog.gz",
];
const SHA256: &str = "0471ced07fc241e625613baa689b2fa5f4b59fd733f731003e57d2d436036cdb";

fn main() {
    let directory = std::env::temp_dir().join("solar_rust_fetch_stars");
    std::fs::create_dir_all(&directory).expect("Failed to create a temporary directory");
    let download = directory.join("bsc5.dat.gz");

    let fetched = URLS.iter().any(|url| {
        let ok = Command::new("curl")
            .args(["--silent", "--show-error", "--fail", "--location"])
            .args(["--max-time", "120", "--output"])
            .arg(&download)
            .arg(url)
            .status()
            .is_ok_and(|status| status.success());
        if !ok {
            eprintln!("Couldn't download {url}");
        }
        ok && sha256(&download) == SHA256
    });
    if !fetched {
        eprintln!("No mirror gave the expected file (SHA-256 {SHA256})");
        std::process::exit(1);
    }

    let output = Command::new("gzip")
        .arg("-dc")
        .arg(&download)
        .output()
        .expect("Failed to run gzip");
    let _ = std::fs::remove_dir_all(&directory);
    let catalog = String::from_utf8_lossy(&output.stdout);

    let mut csv = String::from(
        "# Yale Bright Star Catalogue, 5th revised edition (Hoffleit & Warren 1991). J2000.\n\
         ra_deg,dec_deg,vmag,b_v\n",
    );
    let mut count = 0;
    for line in catalog.lines() {
        if let Some(star) = parse(line) {
            csv += &star;
            count += 1;
        }
    }

    let path = format!("{}/data/stars.csv", env!("CARGO_MANIFEST_DIR"));
    std::fs::write(&path, csv).expect("Failed to write the stars file");
    println!("Wrote {count} stars to {path}");
}

/// One star as a CSV line, from the catalogue's fixed columns (see its ReadMe). Entries without a
/// position are objects later found not to be stars; a missing B-V is taken as the Sun's, 0.65.
fn parse(line: &str) -> Option<String> {
    let field = |from: usize, to: usize| line.get(from - 1..to).map(str::trim);
    let number = |from, to| field(from, to).and_then(|f| f.parse::<f64>().ok());

    let ra = (number(76, 77)? + number(78, 79)? / 60.0 + number(80, 83)? / 3600.0) * 15.0;
    let sign = if field(84, 84)? == "-" { -1.0 } else { 1.0 };
    let dec = sign * (number(85, 86)? + number(87, 88)? / 60.0 + number(89, 90)? / 3600.0);
    let vmag = number(103, 107)?;
    let bv = number(110, 114).unwrap_or(0.65);
    Some(format!("{ra:.5},{dec:.5},{vmag:.2},{bv:.2}\n"))
}

fn sha256(path: &std::path::Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("Failed to run sha256sum");
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}
