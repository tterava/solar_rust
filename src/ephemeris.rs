//! Real solar system: known bodies, and their states as published by JPL Horizons.
//!
//! States are barycentric, in the ecliptic and equinox of J2000 (x towards the vernal
//! equinox, z towards the ecliptic north pole), at 00:00 TDB on the given date. They're
//! fetched with `cargo run --example fetch_horizons -- <YYYY-MM-DD>` into `data/`.

use std::fmt;

use glam::{DMat3, DVec3};

use crate::astronomy::{Bodies, Emission, NewBody, Oblateness};
use crate::forces::G;

/// The date the simulation starts from
pub const EPOCH: Date = Date {
    year: 2026,
    month: 1,
    day: 1,
};
const EPOCH_STATES: &str = include_str!("../data/horizons_2026-01-01.csv");

pub struct KnownBody {
    pub name: &'static str,
    /// Target id for the Horizons API. "N;" is the asteroid with number N, "DES=..." a
    /// designation, and negative ids are spacecraft.
    pub horizons_id: &'static str,
    /// Gravitational parameter G * mass in km^3/s^2, which is known far more precisely
    /// than G or the mass alone
    pub gm: f64,
    /// Mean radius in km
    pub radius: f64,
    pub color: [u8; 3],
    pub magnification: f64,
}

const fn body(
    name: &'static str,
    horizons_id: &'static str,
    gm: f64,
    radius: f64,
    color: [u8; 3],
    magnification: f64,
) -> KnownBody {
    KnownBody {
        name,
        horizons_id,
        gm,
        radius,
        color,
        magnification,
    }
}

/// GM and radii are from Horizons' physical data for each body (DE441 and the JPL satellite
/// ephemerides), unless noted. Planets are their own GM, without their moons. Asteroid GMs are
/// the ones DE440 was integrated with (its header's MA constants). Radii of irregular bodies
/// are those of a sphere with the same volume.
///
/// GM 0 means the body's pull is ignored: its mass isn't known, or is too small to matter.
#[rustfmt::skip]
pub const KNOWN_BODIES: &[KnownBody] = &[
    body("Sun", "10", 132712440041.93938, 695700.0, [255, 255, 0], 100.0),
    body("Mercury", "199", 22031.86855, 2439.4, [160, 150, 140], 2.0E7),
    body("Venus", "299", 324858.592, 6051.84, [230, 205, 150], 2.0E7),
    body("Earth", "399", 398600.435436, 6371.01, [90, 140, 230], 1.0E7),
    body("Moon", "301", 4902.800066, 1737.4, [255, 255, 255], 1.0E7),
    body("Mars", "499", 42828.375662, 3389.92, [255, 50, 0], 2.0E7),
    body("Phobos", "401", 7.208E-4, 11.1, [200, 200, 200], 2.0E11), // From mass 1.08E16 kg
    body("Deimos", "402", 1.201E-4, 6.2, [150, 150, 150], 2.0E11), // From mass 1.80E15 kg
    body("Jupiter", "599", 126686531.900, 69911.0, [216, 202, 157], 2.0E7),
    body("Io", "501", 5959.9155, 1821.49, [230, 210, 90], 1.0E8),
    body("Europa", "502", 3202.7121, 1560.8, [200, 180, 150], 1.0E8),
    body("Ganymede", "503", 9887.8328, 2631.2, [160, 150, 140], 1.0E8),
    body("Callisto", "504", 7179.2834, 2410.3, [110, 100, 90], 1.0E8),
    body("Amalthea", "505", 0.1646, 83.5, [180, 90, 60], 1.0E9),
    body("Himalia", "506", 0.151552, 85.0, [140, 130, 120], 1.0E9),
    body("Elara", "507", 0.0, 40.0, [140, 130, 120], 1.0E10),
    body("Pasiphae", "508", 0.0, 18.0, [140, 130, 120], 1.0E10),
    body("Sinope", "509", 0.0, 14.0, [140, 130, 120], 1.0E10),
    body("Lysithea", "510", 0.0, 12.0, [140, 130, 120], 1.0E10),
    body("Carme", "511", 0.0, 15.0, [140, 130, 120], 1.0E10),
    body("Ananke", "512", 0.0, 10.0, [140, 130, 120], 1.0E10),
    body("Leda", "513", 0.0, 5.0, [140, 130, 120], 2.0E11),
    body("Thebe", "514", 0.0301, 49.3, [170, 100, 70], 1.0E10),
    body("Adrastea", "515", 0.0001, 8.2, [150, 120, 100], 2.0E11),
    body("Metis", "516", 0.0025, 21.5, [150, 120, 100], 1.0E10),
    body("Saturn", "699", 37931206.234, 58232.0, [191, 189, 175], 2.0E7),
    body("Mimas", "601", 2.503489, 198.8, [200, 200, 200], 1.0E9),
    body("Enceladus", "602", 7.210367, 252.3, [235, 235, 235], 1.0E9),
    body("Tethys", "603", 41.21, 536.3, [210, 210, 210], 1.0E9),
    body("Dione", "604", 73.116, 562.5, [190, 190, 190], 1.0E9),
    body("Rhea", "605", 153.94, 764.5, [180, 180, 180], 1.0E9),
    body("Titan", "606", 8978.14, 2575.5, [220, 170, 80], 1.0E8),
    body("Hyperion", "607", 0.3705, 133.0, [170, 150, 120], 1.0E9),
    body("Iapetus", "608", 120.52, 734.5, [160, 140, 120], 1.0E9),
    body("Phoebe", "609", 0.5548, 106.6, [90, 85, 80], 1.0E9),
    // Janus and Epimetheus share an orbit and swap places every 4 years, so their masses
    // matter. From masses 1.98E18 and 5.5E17 kg.
    body("Janus", "610", 0.13215, 89.7, [200, 200, 190], 1.0E9),
    body("Epimetheus", "611", 0.036709, 58.2, [200, 200, 190], 1.0E10),
    body("Helene", "612", 0.00047574, 16.0, [220, 220, 220], 1.0E10),
    body("Telesto", "613", 0.00048, 12.4, [220, 220, 220], 1.0E10),
    body("Calypso", "614", 0.00024, 9.6, [220, 220, 220], 2.0E11),
    body("Atlas", "615", 0.0, 15.1, [200, 190, 170], 1.0E10),
    body("Prometheus", "616", 0.009344, 43.1, [200, 190, 170], 1.0E10), // From mass 1.4E17 kg
    body("Pandora", "617", 0.0086766, 40.6, [200, 190, 170], 1.0E10), // From mass 1.3E17 kg
    body("Pan", "618", 0.0, 14.0, [200, 190, 170], 1.0E10),
    body("Uranus", "799", 5793950.6103, 25362.0, [209, 231, 231], 2.0E7),
    body("Miranda", "705", 4.3, 235.8, [170, 170, 180], 1.0E9),
    body("Ariel", "701", 83.43, 578.9, [180, 180, 190], 1.0E9),
    body("Umbriel", "702", 85.40, 584.7, [130, 130, 140], 1.0E9),
    body("Titania", "703", 222.8, 788.9, [175, 170, 170], 1.0E9),
    body("Oberon", "704", 205.34, 761.4, [160, 150, 150], 1.0E9),
    body("Puck", "715", 0.0, 77.0, [120, 120, 130], 1.0E9),
    body("Neptune", "899", 6835099.97, 24624.0, [39, 70, 135], 2.0E7),
    body("Triton", "801", 1428.495, 1352.6, [200, 190, 210], 1.0E8),
    body("Nereid", "802", 0.0, 170.0, [150, 150, 160], 1.0E9),
    body("Proteus", "808", 2.58, 208.0, [110, 110, 115], 1.0E9),
    // Never 4get
    body("Pluto", "999", 869.326, 1188.3, [190, 190, 255], 2.0E7),
    body("Charon", "901", 106.10, 606.0, [160, 160, 160], 1.0E8),
    body("Nix", "902", 0.001496, 18.0, [220, 220, 220], 1.0E10),
    body("Hydra", "903", 0.00201, 18.5, [220, 220, 220], 1.0E10),
    body("Kerberos", "904", 0.00006038, 6.0, [200, 200, 200], 2.0E11),
    body("Styx", "905", 0.0000405, 5.2, [200, 200, 200], 2.0E11),
    // Dwarf planets beyond Neptune, with masses from their moons' orbits
    body("Eris", "136199;", 1099.0, 1163.0, [235, 235, 235], 1.0E8), // Holler et al. 2021, Sicardy et al. 2011
    body("Haumea", "136108;", 267.37, 797.6, [230, 230, 230], 1.0E9), // Ragozzine & Brown 2009, Ortiz et al. 2017
    body("Makemake", "136472;", 0.0, 715.0, [200, 150, 120], 1.0E9), // Ortiz et al. 2012
    body("Gonggong", "225088;", 116.8, 615.0, [190, 110, 90], 1.0E9), // Kiss et al. 2019
    body("Quaoar", "50000;", 93.44, 555.0, [170, 120, 100], 1.0E9), // Fraser et al. 2013, Braga-Ribas et al. 2013
    body("Sedna", "90377;", 0.0, 498.0, [200, 90, 70], 1.0E9), // Pal et al. 2012
    body("Orcus", "90482;", 42.18, 458.0, [170, 170, 170], 1.0E9), // Brown et al. 2010, Fornasier et al. 2013
    // Asteroids: the largest, and the ones spacecraft have visited
    body("Ceres", "1;", 62.62888864, 469.7, [140, 130, 120], 1.0E9),
    body("Pallas", "2;", 13.66587815, 256.5, [150, 140, 130], 1.0E9),
    body("Juno", "3;", 1.92057070, 123.3, [160, 150, 140], 1.0E9),
    body("Vesta", "4;", 17.28823288, 261.385, [170, 160, 150], 1.0E9),
    body("Hygiea", "10;", 5.62514765, 203.56, [120, 110, 100], 1.0E9),
    body("Eunomia", "15;", 2.02302099, 115.8, [160, 150, 140], 1.0E9),
    body("Psyche", "16;", 1.58965824, 111.0, [170, 170, 160], 1.0E9),
    body("Lutetia", "21;", 0.08862093, 49.0, [140, 130, 120], 1.0E10),
    body("Euphrosyne", "31;", 1.07937146, 133.5, [110, 100, 95], 1.0E9),
    body("52 Europa", "52;", 2.68303592, 152.0, [120, 110, 100], 1.0E9),
    body("Sylvia", "87;", 2.16823207, 126.5, [120, 110, 100], 1.0E9),
    body("Davida", "511;", 3.89448315, 135.2, [120, 110, 100], 1.0E9),
    body("Interamnia", "704;", 2.83040964, 153.2, [120, 110, 100], 1.0E9),
    body("Hektor", "624;", 0.5273, 112.5, [130, 100, 90], 1.0E9), // Jupiter trojan. Marchis et al. 2014
    body("Ida", "243;", 0.00275, 16.0, [150, 140, 130], 1.0E10),
    body("Eros", "433;", 0.000446270, 8.42, [170, 140, 110], 2.0E11),
    body("Itokawa", "25143;", 2.1E-9, 0.165, [160, 150, 140], 2.0E11),
    body("Ryugu", "162173;", 3.0E-8, 0.448, [90, 85, 80], 2.0E11),
    body("Bennu", "101955;", 4.892E-9, 0.2412, [90, 85, 80], 2.0E11), // Scheeres et al. 2019
    body("Apophis", "99942;", 0.0, 0.170, [160, 150, 140], 2.0E11),
    body("Didymos", "65803;", 0.0, 0.390, [160, 150, 140], 2.0E11),
    body("Arrokoth", "486958;", 0.0, 9.2, [170, 90, 70], 2.0E11),
    // Comets, without the push of the gas they boil off, so they drift from JPL's
    // positions near the Sun. "CAP" picks the orbit solution for the current apparition.
    body("Halley", "DES=1P;CAP", 0.0, 5.5, [200, 220, 255], 2.0E11),
    body("Encke", "DES=2P;CAP", 0.0, 2.4, [200, 220, 255], 2.0E11),
    body("Tempel 1", "DES=9P;CAP", 0.0, 3.0, [200, 220, 255], 2.0E11),
    body("Churyumov-Gerasimenko", "DES=67P;CAP", 0.0, 2.0, [200, 220, 255], 2.0E11),
    body("Tempel-Tuttle", "DES=55P;CAP", 0.0, 1.8, [200, 220, 255], 2.0E11),
    body("Swift-Tuttle", "DES=109P;CAP", 0.0, 13.0, [200, 220, 255], 2.0E11),
    body("Hale-Bopp", "DES=C/1995 O1;", 0.0, 30.0, [200, 220, 255], 2.0E11),
    // Visitors from interstellar space
    body("'Oumuamua", "DES=1I;", 0.0, 0.1, [200, 120, 90], 2.0E11),
    body("Borisov", "DES=2I;", 0.0, 0.5, [200, 220, 255], 2.0E11),
    body("3I/ATLAS", "DES=3I;", 0.0, 1.0, [200, 220, 255], 2.0E11),
    // Spacecraft, which drift from JPL's predictions whenever they fire their engines
    body("James Webb Space Telescope", "-170", 0.0, 0.01, [230, 190, 60], 1.0E7),
    body("Parker Solar Probe", "-96", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Solar Orbiter", "-144", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("BepiColombo", "-121", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Hayabusa2", "-37", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("OSIRIS-APEX", "-64", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Psyche (spacecraft)", "-255", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Lucy", "-49", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Juno (spacecraft)", "-61", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Europa Clipper", "-159", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("JUICE", "-28", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("New Horizons", "-98", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Pioneer 10", "-23", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Pioneer 11", "-24", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Voyager 1", "-31", 0.0, 0.01, [230, 230, 230], 1.0E7),
    body("Voyager 2", "-32", 0.0, 0.01, [230, 230, 230], 1.0E7),
];

/// Planets with moons, and the Horizons id of their system's barycenter. A planet's moons have
/// ids with the same first digit as the planet's (the Earth's Moon is 301, the Earth 399).
///
/// The whole system is shifted so that its barycenter moves exactly as in JPL's planetary
/// ephemeris, even if the moon masses here differ slightly from the ones JPL used. The Earth and
/// the Moon are integrated individually in JPL's ephemeris, so they don't need this.
pub const BARYCENTERS: &[(&str, &str)] = &[
    ("Mars", "4"),
    ("Jupiter", "5"),
    ("Saturn", "6"),
    ("Uranus", "7"),
    ("Neptune", "8"),
    ("Pluto", "9"),
];

/// Zonal gravity harmonics: name, reference radius (km), [J2, J4, J6], and the north pole's
/// right ascension and declination (degrees, ICRF) in 2026 from the IAU WGCCRE 2015 report
/// (Archinal et al. 2018), ignoring its slow drift.
#[rustfmt::skip]
const OBLATENESS: &[(&str, f64, [f64; 3], f64, f64)] = &[
    // DE440
    ("Sun", 696000.0, [2.1961E-7, 0.0, 0.0], 286.13, 63.87),
    // IERS 2010 (Horizons) and EGM2008
    ("Earth", 6378.137, [1.08262545E-3, -1.6196E-6, 0.0], -0.167, 89.855),
    // MRO120 (Konopliv et al. 2016)
    ("Mars", 3396.0, [1.95545E-3, 0.0, 0.0], 317.654, 52.871),
    // Juno (Durante et al. 2020)
    ("Jupiter", 71492.0, [14696.5735E-6, -586.6085E-6, 34.2007E-6], 268.0549, 64.4959),
    // Cassini (Iess et al. 2019)
    ("Saturn", 60330.0, [16290.573E-6, -935.314E-6, 86.340E-6], 40.5796, 83.536),
    // Jacobson 2014
    ("Uranus", 25559.0, [3510.68E-6, -34.17E-6, 0.0], 257.311, -15.175),
    // Jacobson 2009
    ("Neptune", 25225.0, [3408.4E-6, -33.4E-6, 0.0], 299.50, 42.96),
];

/// Light given off, where it's known better than the mass and radius imply: the IAU 2015 nominal
/// solar luminosity and effective temperature (resolution B3)
const EMISSION: &[(&str, Emission)] = &[(
    "Sun",
    Emission {
        luminosity: Some(3.828E26),
        temperature: Some(5772.0),
    },
)];

/// Obliquity of the ecliptic at J2000 (IAU 1976), the angle between the ICRF equator and
/// the ecliptic. Horizons uses this value for its ecliptic frame.
const OBLIQUITY_DEGREES: f64 = 84381.448 / 3600.0;

/// Rotates equatorial (ICRF) vectors into the ecliptic frame
pub fn equatorial_to_ecliptic() -> DMat3 {
    DMat3::from_rotation_x(-OBLIQUITY_DEGREES.to_radians())
}

/// Unit vector in the ecliptic frame, from equatorial (ICRF) right ascension and declination
pub fn from_equatorial(ra_degrees: f64, dec_degrees: f64) -> DVec3 {
    let (ra, dec) = (ra_degrees.to_radians(), dec_degrees.to_radians());
    let equatorial = DVec3::new(dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin());
    equatorial_to_ecliptic() * equatorial
}

fn oblateness(name: &str) -> Option<Oblateness> {
    OBLATENESS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|&(_, radius, j, ra, dec)| Oblateness {
            radius: radius * 1000.0,
            j,
            pole: from_equatorial(ra, dec),
        })
}

/// Position (m) and velocity (m/s) of a body
#[derive(Clone, Copy, Debug)]
pub struct State {
    pub position: DVec3,
    pub velocity: DVec3,
}

/// Parses a states file written by the `fetch_horizons` example: `name,horizons_id` followed by
/// position (km) and velocity (km/s). Returns (horizons_id, state) pairs.
pub fn parse_states(csv: &str) -> Vec<(&str, State)> {
    csv.lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("name,"))
        .map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            let number = |i: usize| -> f64 {
                fields[i]
                    .trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("Bad number in states file: {line}"))
            };
            let vector = |i| DVec3::new(number(i), number(i + 1), number(i + 2)) * 1000.0;
            (
                fields[1],
                State {
                    position: vector(2),
                    velocity: vector(5),
                },
            )
        })
        .collect()
}

/// The solar system as it was at [`EPOCH`]
pub fn solar_system() -> Bodies {
    let states = parse_states(EPOCH_STATES);
    let mut bodies = Bodies::default();

    for known in KNOWN_BODIES {
        let Some((_, state)) = states.iter().find(|(id, _)| *id == known.horizons_id) else {
            panic!("No state for {} in the epoch states file", known.name);
        };
        bodies.push(NewBody {
            name: known.name.to_string(),
            mass: known.gm * 1.0E9 / G,
            radius: known.radius * 1000.0,
            position: state.position,
            velocity: state.velocity,
            oblateness: oblateness(known.name),
            magnification: known.magnification,
            color: known.color,
            emission: EMISSION
                .iter()
                .find(|(name, _)| *name == known.name)
                .map_or_else(Emission::default, |&(_, emission)| emission),
        });
    }

    for &(planet, barycenter_id) in BARYCENTERS {
        if let Some((_, barycenter)) = states.iter().find(|(id, _)| *id == barycenter_id) {
            place_by_barycenter(&mut bodies, planet, barycenter);
        }
    }

    bodies
}

/// Shifts a planet and its moons together so that their barycenter is at `barycenter`,
/// keeping their positions and velocities relative to each other
fn place_by_barycenter(bodies: &mut Bodies, planet: &str, barycenter: &State) {
    let Some(planet) = KNOWN_BODIES.iter().find(|b| b.name == planet) else {
        return;
    };

    // The planet and its moons: 3 digit ids starting with the same digit (asteroid ids end
    // in ";" and spacecraft ids start with "-")
    let index = |name: &str| bodies.info.iter().position(|i| &*i.name == name);
    let in_system = |id: &str| {
        id.len() == 3
            && id.bytes().all(|c| c.is_ascii_digit())
            && id[..1] == planet.horizons_id[..1]
    };
    let system: Vec<usize> = KNOWN_BODIES
        .iter()
        .filter(|b| in_system(b.horizons_id))
        .filter_map(|b| index(b.name))
        .collect();

    let total_mass: f64 = system.iter().map(|&b| bodies.mass[b]).sum();
    let center = |values: &[DVec3]| {
        system
            .iter()
            .map(|&b| values[b] * bodies.mass[b])
            .sum::<DVec3>()
            / total_mass
    };
    let position_shift = barycenter.position - center(&bodies.position);
    let velocity_shift = barycenter.velocity - center(&bodies.velocity);

    for &b in &system {
        bodies.position[b] += position_shift;
        bodies.velocity[b] += velocity_shift;
    }
}

/// A calendar date (proleptic Gregorian)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Parses YYYY-MM-DD
    pub fn parse(text: &str) -> Option<Date> {
        let mut parts = text.splitn(3, '-').map(str::parse::<i64>);
        let (year, month, day) = (
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        );
        let date = Date {
            year: year as i32,
            month: month as u32,
            day: day as u32,
        };
        (Date::from_days(date.days()) == date).then_some(date)
    }

    /// Days since 1970-01-01, from Howard Hinnant's `days_from_civil`
    pub fn days(self) -> i64 {
        let year = self.year as i64 - (self.month <= 2) as i64;
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let month = self.month as i64;
        let day_of_year =
            (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + self.day as i64 - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146097 + day_of_era - 719468
    }

    /// Inverse of [`Date::days`]
    pub fn from_days(days: i64) -> Date {
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let day_of_era = z - era * 146097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let mp = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        Date {
            year: (year_of_era + era * 400 + (month <= 2) as i64) as i32,
            month,
            day,
        }
    }

    /// Julian date at 00:00 of this date
    pub fn julian_date(self) -> f64 {
        self.days() as f64 + 2440587.5
    }

    /// Seconds from 00:00 of `self` to 00:00 of `later`
    pub fn seconds_until(self, later: Date) -> f64 {
        (later.days() - self.days()) as f64 * 86400.0
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// The date and time (TDB) `seconds` after the start of [`EPOCH`], like "2026-03-14 05:12"
pub fn format_time(seconds: f64) -> String {
    let days = (seconds / 86400.0).floor();
    let minutes = ((seconds - days * 86400.0) / 60.0).floor() as u32;
    let date = Date::from_days(EPOCH.days() + days as i64);
    format!("{date} {:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        assert_eq!(Date::parse("1970-01-01").unwrap().days(), 0);
        assert_eq!(EPOCH.julian_date(), 2461041.5); // As Horizons reports it
        assert_eq!(Date::parse("2026-02-30"), None);
        for days in -800_000..800_000 {
            assert_eq!(Date::from_days(days).days(), days);
        }
        assert_eq!(
            format_time(86400.0 * 59.0 + 3600.0 * 5.0 + 60.0 * 12.5),
            "2026-03-01 05:12"
        );
    }

    #[test]
    fn poles_point_the_right_way() {
        // The equatorial north pole is tilted from the ecliptic one by the obliquity
        let earth = from_equatorial(0.0, 90.0);
        assert!((earth.z - OBLIQUITY_DEGREES.to_radians().cos()).abs() < 1e-15);
        // The vernal equinox direction is shared by both frames
        assert!((from_equatorial(0.0, 0.0) - DVec3::X).length() < 1e-15);
        // Uranus is tipped over: 97.8 degrees, but the IAU's north pole is the one on the
        // north side of the ecliptic, so it spins backwards around it
        let tilt = |name| oblateness(name).unwrap().pole.z.acos().to_degrees();
        assert!(
            (tilt("Uranus") - (180.0 - 97.8)).abs() < 1.0,
            "{}",
            tilt("Uranus")
        );
        assert!((tilt("Jupiter") - 3.1).abs() < 1.5, "{}", tilt("Jupiter"));
        assert!((tilt("Saturn") - 26.7).abs() < 3.0, "{}", tilt("Saturn"));
        for known in OBLATENESS {
            assert!(KNOWN_BODIES.iter().any(|b| b.name == known.0));
        }
    }

    #[test]
    fn every_known_body_has_a_state() {
        let bodies = solar_system();
        assert_eq!(bodies.len(), KNOWN_BODIES.len());
        // The barycenter should be close to the origin
        let total_mass: f64 = bodies.mass.iter().sum();
        let barycenter: DVec3 = bodies
            .position
            .iter()
            .zip(&bodies.mass)
            .map(|(p, m)| *p * *m)
            .sum::<DVec3>()
            / total_mass;
        assert!(barycenter.length() < 1.0E6, "{barycenter}");
    }
}
