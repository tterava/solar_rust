//! Downloads the state of every known body from JPL Horizons, at 00:00 TDB on a date:
//!
//! cargo run --example fetch_horizons -- 2026-01-01
//!
//! Writes `data/horizons_<date>.csv`. Needs `curl`.

use std::process::Command;

use solar_rust::ephemeris::{Date, BARYCENTERS, KNOWN_BODIES};

const API: &str = "https://ssd.jpl.nasa.gov/api/horizons.api";

fn main() {
    let date = std::env::args()
        .nth(1)
        .and_then(|arg| Date::parse(&arg))
        .expect("Usage: fetch_horizons YYYY-MM-DD");

    let mut csv = format!(
        "# JPL Horizons, {date} 00:00 TDB (JD {}). Barycentric, ecliptic and equinox of J2000.\n\
         name,horizons_id,x_km,y_km,z_km,vx_km_s,vy_km_s,vz_km_s\n",
        date.julian_date()
    );

    let barycenters = BARYCENTERS
        .iter()
        .map(|(planet, id)| (format!("{planet} barycenter"), *id));
    let bodies = KNOWN_BODIES
        .iter()
        .map(|body| (body.name.to_string(), body.horizons_id))
        .chain(barycenters);

    for (name, id) in bodies {
        match fetch_state(id, date) {
            Ok(state) => {
                println!("{name}");
                csv += &format!("{name},{id},{}\n", state.join(","));
            }
            // Spacecraft are only covered for limited periods
            Err(error) => eprintln!("Skipping {name}: {error}"),
        }
    }

    let path = format!("{}/data/horizons_{date}.csv", env!("CARGO_MANIFEST_DIR"));
    std::fs::write(&path, csv).expect("Failed to write the states file");
    println!("Wrote {path}");
}

/// Returns x, y, z, vx, vy, vz as Horizons printed them (km, km/s)
fn fetch_state(id: &str, date: Date) -> Result<Vec<String>, String> {
    let parameters = [
        ("COMMAND", id.to_string()),
        ("OBJ_DATA", "NO".to_string()),
        ("MAKE_EPHEM", "YES".to_string()),
        ("EPHEM_TYPE", "VECTORS".to_string()),
        ("CENTER", "500@0".to_string()), // Solar system barycenter
        ("REF_PLANE", "ECLIPTIC".to_string()),
        ("TLIST", date.julian_date().to_string()),
        ("TIME_TYPE", "TDB".to_string()),
        ("VEC_TABLE", "2".to_string()), // Position and velocity
        ("OUT_UNITS", "KM-S".to_string()),
        ("CSV_FORMAT", "YES".to_string()),
    ];

    let mut curl = Command::new("curl");
    curl.args(["--silent", "--show-error", "--fail", "--get", API]);
    curl.args(["--data-urlencode", "format=text"]);
    // Horizons wants every value (except format) in single quotes
    for (key, value) in parameters {
        curl.args(["--data-urlencode", &format!("{key}='{value}'")]);
    }

    let output = curl
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout);

    // The single result line is between $$SOE and $$EOE:
    // JDTDB, Calendar Date, X, Y, Z, VX, VY, VZ,
    let line = text
        .split("$$SOE")
        .nth(1)
        .and_then(|rest| rest.split("$$EOE").next())
        .map(str::trim)
        .ok_or_else(|| last_lines(&text))?;

    let fields: Vec<String> = line.split(',').map(|f| f.trim().to_string()).collect();
    if fields.len() < 8 {
        return Err(format!("Unexpected result line: {line}"));
    }
    Ok(fields[2..8].to_vec())
}

/// The end of a Horizons response, which is where it explains what went wrong
fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(5)..].join("\n")
}
