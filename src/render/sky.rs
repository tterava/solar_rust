//! The background stars: the Yale Bright Star Catalogue, every star brighter than magnitude 6.5,
//! drawn as points infinitely far away. Their colors come from their B-V color index through the
//! same blackbody colors as the simulation's own stars.

use solar_rust::ephemeris::from_equatorial;

use super::light;

const CATALOG: &str = include_str!("../../data/stars.csv");

/// One star for the sky shader
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkyStar {
    /// Unit vector in the ecliptic frame
    pub direction: [f32; 3],
    /// Radius of its glow in pixels
    pub size: f32,
    /// Linear RGB, brightness included
    pub color: [f32; 3],
}

/// Pixels across a star's glow at least, and how much more the brightest get
const MIN_SIZE: f32 = 1.3;
const SIZE_PER_BRIGHTNESS: f32 = 1.4;

/// The catalogue's stars, ready to draw
pub fn stars() -> Vec<SkyStar> {
    CATALOG
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("ra_deg"))
        .filter_map(|line| {
            let mut fields = line.split(',').map(|f| f.trim().parse::<f64>());
            let (ra, dec, vmag, bv) = (
                fields.next()?.ok()?,
                fields.next()?.ok()?,
                fields.next()?.ok()?,
                fields.next()?.ok()?,
            );
            let (size, brightness) = appearance(vmag);
            let color = light::color(light::temperature_from_color_index(bv));
            Some(SkyStar {
                direction: from_equatorial(ra, dec).as_vec3().into(),
                size,
                color: color.map(|c| c * brightness),
            })
        })
        .collect()
}

/// Size (pixels) and brightness (0 to 1) of a star of visual magnitude `vmag`. The eye's response
/// is far from linear, so both follow the square root of the star's flux: magnitude 0 is full
/// brightness, and the faintest (6.5) are 1/20 of it.
fn appearance(vmag: f64) -> (f32, f32) {
    let root_flux = 10f64.powf(-0.2 * vmag) as f32;
    let size = MIN_SIZE + SIZE_PER_BRIGHTNESS * (root_flux - 0.2).max(0.0);
    (size, root_flux.min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog() {
        let stars = stars();
        assert!(stars.len() > 9000, "{}", stars.len());
        // Sirius is the brightest, and white-blue
        let brightest = stars
            .iter()
            .max_by(|a, b| a.size.total_cmp(&b.size))
            .unwrap();
        let sirius = from_equatorial(101.287, -16.716).as_vec3();
        assert!(glam::Vec3::from(brightest.direction).dot(sirius) > 0.9999);
        let [r, _, b] = brightest.color;
        assert!(b >= r);
    }

    #[test]
    fn fainter_is_smaller_and_dimmer() {
        let (bright_size, bright) = appearance(0.0);
        let (faint_size, faint) = appearance(6.5);
        assert_eq!(bright, 1.0);
        assert!((faint - 0.05).abs() < 0.001);
        assert!(faint_size < bright_size);
    }
}
