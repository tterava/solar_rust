//! Which bodies shine, how brightly and in what color, from their mass and radius unless their
//! [`Emission`] says otherwise. Nothing here is special to the Sun: bodies are sorted into kinds by
//! mass and radius, the way stellar astronomy does, so stars added to the simulation light up their
//! neighbors too, and a neutron star or black hole looks like one.
//!
//! - Kind: from mass and radius, see [`classify`].
//! - Temperature: given, typical for the kind, or for main-sequence stars from the luminosity and
//!   radius (Stefan-Boltzmann).
//! - Luminosity: given, from the mass-luminosity relation for main-sequence stars, or from the
//!   temperature and radius.
//! - Brightness: the part of the light that's visible (380 to 750 nm), so a million-degree neutron
//!   star, which shines in X-rays, looks faint.
//! - Color: the blackbody color at that temperature, relative to sunlight, so the Sun's light is
//!   white and cooler stars are redder.

use std::f64::consts::PI;

use solar_rust::astronomy::Emission;
use solar_rust::forces::G;

/// The IAU 2015 nominal solar values (resolution B3)
pub const SOLAR_LUMINOSITY: f64 = 3.828E26;
const SOLAR_MASS: f64 = 1.3271244E20 / G;
const SOLAR_RADIUS: f64 = 6.957E8;
const SOLAR_TEMPERATURE: f64 = 5772.0;
/// W / (m^2 K^4)
const STEFAN_BOLTZMANN: f64 = 5.670374419E-8;
const SPEED_OF_LIGHT: f64 = 299_792_458.0;
const PLANCK: f64 = 6.62607015E-34;
const BOLTZMANN: f64 = 1.380649E-23;

/// What a body is, as far as its light goes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Planet,
    /// 13 to 75 Jupiter masses: too light to fuse hydrogen, glowing dull red as it cools
    BrownDwarf,
    /// Fusing hydrogen in its core
    MainSequence,
    /// Swollen to well beyond its main-sequence size
    Giant,
    /// A Sun's mass in an Earth's size, glowing from leftover heat
    WhiteDwarf,
    /// Over a Sun's mass in a city's size: hot, but tiny and shining in X-rays
    NeutronStar,
    /// Inside its photon sphere, where nothing can hold itself up: gives off no light
    BlackHole,
}

/// Sorts a body into a kind by its mass (kg) and radius (m):
///
/// | Kind | Mass | Radius |
/// |---|---|---|
/// | Black hole | any | under 1.5 Schwarzschild radii |
/// | Neutron star | over 0.1 M☉ | under 100 km |
/// | White dwarf | over 0.1 M☉ | under 0.05 R☉ (35,000 km) |
/// | Giant | over 0.075 M☉ | over 4 main-sequence radii |
/// | Main sequence | over 0.075 M☉ (the hydrogen burning limit) | |
/// | Brown dwarf | 0.012 to 0.075 M☉ (13 to 78 Jupiter masses) | |
/// | Planet | the rest | |
pub fn classify(mass: f64, radius: f64) -> Kind {
    let schwarzschild = 2.0 * G * mass / (SPEED_OF_LIGHT * SPEED_OF_LIGHT);
    let solar_masses = mass / SOLAR_MASS;
    if radius < 1.5 * schwarzschild {
        Kind::BlackHole
    } else if solar_masses >= 0.1 && radius < 100.0E3 {
        Kind::NeutronStar
    } else if solar_masses >= 0.1 && radius < 0.05 * SOLAR_RADIUS {
        Kind::WhiteDwarf
    } else if solar_masses >= 0.075 {
        if radius > 4.0 * main_sequence_radius(solar_masses) * SOLAR_RADIUS {
            Kind::Giant
        } else {
            Kind::MainSequence
        }
    } else if solar_masses >= 0.012 {
        Kind::BrownDwarf
    } else {
        Kind::Planet
    }
}

impl Kind {
    /// A typical surface temperature (K), for kinds whose luminosity doesn't follow from their
    /// mass. Real ones span a wide range with age; [`Emission`] can say otherwise.
    fn typical_temperature(self) -> Option<f64> {
        match self {
            Kind::BrownDwarf => Some(1500.0),
            // On the red giant branch
            Kind::Giant => Some(4500.0),
            Kind::WhiteDwarf => Some(10_000.0),
            // A young one, as X-ray telescopes see them
            Kind::NeutronStar => Some(1.0E6),
            Kind::MainSequence | Kind::Planet | Kind::BlackHole => None,
        }
    }
}

/// A body that shines
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shine {
    pub kind: Kind,
    /// W
    pub luminosity: f64,
    /// K
    pub temperature: f64,
    /// Compared to the Sun's visible light
    pub brightness: f64,
}

/// How a body with this mass (kg) and radius (m) shines, if it does
pub fn shine(mass: f64, radius: f64, emission: &Emission) -> Option<Shine> {
    let kind = classify(mass, radius);
    let surface = 4.0 * PI * radius * radius;
    let from_temperature = |t: f64| surface * STEFAN_BOLTZMANN * t.powi(4);
    let (luminosity, temperature) = match (emission.luminosity, emission.temperature) {
        (Some(luminosity), Some(temperature)) => (luminosity, temperature),
        (Some(luminosity), None) => (luminosity, temperature_of(luminosity, surface)),
        (None, Some(temperature)) => (from_temperature(temperature), temperature),
        (None, None) => match kind {
            Kind::MainSequence => {
                let luminosity = main_sequence_luminosity(mass / SOLAR_MASS) * SOLAR_LUMINOSITY;
                (luminosity, temperature_of(luminosity, surface))
            }
            _ => {
                let temperature = kind.typical_temperature()?;
                (from_temperature(temperature), temperature)
            }
        },
    };
    let visible = |l: f64, t: f64| l * visible_fraction(t);
    Some(Shine {
        kind,
        luminosity,
        temperature,
        brightness: visible(luminosity, temperature) / visible(SOLAR_LUMINOSITY, SOLAR_TEMPERATURE),
    })
}

fn temperature_of(luminosity: f64, surface: f64) -> f64 {
    (luminosity / (surface * STEFAN_BOLTZMANN)).powf(0.25)
}

/// Luminosity of a main-sequence star in solar units, from its mass in solar masses (Salaris &
/// Cassisi 2005, Duric 2004)
fn main_sequence_luminosity(mass: f64) -> f64 {
    if mass < 0.43 {
        0.23 * mass.powf(2.3)
    } else if mass < 2.0 {
        mass.powi(4)
    } else if mass < 55.0 {
        1.4 * mass.powf(3.5)
    } else {
        32000.0 * mass
    }
}

/// Radius of a main-sequence star in solar radii, from its mass in solar masses (Demircan &
/// Kahraman 1991)
fn main_sequence_radius(mass: f64) -> f64 {
    if mass < 1.66 {
        1.06 * mass.powf(0.945)
    } else {
        1.33 * mass.powf(0.555)
    }
}

/// The fraction of a blackbody's light at `temperature` (K) that's visible, 380 to 750 nm: the
/// Planck spectrum integrated over that band (Simpson's rule), over its total, σT⁴/π
fn visible_fraction(temperature: f64) -> f64 {
    const STEPS: usize = 64;
    let (from, to) = (380.0E-9, 750.0E-9);
    let planck = |wavelength: f64| {
        let x = PLANCK * SPEED_OF_LIGHT / (wavelength * BOLTZMANN * temperature);
        2.0 * PLANCK * SPEED_OF_LIGHT * SPEED_OF_LIGHT / wavelength.powi(5) / x.exp_m1()
    };
    let h = (to - from) / STEPS as f64;
    let sum: f64 = (0..=STEPS)
        .map(|k| {
            let weight = if k == 0 || k == STEPS {
                1.0
            } else if k % 2 == 1 {
                4.0
            } else {
                2.0
            };
            weight * planck(from + k as f64 * h)
        })
        .sum();
    sum * h / 3.0 / (STEFAN_BOLTZMANN * temperature.powi(4) / PI)
}

/// Temperature (K) of a star with this B-V color index (Ballesteros 2012)
pub fn temperature_from_color_index(bv: f64) -> f64 {
    4600.0 * (1.0 / (0.92 * bv + 1.7) + 1.0 / (0.92 * bv + 0.62))
}

/// Linear RGB of a blackbody at `temperature` (K), relative to the Sun's, with the brightest
/// channel 1
pub fn color(temperature: f64) -> [f32; 3] {
    let [r, g, b] = blackbody_srgb(temperature);
    let [sr, sg, sb] = blackbody_srgb(SOLAR_TEMPERATURE);
    let relative = [r / sr, g / sg, b / sb];
    let max = relative.iter().copied().fold(f64::MIN, f64::max);
    relative.map(|c| (c / max) as f32)
}

/// Linear sRGB (not normalized) of a blackbody, from where it lies on the Planckian locus in
/// CIE xy (Kim et al. 2002, valid from 1667 K to 25000 K, and clamped to that)
fn blackbody_srgb(temperature: f64) -> [f64; 3] {
    let t = temperature.clamp(1667.0, 25000.0);
    let (t1, t2, t3) = (1.0E3 / t, 1.0E6 / (t * t), 1.0E9 / (t * t * t));
    let x = if t <= 4000.0 {
        -0.2661239 * t3 - 0.2343589 * t2 + 0.8776956 * t1 + 0.179910
    } else {
        -3.0258469 * t3 + 2.1070379 * t2 + 0.2226347 * t1 + 0.240390
    };
    let y = if t <= 2222.0 {
        -1.1063814 * x.powi(3) - 1.34811020 * x * x + 2.18555832 * x - 0.20219683
    } else if t <= 4000.0 {
        -0.9549476 * x.powi(3) - 1.37418593 * x * x + 2.09137015 * x - 0.16748867
    } else {
        3.0817580 * x.powi(3) - 5.87338670 * x * x + 3.75112997 * x - 0.37001483
    };
    let (big_x, big_y, big_z) = (x / y, 1.0, (1.0 - x - y) / y);
    [
        3.2406 * big_x - 1.5372 * big_y - 0.4986 * big_z,
        -0.9689 * big_x + 1.8758 * big_y + 0.0415 * big_z,
        0.0557 * big_x - 0.2040 * big_y + 1.0570 * big_z,
    ]
    .map(|c| c.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_rust::ephemeris::solar_system;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a / b - 1.0).abs() < tolerance
    }

    /// From its mass and radius alone, the Sun comes out as the Sun
    #[test]
    fn sun_from_its_mass_and_radius() {
        let bodies = solar_system();
        let sun = bodies.info.iter().position(|b| &*b.name == "Sun").unwrap();
        let computed = shine(bodies.mass[sun], bodies.radius[sun], &Emission::default()).unwrap();
        assert!(close(computed.luminosity, SOLAR_LUMINOSITY, 0.01));
        assert!(close(computed.temperature, SOLAR_TEMPERATURE, 0.005));

        // The ephemeris gives the nominal values, which win
        let given = shine(
            bodies.mass[sun],
            bodies.radius[sun],
            &bodies.info[sun].emission,
        );
        assert_eq!(given.unwrap().temperature, SOLAR_TEMPERATURE);
    }

    #[test]
    fn only_the_sun_shines_in_the_solar_system() {
        let bodies = solar_system();
        let shining: Vec<_> = (0..bodies.len())
            .filter(|&i| {
                let info = &bodies.info[i];
                shine(bodies.mass[i], bodies.radius[i], &info.emission).is_some()
            })
            .map(|i| &*bodies.info[i].name)
            .collect();
        assert_eq!(shining, ["Sun"]);
    }

    #[test]
    fn kinds() {
        const JUPITER: f64 = 1.898E27;
        let cases = [
            ("Sun", SOLAR_MASS, SOLAR_RADIUS, Kind::MainSequence),
            ("Jupiter", JUPITER, 6.99E7, Kind::Planet),
            ("Earth", 5.97E24, 6.37E6, Kind::Planet),
            (
                "Brown dwarf",
                50.0 * JUPITER,
                0.1 * SOLAR_RADIUS,
                Kind::BrownDwarf,
            ),
            (
                "Proxima Centauri",
                0.122 * SOLAR_MASS,
                0.154 * SOLAR_RADIUS,
                Kind::MainSequence,
            ),
            (
                "Sirius A",
                2.06 * SOLAR_MASS,
                1.71 * SOLAR_RADIUS,
                Kind::MainSequence,
            ),
            (
                "Betelgeuse",
                16.5 * SOLAR_MASS,
                764.0 * SOLAR_RADIUS,
                Kind::Giant,
            ),
            (
                "Sirius B",
                1.02 * SOLAR_MASS,
                0.0084 * SOLAR_RADIUS,
                Kind::WhiteDwarf,
            ),
            ("Neutron star", 1.4 * SOLAR_MASS, 12.0E3, Kind::NeutronStar),
            ("Black hole", 10.0 * SOLAR_MASS, 29.5E3, Kind::BlackHole),
        ];
        for (name, mass, radius, kind) in cases {
            assert_eq!(classify(mass, radius), kind, "{name}");
        }
    }

    #[test]
    fn remnants() {
        let none = Emission::default();
        assert!(shine(10.0 * SOLAR_MASS, 29.5E3, &none).is_none());

        // A neutron star gives off a quarter of the Sun's power, nearly all as X-rays
        let neutron = shine(1.4 * SOLAR_MASS, 12.0E3, &none).unwrap();
        assert!((0.1..0.5).contains(&(neutron.luminosity / SOLAR_LUMINOSITY)));
        assert!(neutron.brightness < 1e-5, "{}", neutron.brightness);

        // Sirius B: about 2.5% of the Sun's power at 25,000 K (Barstow et al. 2005)
        let given = Emission {
            luminosity: None,
            temperature: Some(25_000.0),
        };
        let sirius_b = shine(1.02 * SOLAR_MASS, 0.0084 * SOLAR_RADIUS, &given).unwrap();
        assert!(close(sirius_b.luminosity / SOLAR_LUMINOSITY, 0.026, 0.1));
    }

    #[test]
    fn visible_light() {
        // About 40% of sunlight is visible; far less of a cool red dwarf's or a hot star's
        let sun = visible_fraction(SOLAR_TEMPERATURE);
        assert!((0.35..0.5).contains(&sun), "{sun}");
        assert!(visible_fraction(3000.0) < 0.5 * sun);
        assert!(visible_fraction(30_000.0) < 0.5 * sun);
        let brown_dwarf = shine(0.05 * SOLAR_MASS, 0.1 * SOLAR_RADIUS, &Emission::default());
        assert!(brown_dwarf.unwrap().brightness < 1e-4);
    }

    #[test]
    fn red_dwarf() {
        let star = shine(0.3 * SOLAR_MASS, 0.3 * SOLAR_RADIUS, &Emission::default()).unwrap();
        assert!(close(star.luminosity / SOLAR_LUMINOSITY, 0.0145, 0.02));
        assert!(
            (3000.0..4000.0).contains(&star.temperature),
            "{}",
            star.temperature
        );
        let [r, g, b] = color(star.temperature);
        assert!(r == 1.0 && g < 0.9 && b < g, "{r} {g} {b}");
    }

    /// Either value can be given; the other follows from the radius
    #[test]
    fn overrides() {
        let hot = Emission {
            luminosity: None,
            temperature: Some(SOLAR_TEMPERATURE),
        };
        let star = shine(1.0, SOLAR_RADIUS, &hot).unwrap();
        assert!(close(star.luminosity, SOLAR_LUMINOSITY, 0.001));

        let bright = Emission {
            luminosity: Some(SOLAR_LUMINOSITY),
            temperature: None,
        };
        let star = shine(1.0, SOLAR_RADIUS, &bright).unwrap();
        assert!(close(star.temperature, SOLAR_TEMPERATURE, 0.001));
    }

    #[test]
    fn colors() {
        assert_eq!(color(SOLAR_TEMPERATURE), [1.0, 1.0, 1.0]);
        let [r, _, b] = color(3000.0);
        assert!(r == 1.0 && b < 0.5);
        let [r, _, b] = color(10000.0);
        assert!(b == 1.0 && r < 0.9);
        // The Sun's B-V is 0.65
        assert!((temperature_from_color_index(0.65) - SOLAR_TEMPERATURE).abs() < 150.0);
    }
}
