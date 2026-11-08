//! Which way each body faces: the IAU rotation models, for drawing textured bodies. Visual only,
//! the simulation doesn't model rotation.
//!
//! The values are from the IAU WGCCRE 2015 report (Archinal et al. 2018) as NAIF publishes it in
//! `pck00011.tpc`. A model gives the north pole's right ascension α₀ and declination δ₀ (ICRF), and
//! the prime meridian's angle W, measured from where the body's equator crosses the ICRF equator:
//!
//! - α₀ = α₀₀ + α₀₁·T + Σ aᵢ·sin θᵢ
//! - δ₀ = δ₀₀ + δ₀₁·T + Σ dᵢ·cos θᵢ
//! - W = W₀ + W₁·d + W₂·d² + Σ wᵢ·sin θᵢ
//!
//! with T in Julian centuries and d in days since J2000 TDB. The angles θᵢ are shared by a planet's
//! system: θᵢ = θᵢ₀ + θᵢ₁·T + θᵢ₂·T².
//!
//! Left out: Mercury's tiny libration (0.01°). The Moon's model is the mean Earth / polar axis
//! frame that lunar maps use. Bodies without a model aren't rotated, which only matters for
//! textured bodies.

// The tables are copied digit for digit from the source, and Triton's -6.28 isn't 2π
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use glam::DMat3;

use solar_rust::ephemeris::{equatorial_to_ecliptic, EPOCH};

/// Julian date of J2000 (2000-01-01 12:00 TDB)
const J2000: f64 = 2451545.0;
const DAYS_PER_CENTURY: f64 = 36525.0;

struct Model {
    name: &'static str,
    /// α₀₀ (degrees), α₀₁ (degrees per century)
    ra: [f64; 2],
    /// δ₀₀, δ₀₁
    dec: [f64; 2],
    /// W₀ (degrees), W₁ (degrees per day), W₂ (degrees per day²)
    pm: [f64; 3],
    /// The angles θᵢ of the body's system, (degrees, per century, per century²)
    angles: &'static [[f64; 3]],
    /// aᵢ, dᵢ and wᵢ, in the order of `angles`. Missing ones are 0.
    nut_ra: &'static [f64],
    nut_dec: &'static [f64],
    nut_pm: &'static [f64],
}

const fn simple(name: &'static str, ra: [f64; 2], dec: [f64; 2], pm: [f64; 3]) -> Model {
    Model {
        name,
        ra,
        dec,
        pm,
        angles: &[],
        nut_ra: &[],
        nut_dec: &[],
        nut_pm: &[],
    }
}

/// The Moon's angles E1 to E13
#[rustfmt::skip]
const EARTH_ANGLES: &[[f64; 3]] = &[
    [125.045, -1935.5364525, 0.0], [250.089, -3871.072905, 0.0],
    [260.008, 475263.3328725, 0.0], [176.625, 487269.629985, 0.0],
    [357.529, 35999.0509575, 0.0], [311.589, 964468.49931, 0.0],
    [134.963, 477198.869325, 0.0], [276.617, 12006.300765, 0.0],
    [34.226, 63863.5132425, 0.0], [15.134, -5806.6093575, 0.0],
    [119.743, 131.84064, 0.0], [239.961, 6003.1503825, 0.0],
    [25.053, 473327.79642, 0.0],
];

#[rustfmt::skip]
const MARS_ANGLES: &[[f64; 3]] = &[
    // Phobos
    [190.72646643, 15917.10818695, 0.0], [21.46892470, 31834.27934054, 0.0],
    [332.86082793, 19139.89694742, 0.0], [394.93256437, 38280.79631835, 0.0],
    [189.63271560, 41215158.18420050, 12.711923222],
    // Deimos
    [121.46893664, 660.22803474, 0.0], [231.05028581, 660.99123540, 0.0],
    [251.37314025, 1320.50145245, 0.0], [217.98635955, 38279.96125550, 0.0],
    [196.19729402, 19139.83628608, 0.0],
    // Mars
    [198.991226, 19139.4819985, 0.0], [226.292679, 38280.8511281, 0.0],
    [249.663391, 57420.7251593, 0.0], [266.183510, 76560.6367950, 0.0],
    [79.398797, 0.5042615, 0.0],
    [122.433576, 19139.9407476, 0.0], [43.058401, 38280.8753272, 0.0],
    [57.663379, 57420.7517205, 0.0], [79.476401, 76560.6495004, 0.0],
    [166.325722, 0.5042615, 0.0],
    [129.071773, 19140.0328244, 0.0], [36.352167, 38281.0473591, 0.0],
    [56.668646, 57420.9295360, 0.0], [67.364003, 76560.2552215, 0.0],
    [104.792680, 95700.4387578, 0.0], [95.391654, 0.5042615, 0.0],
];

/// J1 to J8 for the Galilean moons, then Ja to Je for Jupiter
#[rustfmt::skip]
const JUPITER_ANGLES: &[[f64; 3]] = &[
    [73.32, 91472.9, 0.0], [24.62, 45137.2, 0.0], [283.90, 4850.7, 0.0],
    [355.80, 1191.3, 0.0], [119.90, 262.1, 0.0], [229.80, 64.3, 0.0],
    [352.25, 2382.6, 0.0], [113.35, 6070.0, 0.0], [146.64, 182945.8, 0.0],
    [49.24, 90274.4, 0.0],
    [99.360714, 4850.4046, 0.0], [175.895369, 1191.9605, 0.0],
    [300.323162, 262.5475, 0.0], [114.012305, 6070.2476, 0.0],
    [49.511251, 64.3000, 0.0],
];

#[rustfmt::skip]
const SATURN_ANGLES: &[[f64; 3]] = &[
    [353.32, 75706.7, 0.0], [28.72, 75706.7, 0.0], [177.40, -36505.5, 0.0],
    [300.00, -7225.9, 0.0], [316.45, 506.2, 0.0], [345.20, -1016.3, 0.0],
    [706.64, 151413.4, 0.0], [57.44, 151413.4, 0.0],
];

#[rustfmt::skip]
const URANUS_ANGLES: &[[f64; 3]] = &[
    [115.75, 54991.87, 0.0], [141.69, 41887.66, 0.0], [135.03, 29927.35, 0.0],
    [61.77, 25733.59, 0.0], [249.32, 24471.46, 0.0], [43.86, 22278.41, 0.0],
    [77.66, 20289.42, 0.0], [157.36, 16652.76, 0.0], [101.81, 12872.63, 0.0],
    [138.64, 8061.81, 0.0], [102.23, -2024.22, 0.0], [316.41, 2863.96, 0.0],
    [304.01, -51.94, 0.0], [308.71, -93.17, 0.0], [340.82, -75.32, 0.0],
    [259.14, -504.81, 0.0], [204.46, -4048.44, 0.0], [632.82, 5727.92, 0.0],
];

/// N, N1 to N7, 2·N1, then 2·N7 to 9·N7 for Triton
#[rustfmt::skip]
const NEPTUNE_ANGLES: &[[f64; 3]] = &[
    [357.85, 52.316, 0.0], [323.92, 62606.6, 0.0], [220.51, 55064.2, 0.0],
    [354.27, 46564.5, 0.0], [75.31, 26109.4, 0.0], [35.36, 14325.4, 0.0],
    [142.61, 2824.6, 0.0], [177.85, 52.316, 0.0], [647.840, 125213.200, 0.0],
    [355.700, 104.632, 0.0], [533.550, 156.948, 0.0], [711.400, 209.264, 0.0],
    [889.250, 261.580, 0.0], [1067.100, 313.896, 0.0], [1244.950, 366.212, 0.0],
    [1422.800, 418.528, 0.0], [1600.650, 470.844, 0.0],
];

const SATURN_POLE: ([f64; 2], [f64; 2]) = ([40.66, -0.036], [83.52, -0.004]);
const URANUS_MOON_POLE: [f64; 2] = [257.43, 0.0];

#[rustfmt::skip]
const MODELS: &[Model] = &[
    simple("Sun", [286.13, 0.0], [63.87, 0.0], [84.176, 14.1844, 0.0]),
    simple("Mercury", [281.0103, -0.0328], [61.4155, -0.0049], [329.5988, 6.1385108, 0.0]),
    simple("Venus", [272.76, 0.0], [67.16, 0.0], [160.20, -1.4813688, 0.0]),
    simple("Earth", [0.0, -0.641], [90.0, -0.557], [190.147, 360.9856235, 0.0]),
    Model {
        name: "Moon",
        ra: [269.9949, 0.0031],
        dec: [66.5392, 0.0130],
        pm: [38.3213, 13.17635815, -1.4E-12],
        angles: EARTH_ANGLES,
        nut_ra: &[-3.8787, -0.1204, 0.0700, -0.0172, 0.0, 0.0072, 0.0, 0.0, 0.0, -0.0052, 0.0,
            0.0, 0.0043],
        nut_dec: &[1.5419, 0.0239, -0.0278, 0.0068, 0.0, -0.0029, 0.0009, 0.0, 0.0, 0.0008, 0.0,
            0.0, -0.0009],
        nut_pm: &[3.5610, 0.1208, -0.0642, 0.0158, 0.0252, -0.0066, -0.0047, -0.0046, 0.0028,
            0.0052, 0.0040, 0.0019, -0.0044],
    },
    Model {
        name: "Mars",
        ra: [317.269202, -0.10927547],
        dec: [54.432516, -0.05827105],
        pm: [176.049863, 350.891982443297, 0.0],
        angles: MARS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.000068, 0.000238, 0.000052, 0.000009, 0.419057],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.000051, 0.000141, 0.000031, 0.000005, 1.591274],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0,
            0.000145, 0.000157, 0.000040, 0.000001, 0.000001, 0.584542],
    },
    Model {
        name: "Phobos",
        ra: [317.67071657, -0.10844326],
        dec: [52.88627266, -0.06134706],
        pm: [35.18774440, 1128.84475928, 9.536137031212154E-9],
        angles: MARS_ANGLES,
        nut_ra: &[-1.78428399, 0.02212824, -0.01028251, -0.00475595],
        nut_dec: &[-1.07516537, 0.00668626, -0.00648740, 0.00281576],
        nut_pm: &[1.42421769, -0.02273783, 0.00410711, 0.00631964, -1.143],
    },
    Model {
        name: "Deimos",
        ra: [316.65705808, -0.10518014],
        dec: [53.50992033, -0.05979094],
        pm: [79.39932954, 285.16188899, 0.0],
        angles: MARS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0,
            3.09217726, 0.22980637, 0.06418655, 0.02533537, 0.00778695],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0,
            1.83936004, 0.14325320, 0.01911409, -0.01482590, 0.00192430],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0,
            -2.73954829, -0.39968606, -0.06563259, -0.02912940, 0.01699160],
    },
    Model {
        name: "Jupiter",
        ra: [268.056595, -0.006499],
        dec: [64.495303, 0.002413],
        pm: [284.95, 870.5360000, 0.0],
        angles: JUPITER_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.000117, 0.000938, 0.001432, 0.000030, 0.002150],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.000050, 0.000404, 0.000617, -0.000013, 0.000926],
        nut_pm: &[],
    },
    Model {
        name: "Io",
        ra: [268.05, -0.009],
        dec: [64.50, 0.003],
        pm: [200.39, 203.4889538, 0.0],
        angles: JUPITER_ANGLES,
        nut_ra: &[0.0, 0.0, 0.094, 0.024],
        nut_dec: &[0.0, 0.0, 0.040, 0.011],
        nut_pm: &[0.0, 0.0, -0.085, -0.022],
    },
    Model {
        name: "Europa",
        ra: [268.08, -0.009],
        dec: [64.51, 0.003],
        pm: [36.022, 101.3747235, 0.0],
        angles: JUPITER_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 1.086, 0.060, 0.015, 0.009],
        nut_dec: &[0.0, 0.0, 0.0, 0.468, 0.026, 0.007, 0.002],
        nut_pm: &[0.0, 0.0, 0.0, -0.980, -0.054, -0.014, -0.008],
    },
    Model {
        name: "Ganymede",
        ra: [268.20, -0.009],
        dec: [64.57, 0.003],
        pm: [44.064, 50.3176081, 0.0],
        angles: JUPITER_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, -0.037, 0.431, 0.091],
        nut_dec: &[0.0, 0.0, 0.0, -0.016, 0.186, 0.039],
        nut_pm: &[0.0, 0.0, 0.0, 0.033, -0.389, -0.082],
    },
    Model {
        name: "Callisto",
        ra: [268.72, -0.009],
        dec: [64.83, 0.003],
        pm: [259.51, 21.5710715, 0.0],
        angles: JUPITER_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, -0.068, 0.590, 0.0, 0.010],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, -0.029, 0.254, 0.0, -0.004],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.061, -0.533, 0.0, -0.009],
    },
    simple("Saturn", [40.589, -0.036], [83.537, -0.004], [38.90, 810.7939024, 0.0]),
    Model {
        name: "Mimas",
        ra: SATURN_POLE.0,
        dec: SATURN_POLE.1,
        pm: [333.46, 381.9945550, 0.0],
        angles: SATURN_ANGLES,
        nut_ra: &[0.0, 0.0, 13.56],
        nut_dec: &[0.0, 0.0, -1.53],
        nut_pm: &[0.0, 0.0, -13.48, 0.0, -44.85],
    },
    simple("Enceladus", SATURN_POLE.0, SATURN_POLE.1, [6.32, 262.7318996, 0.0]),
    Model {
        name: "Tethys",
        ra: SATURN_POLE.0,
        dec: SATURN_POLE.1,
        pm: [8.95, 190.6979085, 0.0],
        angles: SATURN_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 9.66],
        nut_dec: &[0.0, 0.0, 0.0, -1.09],
        nut_pm: &[0.0, 0.0, 0.0, -9.60, 2.23],
    },
    simple("Dione", SATURN_POLE.0, SATURN_POLE.1, [357.6, 131.5349316, 0.0]),
    Model {
        name: "Rhea",
        ra: [40.38, -0.036],
        dec: [83.55, -0.004],
        pm: [235.16, 79.6900478, 0.0],
        angles: SATURN_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 3.10],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, -0.35],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, -3.08],
    },
    simple("Titan", [39.4827, 0.0], [83.4279, 0.0], [186.5855, 22.5769768, 0.0]),
    simple("Iapetus", [318.16, -3.949], [75.03, -1.143], [355.2, 4.5379572, 0.0]),
    simple("Uranus", [257.311, 0.0], [-15.175, 0.0], [203.81, -501.1600928, 0.0]),
    Model {
        name: "Miranda",
        ra: URANUS_MOON_POLE,
        dec: [-15.08, 0.0],
        pm: [30.70, -254.6906892, 0.0],
        angles: URANUS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            4.41, 0.0, 0.0, 0.0, 0.0, 0.0, -0.04, 0.0],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            4.25, 0.0, 0.0, 0.0, 0.0, 0.0, -0.02, 0.0],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            1.15, -1.27, 0.0, 0.0, 0.0, 0.0, -0.09, 0.15],
    },
    Model {
        name: "Ariel",
        ra: URANUS_MOON_POLE,
        dec: [-15.10, 0.0],
        pm: [156.22, -142.8356681, 0.0],
        angles: URANUS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.29],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.28],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.05, 0.08],
    },
    Model {
        name: "Umbriel",
        ra: URANUS_MOON_POLE,
        dec: [-15.10, 0.0],
        pm: [108.05, -86.8688923, 0.0],
        angles: URANUS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.21],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.20],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.09, 0.0, 0.06],
    },
    Model {
        name: "Titania",
        ra: URANUS_MOON_POLE,
        dec: [-15.10, 0.0],
        pm: [77.74, -41.3514316, 0.0],
        angles: URANUS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.29],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.28],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.08],
    },
    Model {
        name: "Oberon",
        ra: URANUS_MOON_POLE,
        dec: [-15.10, 0.0],
        pm: [6.77, -26.7394932, 0.0],
        angles: URANUS_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.16],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.16],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.04],
    },
    Model {
        name: "Neptune",
        ra: [299.36, 0.0],
        dec: [43.46, 0.0],
        pm: [249.978, 541.1397757, 0.0],
        angles: NEPTUNE_ANGLES,
        nut_ra: &[0.70],
        nut_dec: &[-0.51],
        nut_pm: &[-0.48],
    },
    Model {
        name: "Triton",
        ra: [299.36, 0.0],
        dec: [41.17, 0.0],
        pm: [296.53, -61.2572637, 0.0],
        angles: NEPTUNE_ANGLES,
        nut_ra: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -32.35, 0.0, -6.28, -2.08, -0.74, -0.28,
            -0.11, -0.07, -0.02, -0.01],
        nut_dec: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 22.55, 0.0, 2.10, 0.55, 0.16, 0.05, 0.02,
            0.01, 0.0, 0.0],
        nut_pm: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 22.25, 0.0, 6.73, 2.05, 0.74, 0.28, 0.11,
            0.05, 0.02, 0.01],
    },
    simple("Pluto", [132.993, 0.0], [-6.163, 0.0], [302.695, 56.3625225, 0.0]),
    simple("Charon", [132.993, 0.0], [-6.163, 0.0], [122.695, 56.3625225, 0.0]),
    simple("Ceres", [291.418, 0.0], [66.764, 0.0], [170.650, 952.1532, 0.0]),
    simple("Vesta", [309.031, 0.0], [42.235, 0.0], [285.39, 1617.3329428, 0.0]),
];

/// Days since J2000 TDB, `seconds` after the start of the simulation's epoch
pub fn days_since_j2000(seconds: f64) -> f64 {
    EPOCH.julian_date() - J2000 + seconds / 86400.0
}

/// Pole right ascension and declination, and prime meridian angle, all in degrees
fn angles(model: &Model, d: f64) -> (f64, f64, f64) {
    let t = d / DAYS_PER_CENTURY;
    let (mut ra, mut dec) = (
        model.ra[0] + model.ra[1] * t,
        model.dec[0] + model.dec[1] * t,
    );
    let mut pm = model.pm[0] + model.pm[1] * d + model.pm[2] * d * d;
    for (i, [a0, a1, a2]) in model.angles.iter().enumerate() {
        let (sin, cos) = (a0 + a1 * t + a2 * t * t).to_radians().sin_cos();
        let coefficient = |terms: &[f64]| terms.get(i).copied().unwrap_or(0.0);
        ra += coefficient(model.nut_ra) * sin;
        dec += coefficient(model.nut_dec) * cos;
        pm += coefficient(model.nut_pm) * sin;
    }
    (ra, dec, pm)
}

/// Rotation from the body-fixed frame to the ecliptic frame, `d` days after J2000 TDB. The
/// body-fixed frame has x towards the prime meridian on the equator, y towards 90° east and z
/// towards the north pole. `None` for bodies without a model.
pub fn orientation(name: &str, d: f64) -> Option<DMat3> {
    let model = MODELS.iter().find(|m| m.name == name)?;
    let (ra, dec, pm) = angles(model, d);
    let body_to_equatorial = DMat3::from_rotation_z((ra + 90.0).to_radians())
        * DMat3::from_rotation_x((90.0 - dec).to_radians())
        * DMat3::from_rotation_z((pm % 360.0).to_radians());
    Some(equatorial_to_ecliptic() * body_to_equatorial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use solar_rust::ephemeris::{from_equatorial, parse_states, Date};

    /// East longitude and latitude (degrees) where `direction` (ecliptic frame) points, in a body
    /// frame given by `orientation`
    fn longitude_latitude(orientation: &DMat3, direction: DVec3) -> (f64, f64) {
        let local = orientation.transpose() * direction.normalize();
        (
            local.y.atan2(local.x).to_degrees(),
            local.z.clamp(-1.0, 1.0).asin().to_degrees(),
        )
    }

    /// Horizons states at 00:00 TDB on these dates, from `data/`
    const STATES: &[(&str, &str)] = &[
        (
            "2026-01-01",
            include_str!("../../data/horizons_2026-01-01.csv"),
        ),
        (
            "2026-02-01",
            include_str!("../../data/horizons_2026-02-01.csv"),
        ),
        (
            "2027-01-01",
            include_str!("../../data/horizons_2027-01-01.csv"),
        ),
        (
            "2036-01-01",
            include_str!("../../data/horizons_2036-01-01.csv"),
        ),
    ];

    /// Days since J2000, and the direction from one body to another (Horizons ids)
    fn direction(date: &str, csv: &str, from: &str, to: &str) -> (f64, DVec3) {
        let states = parse_states(csv);
        let position = |id| states.iter().find(|(i, _)| *i == id).unwrap().1.position;
        let d = Date::parse(date).unwrap().julian_date() - J2000;
        (d, position(to) - position(from))
    }

    /// At 00:00 TDB it's midnight in Greenwich, so the Sun is over the Pacific near 180°: off by
    /// the equation of time (up to 4°), and 0.3° because UT1 is about 69 s behind TDB.
    #[test]
    fn earth_midnight_at_greenwich() {
        // Degrees east of 180°, from the equation of time on that date and the 0.3°
        let expected = [1.1, 3.6, 1.1, 1.1];
        for (&(date, csv), expected) in STATES.iter().zip(expected) {
            let (d, sun) = direction(date, csv, "399", "10");
            let earth = orientation("Earth", d).unwrap();
            let (longitude, latitude) = longitude_latitude(&earth, sun);
            let east_of_antimeridian = longitude.rem_euclid(360.0) - 180.0;
            assert!(
                (east_of_antimeridian - expected).abs() < 0.5,
                "{date}: Sun over {longitude}°"
            );
            // Southern summer: the Sun is south of the equator
            assert!((-24.0..-16.0).contains(&latitude), "{date}: {latitude}");
        }
    }

    /// The Earth turns once a sidereal day, 23 h 56 min 4 s
    #[test]
    fn earth_sidereal_day() {
        let start = orientation("Earth", 0.0).unwrap();
        let later = orientation("Earth", 86164.0905 / 86400.0).unwrap();
        let (longitude, _) = longitude_latitude(&start, later * DVec3::X);
        assert!(longitude.abs() < 1e-3, "{longitude}");
    }

    /// The Moon keeps its near side towards the Earth, give or take its libration of up to 8° in
    /// longitude and 7° in latitude
    #[test]
    fn moon_faces_the_earth() {
        for &(date, csv) in STATES {
            let (d, earth) = direction(date, csv, "301", "399");
            let moon = orientation("Moon", d).unwrap();
            let (longitude, latitude) = longitude_latitude(&moon, earth);
            assert!(longitude.abs() < 8.5, "{date}: {longitude}");
            assert!(latitude.abs() < 7.5, "{date}: {latitude}");
        }
    }

    /// The rotation poles agree with the poles the simulation uses for oblateness
    #[test]
    fn poles_match_the_simulation() {
        let bodies = solar_rust::ephemeris::solar_system();
        let d = days_since_j2000(0.0);
        let mut checked = 0;
        for (info, oblateness) in bodies.info.iter().zip(&bodies.oblateness) {
            let Some(oblateness) = oblateness else {
                continue;
            };
            let model = orientation(&info.name, d).unwrap();
            let angle = model.z_axis.angle_between(oblateness.pole).to_degrees();
            assert!(angle < 0.02, "{}: poles {angle}° apart", info.name);
            checked += 1;
        }
        assert_eq!(checked, 7);
    }

    #[test]
    fn frames_are_rotations() {
        let d = days_since_j2000(1.0E6);
        for model in MODELS {
            let m = orientation(model.name, d).unwrap();
            assert!((m.determinant() - 1.0).abs() < 1e-12, "{}", model.name);
            let identity = m.transpose() * m;
            assert!(
                identity.abs_diff_eq(DMat3::IDENTITY, 1e-12),
                "{}",
                model.name
            );
            // The equator's ascending node on the ICRF equator is 90° east of the pole's RA
            let (ra, dec, _) = angles(model, d);
            let pole = from_equatorial(ra, dec);
            assert!(m.z_axis.angle_between(pole) < 1e-9, "{}", model.name);
        }
    }
}
