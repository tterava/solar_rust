//! What to draw this frame, worked out on the CPU from the simulation's snapshot and the camera.
//! Reads the bodies and never changes them.

use std::sync::Arc;

use egui::{Pos2, Rect};
use glam::{DMat3, DVec3, Mat4, Vec4};

use solar_rust::astronomy::Bodies;

use super::rotation;
use super::textures::srgb_to_linear;
use crate::camera::Camera;
use crate::ui::Target;

/// Bodies smaller than this on screen (points) are magnified, up to this size
const MAX_MAGNIFIED_RADIUS: f64 = 10.0;
/// Nothing is drawn smaller than this (points)
const MIN_RADIUS: f64 = 3.0;
/// Near plane distance in meters. With reversed-Z and a floating point depth buffer the depth
/// precision is about the same at every distance, so this can be tiny.
pub const NEAR: f32 = 1.0;

/// One body for the sphere shader, positioned relative to the camera in the view frame
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    /// Meters from the camera
    pub center: [f32; 3],
    /// Drawn radius in meters, which includes any magnification
    pub radius: f32,
    /// Linear RGB
    pub color: [f32; 3],
    /// 1 for bodies that shine on their own (the Sun), 0 for lit ones
    pub emissive: f32,
    /// Unit vector towards the Sun
    pub light: [f32; 3],
    /// 1 if the body's texture is drawn instead of its color. Set by the renderer, which knows
    /// which textures have loaded.
    pub textured: f32,
    /// The body-fixed axes in the view frame: towards the prime meridian on the equator, towards
    /// 90° east, and towards the north pole
    pub axes: [[f32; 3]; 3],
}

/// Per-frame values shared by every instance
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub projection: [[f32; 4]; 4],
    pub near: f32,
    /// The size of a pixel at distance 1, for smoothing outlines
    pub pixel: f32,
    pub _padding: [f32; 2],
}

#[derive(Default)]
pub struct Scene {
    pub instances: Vec<Instance>,
    /// The body each instance is, for finding its texture
    pub names: Vec<Arc<str>>,
    pub projection: Mat4,
}

/// The uniforms for drawing with `projection` into a target `height_px` pixels high
pub fn uniforms(projection: Mat4, height_px: u32) -> Uniforms {
    // The vertical field of view spans 2 / focal_y at distance 1
    let focal_y = projection.y_axis.y;
    Uniforms {
        projection: projection.to_cols_array_2d(),
        near: NEAR,
        pixel: 2.0 / (focal_y * height_px.max(1) as f32),
        _padding: [0.0; 2],
    }
}

/// Perspective projection with reversed-Z (depth 1 at the near plane, 0 at infinity) and no
/// far plane, for a horizontal field of view
fn projection(horizontal_fov_degrees: f64, aspect: f64) -> Mat4 {
    let focal_x = 1.0 / (horizontal_fov_degrees / 2.0).to_radians().tan();
    let focal_y = focal_x * aspect;
    Mat4::from_cols(
        Vec4::new(focal_x as f32, 0.0, 0.0, 0.0),
        Vec4::new(0.0, focal_y as f32, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, -1.0),
        Vec4::new(0.0, 0.0, NEAR, 0.0),
    )
}

/// Fills `scene` with the bodies as seen by `camera` in `viewport` (points), `days` after J2000
/// TDB, and `targets` with where they are on screen for clicking, farthest first.
pub fn build(
    bodies: &Bodies,
    camera: &Camera,
    viewport: Rect,
    days: f64,
    scene: &mut Scene,
    targets: &mut Vec<Target>,
) {
    let (width, height) = (viewport.width() as f64, viewport.height() as f64);
    scene.projection = projection(camera.fov, width / height.max(1.0));
    scene.instances.clear();
    scene.names.clear();
    targets.clear();

    // Points on screen per unit of x / depth
    let screen_scale = width / 2.0 / (camera.fov / 2.0).to_radians().tan();
    let rotation = camera.view_rotation();
    let sun = bodies.info.iter().position(|info| &*info.name == "Sun");
    let sun_position = sun.map(|s| bodies.position[s]);

    let mut drawn: Vec<(f64, Target)> = Vec::new();

    for i in 0..bodies.len() {
        let info = &bodies.info[i];
        let true_radius = bodies.radius[i];
        let center = camera.to_view(&rotation, bodies.position[i]);
        let depth = -center.z;

        // Small bodies are magnified to stay visible, until they're big enough on their own
        let radius = if depth > 0.0 {
            let true_size = true_radius / depth * screen_scale;
            let size = if true_size < MAX_MAGNIFIED_RADIUS {
                (true_size * info.magnification.cbrt()).min(MAX_MAGNIFIED_RADIUS)
            } else {
                true_size
            }
            .max(MIN_RADIUS);
            true_radius * size / true_size
        } else {
            true_radius
        };

        // Inside the body, or all of it behind the camera
        if center.length() <= radius || center.z >= radius {
            continue;
        }

        let light = match (sun_position, Some(i) == sun) {
            (Some(sun), false) => (rotation * (sun - bodies.position[i])).normalize_or_zero(),
            _ => DVec3::ZERO,
        };
        let [r, g, b] = info.color.map(srgb_to_linear);
        let orientation = rotation::orientation(&info.name, days).unwrap_or(DMat3::IDENTITY);
        let axes = rotation * orientation;
        scene.instances.push(Instance {
            center: center.as_vec3().into(),
            radius: radius as f32,
            color: [r, g, b],
            emissive: if Some(i) == sun { 1.0 } else { 0.0 },
            light: light.as_vec3().into(),
            textured: 0.0,
            axes: [axes.x_axis, axes.y_axis, axes.z_axis].map(|axis| axis.as_vec3().into()),
        });
        scene.names.push(info.name.clone());

        if depth > 0.0 {
            let screen = Pos2::new(
                viewport.center().x + (center.x / depth * screen_scale) as f32,
                viewport.center().y - (center.y / depth * screen_scale) as f32,
            );
            let size = (radius / depth * screen_scale) as f32;
            if viewport.expand(size).contains(screen) {
                let target = Target {
                    id: info.id,
                    center: screen,
                    radius: size,
                };
                drawn.push((center.length_squared(), target));
            }
        }
    }

    drawn.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    targets.extend(drawn.into_iter().map(|(_, target)| target));
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_rust::ephemeris::solar_system;

    const DAYS: f64 = 9496.5;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(800.0, 600.0))
    }

    /// The GPU projection and the CPU's screen positions for clicking must agree
    #[test]
    fn targets_match_projection() {
        let bodies = solar_system();
        let camera = Camera::default();
        let (mut scene, mut targets) = (Scene::default(), Vec::new());
        build(&bodies, &camera, viewport(), DAYS, &mut scene, &mut targets);
        assert!(targets.len() > 10);

        let rotation = camera.view_rotation();
        for target in &targets {
            let i = bodies.index_of(target.id).unwrap();
            let center = camera.to_view(&rotation, bodies.position[i]).as_vec3();
            let clip = scene.projection * center.extend(1.0);
            let ndc = clip / clip.w;
            let rect = viewport();
            let expected = Pos2::new(
                rect.center().x + ndc.x * rect.width() / 2.0,
                rect.center().y - ndc.y * rect.height() / 2.0,
            );
            assert!(
                target.center.distance(expected) < 0.01,
                "{}: {:?} vs {expected:?}",
                bodies.info[i].name,
                target.center
            );
            // Reversed-Z: nearer is larger, and everything in front is within 0..=1
            assert!(ndc.z > 0.0 && ndc.z <= 1.0, "{}", ndc.z);
        }
    }

    #[test]
    fn sizes_follow_magnification_rules() {
        let bodies = solar_system();
        let mut camera = Camera::default();
        let earth = bodies
            .info
            .iter()
            .position(|b| &*b.name == "Earth")
            .unwrap();
        let (mut scene, mut targets) = (Scene::default(), Vec::new());

        // From the default view everything is small, so magnified within the limits
        build(&bodies, &camera, viewport(), DAYS, &mut scene, &mut targets);
        for target in &targets {
            assert!(
                (MIN_RADIUS as f32 - 1e-3..=MAX_MAGNIFIED_RADIUS as f32 + 1e-3)
                    .contains(&target.radius),
                "{}",
                target.radius
            );
        }

        // Close up, the Earth is drawn at its true size
        camera.target = bodies.position[earth];
        camera.distance = 10.0 * bodies.radius[earth];
        build(&bodies, &camera, viewport(), DAYS, &mut scene, &mut targets);
        let target = targets
            .iter()
            .find(|t| t.id == bodies.info[earth].id)
            .unwrap();
        let screen_scale = 400.0 / (camera.fov / 2.0).to_radians().tan();
        let expected = bodies.radius[earth] / camera.distance * screen_scale;
        assert!((target.radius as f64 - expected).abs() < 1e-3 * expected);
    }

    #[test]
    fn lit_from_the_sun() {
        let bodies = solar_system();
        let (mut scene, mut targets) = (Scene::default(), Vec::new());
        build(
            &bodies,
            &Camera::default(),
            viewport(),
            DAYS,
            &mut scene,
            &mut targets,
        );

        let emissive = scene.instances.iter().filter(|i| i.emissive == 1.0).count();
        assert_eq!(emissive, 1);
        for instance in scene.instances.iter().filter(|i| i.emissive == 0.0) {
            let length = glam::Vec3::from(instance.light).length();
            assert!((length - 1.0).abs() < 1e-5);
        }
    }
}
