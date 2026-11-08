//! What to draw this frame, worked out on the CPU from the simulation's snapshot and the camera.
//! Reads the bodies and never changes them.
//!
//! Nothing here is special to the Sun: every body that shines (see [`light`]) is drawn glowing
//! and lights the bodies around it.

use std::sync::Arc;

use egui::{Pos2, Rect};
use glam::{DMat3, DVec3, Mat4, Vec4};

use solar_rust::astronomy::{Bodies, BodyId, AU};
use solar_rust::forces::G;

use super::curves;
use super::light::{self, Kind, Shine};
use super::orbits;
use super::rotation;
use super::textures::srgb_to_linear;
use super::traces::{self, Traces};
use crate::camera::Camera;
use crate::ui::Target;

/// Magnified mode: bodies smaller than this on screen (points) are magnified, up to this size
const MAX_MAGNIFIED_RADIUS: f64 = 10.0;
/// Magnified mode: nothing is drawn smaller than this (points)
const MIN_RADIUS: f64 = 3.0;
/// True scale mode: bodies smaller than this are drawn as dots of this size (points)
const DOT_RADIUS: f64 = 1.5;
/// Bodies drawn this small (radius in points) are flat dots, too small to show a phase; from
/// [`DOT_RADIUS`] they blend into shaded spheres up to [`MIN_RADIUS`]
/// A star's glow: its radius (points) for the Sun seen from 1 AU, scaled by the fourth root of the
/// star's apparent brightness, within these limits, and at least this many times its disk
const GLOW_RADIUS: f64 = 40.0;
const GLOW_LIMITS: (f64, f64) = (6.0, 400.0);
const GLOW_PER_DISK: f64 = 3.0;
/// Near plane distance in meters. With reversed-Z and a floating point depth buffer the depth
/// precision is about the same at every distance, so this can be tiny.
pub const NEAR: f32 = 1.0;
/// Bodies at least this big (radius, m) are large enough to be round, and their traces and orbits
/// are drawn clearly; smaller ones' faintly, so a hundred asteroids, comets and spacecraft don't
/// bury the planets' orbits
const ROUND_RADIUS: f64 = 200.0E3;
/// How close lines stay to the true curve they stand for, in points on screen
const LINE_TOLERANCE: f64 = 0.25;
/// A trace's fade is drawn in pieces at most this long (seconds of its age)
const FADE_PIECE_SECONDS: f64 = 0.1;
/// Opacity of a trace while it's bright (it fades out with age, see [`traces::brightness`]), and
/// of an orbit:
/// for the selected body, a round body, and anything smaller. Orbits are fainter, so where a body
/// has been stands out from where it's headed.
const TRACE_ALPHA: [f32; 3] = [0.9, 0.7, 0.2];
const ORBIT_ALPHA: [f32; 3] = [0.2, 0.07, 0.02];

/// Ring systems: the body, and the inner and outer radius its ring texture spans (m). Saturn's
/// texture (Solar System Scope) is calibrated by its gaps: the B ring's outer edge (117,580 km) and
/// the A ring's (136,775 km) put its ends at 71,064 and 141,119 km, which also lands the B ring's
/// inner edge, the Encke Gap and the F ring where they are.
const RINGS: &[(&str, f64, f64)] = &[("Saturn", 71_064.0E3, 141_119.0E3)];

/// How traces are drawn
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceMode {
    /// Through space
    Absolute,
    /// Around what each body orbits, for those that orbit something
    Relative,
    Off,
}

impl TraceMode {
    pub fn next(self) -> Self {
        match self {
            TraceMode::Absolute => TraceMode::Relative,
            TraceMode::Relative => TraceMode::Off,
            TraceMode::Off => TraceMode::Absolute,
        }
    }
}

/// What the view shows, toggled from the keyboard
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewOptions {
    pub labels: bool,
    /// Where bodies have been
    pub traces: TraceMode,
    /// The orbits bodies are on
    pub orbits: bool,
    /// Bodies at their true size (small ones as dots) instead of magnified
    pub true_scale: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            labels: true,
            traces: TraceMode::Absolute,
            orbits: true,
            true_scale: false,
        }
    }
}

/// Everything a frame is built from
pub struct View<'a> {
    pub bodies: &'a Bodies,
    pub camera: &'a Camera,
    /// Where the view is on screen (points)
    pub viewport: Rect,
    /// Days since J2000 TDB
    pub days: f64,
    pub options: ViewOptions,
    pub selected: Option<BodyId>,
    pub traces: &'a Traces,
    /// What each body orbits, from [`Bodies::orbital_parents`]
    pub parents: &'a [Option<usize>],
}

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
    /// 1 for bodies that shine, 0 for lit ones
    pub emissive: f32,
    /// The two stars lighting it: unit vectors towards them, and their colors scaled by their
    /// share of the light
    pub light_direction: [f32; 3],
    /// 1 if the body's texture is drawn instead of its color. Set by the renderer, which knows
    /// which textures have loaded.
    pub textured: f32,
    pub light_color: [f32; 3],
    /// 1 for a flat dot, 0 for a shaded sphere, for bodies too small to see their shape
    pub dot: f32,
    pub light_direction_2: [f32; 3],
    pub light_color_2: [f32; 3],
    /// The body-fixed axes in the view frame: towards the prime meridian on the equator, towards
    /// 90° east, and towards the north pole
    pub axes: [[f32; 3]; 3],
}

/// A ring system: a disk in its body's equatorial plane
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RingInstance {
    /// The body's center, meters from the camera
    pub center: [f32; 3],
    /// Inner and outer radius of the ring texture (m), magnified with the body
    pub inner: f32,
    /// Two perpendicular directions in the equatorial plane
    pub axis_x: [f32; 3],
    pub outer: f32,
    pub axis_y: [f32; 3],
    /// The body's drawn radius (m), for its shadow on the rings
    pub body_radius: f32,
    pub light_direction: [f32; 3],
    /// Set by the renderer, like [`Instance::textured`]
    pub textured: f32,
    pub light_color: [f32; 3],
    pub light_direction_2: [f32; 3],
    pub light_color_2: [f32; 3],
}

/// The glow around a star, drawn over everything it's not hidden behind
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Glow {
    /// Meters from the camera
    pub center: [f32; 3],
    /// Meters
    pub radius: f32,
    /// Linear RGB, brightness included
    pub color: [f32; 3],
    pub _padding: f32,
}

/// One end of a line segment, of a trace or an orbit
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    /// Meters from the camera
    pub position: [f32; 3],
    /// Linear RGB and opacity
    pub color: [f32; 4],
}

/// A body's name on screen
#[derive(Clone, Debug)]
pub struct Label {
    pub name: Arc<str>,
    pub center: Pos2,
    /// Drawn radius (points)
    pub radius: f32,
    /// Bigger is more important: the body's true size on screen
    pub priority: f64,
    pub selected: bool,
}

/// Per-frame values shared by every draw
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub projection: [[f32; 4]; 4],
    /// Rotation from the ecliptic frame into the view frame, for the sky
    pub rotation: [[f32; 4]; 4],
    /// Size of the view in pixels
    pub viewport: [f32; 2],
    pub near: f32,
    /// The size of a pixel at distance 1, for smoothing outlines
    pub pixel: f32,
}

#[derive(Default)]
pub struct Scene {
    pub instances: Vec<Instance>,
    /// The body each instance is, for finding its texture
    pub names: Vec<Arc<str>>,
    pub rings: Vec<RingInstance>,
    /// The body each ring system belongs to
    pub ring_names: Vec<Arc<str>>,
    pub glows: Vec<Glow>,
    /// Traces and orbits: pairs of vertices, one pair per segment
    pub lines: Vec<LineVertex>,
    /// Most important first
    pub labels: Vec<Label>,
    pub projection: Mat4,
    pub rotation: Mat4,
}

/// The uniforms for drawing `scene` into a target of `size` pixels
pub fn uniforms(projection: Mat4, rotation: Mat4, size: [u32; 2]) -> Uniforms {
    // The vertical field of view spans 2 / focal_y at distance 1
    let focal_y = projection.y_axis.y;
    Uniforms {
        projection: projection.to_cols_array_2d(),
        rotation: rotation.to_cols_array_2d(),
        viewport: [size[0] as f32, size[1] as f32],
        near: NEAR,
        pixel: 2.0 / (focal_y * size[1].max(1) as f32),
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

/// A body that shines, and the color of its light
struct Star {
    index: usize,
    shine: Shine,
    color: [f32; 3],
}

/// Directions (ecliptic frame) and weighted colors of the two stars lighting body `i`, most
/// light first. Their shares add up to 1, so a body is as bright near one star as between two.
fn lighting(bodies: &Bodies, stars: &[Star], i: usize) -> [(DVec3, [f32; 3]); 2] {
    let mut lights: Vec<(f64, DVec3, [f32; 3])> = stars
        .iter()
        .filter(|star| star.index != i)
        .map(|star| {
            let offset = bodies.position[star.index] - bodies.position[i];
            let flux = star.shine.brightness / offset.length_squared();
            (flux, offset.normalize_or_zero(), star.color)
        })
        .collect();
    lights.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    lights.truncate(2);
    let total: f64 = lights.iter().map(|l| l.0).sum();
    let mut result = [(DVec3::ZERO, [0.0; 3]); 2];
    for (slot, (flux, direction, color)) in result.iter_mut().zip(lights) {
        let share = (flux / total) as f32;
        *slot = (direction, color.map(|c| c * share));
    }
    result
}

/// Fills `scene` with what `view` shows, and `targets` with where bodies are on screen for
/// clicking, farthest first
pub fn build(view: &View, scene: &mut Scene, targets: &mut Vec<Target>) {
    let View {
        bodies,
        camera,
        viewport,
        days,
        options,
        selected,
        traces,
        parents,
    } = *view;
    let (width, height) = (viewport.width() as f64, viewport.height() as f64);
    let rotation = camera.view_rotation();
    scene.projection = projection(camera.fov, width / height.max(1.0));
    scene.rotation = Mat4::from_mat3(rotation.as_mat3());
    scene.instances.clear();
    scene.names.clear();
    scene.rings.clear();
    scene.ring_names.clear();
    scene.glows.clear();
    scene.lines.clear();
    scene.labels.clear();
    targets.clear();

    // Points on screen per unit of x / depth
    let screen_scale = width / 2.0 / (camera.fov / 2.0).to_radians().tan();
    let stars: Vec<Star> = (0..bodies.len())
        .filter_map(|i| {
            let shine = light::shine(bodies.mass[i], bodies.radius[i], &bodies.info[i].emission)?;
            Some(Star {
                index: i,
                shine,
                color: light::color(shine.temperature),
            })
        })
        .collect();
    let to_view = |direction: DVec3| (rotation * direction).as_vec3().to_array();
    let color_of = |i: usize| match stars.iter().find(|s| s.index == i) {
        Some(star) => star.color,
        None => bodies.info[i].color.map(srgb_to_linear),
    };

    let mut drawn: Vec<(f64, Target, Label)> = Vec::new();

    for i in 0..bodies.len() {
        let info = &bodies.info[i];
        let true_radius = bodies.radius[i];
        let center = camera.to_view(&rotation, bodies.position[i]);
        let depth = -center.z;
        let true_size = true_radius / depth * screen_scale;

        // Small bodies are magnified or drawn as dots to stay visible
        let radius = if depth <= 0.0 {
            true_radius
        } else {
            let size = if options.true_scale {
                true_size.max(DOT_RADIUS)
            } else if true_size < MAX_MAGNIFIED_RADIUS {
                (true_size * info.magnification.cbrt())
                    .clamp(MIN_RADIUS, MAX_MAGNIFIED_RADIUS)
                    .max(true_size)
            } else {
                true_size
            };
            true_radius * size / true_size
        };

        // Inside the body, or all of it behind the camera
        if center.length() <= radius || center.z >= radius {
            continue;
        }

        let star = stars.iter().find(|s| s.index == i);
        // A black hole that gives off no light is a black disk hiding what's behind it
        let black_hole =
            star.is_none() && light::classify(bodies.mass[i], true_radius) == Kind::BlackHole;
        let [(direction, color), (direction_2, color_2)] = if stars.is_empty() {
            // Nothing shines, so light everything from the camera to keep it visible
            [(-center.normalize(), [1.0; 3]), (DVec3::ZERO, [0.0; 3])]
                .map(|(d, c)| (rotation.transpose() * d, c))
        } else {
            lighting(bodies, &stars, i)
        };
        let orientation = rotation::orientation(&info.name, days).unwrap_or(DMat3::IDENTITY);
        let axes = rotation * orientation;
        let dot = if depth > 0.0 {
            let drawn = radius / depth * screen_scale;
            ((MIN_RADIUS - drawn) / (MIN_RADIUS - DOT_RADIUS)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        scene.instances.push(Instance {
            center: center.as_vec3().into(),
            radius: radius as f32,
            color: if black_hole { [0.0; 3] } else { color_of(i) },
            emissive: (star.is_some() || black_hole) as u32 as f32,
            light_direction: to_view(direction),
            textured: 0.0,
            light_color: color,
            dot,
            light_direction_2: to_view(direction_2),
            light_color_2: color_2,
            axes: [axes.x_axis, axes.y_axis, axes.z_axis].map(|axis| axis.as_vec3().into()),
        });
        scene.names.push(info.name.clone());

        if let Some(&(_, inner, outer)) = RINGS.iter().find(|(name, ..)| *name == &*info.name) {
            let scale = (radius / true_radius) as f32;
            scene.rings.push(RingInstance {
                center: center.as_vec3().into(),
                inner: inner as f32 * scale,
                axis_x: axes.x_axis.as_vec3().into(),
                outer: outer as f32 * scale,
                axis_y: axes.y_axis.as_vec3().into(),
                body_radius: radius as f32,
                light_direction: to_view(direction),
                textured: 0.0,
                light_color: color,
                light_direction_2: to_view(direction_2),
                light_color_2: color_2,
            });
            scene.ring_names.push(info.name.clone());
        }

        if depth <= 0.0 {
            continue;
        }
        let size = radius / depth * screen_scale;

        if let Some(star) = star {
            // Apparent brightness compared to the Sun from 1 AU
            let brightness = star.shine.brightness / (center.length() / AU).powi(2);
            let glow = (GLOW_RADIUS * brightness.powf(0.25))
                .clamp(GLOW_LIMITS.0, GLOW_LIMITS.1)
                .max(GLOW_PER_DISK * size);
            let intensity = brightness.powf(0.25).clamp(0.3, 1.0) as f32;
            scene.glows.push(Glow {
                center: center.as_vec3().into(),
                radius: (glow * depth / screen_scale) as f32,
                color: star.color.map(|c| c * intensity),
                _padding: 0.0,
            });
        }

        let screen = Pos2::new(
            viewport.center().x + (center.x / depth * screen_scale) as f32,
            viewport.center().y - (center.y / depth * screen_scale) as f32,
        );
        if viewport.expand(size as f32).contains(screen) {
            let target = Target {
                id: info.id,
                center: screen,
                radius: size as f32,
            };
            let label = Label {
                name: info.name.clone(),
                center: screen,
                radius: size as f32,
                priority: true_size,
                selected: selected == Some(info.id),
            };
            drawn.push((center.length_squared(), target, label));
        }
    }

    drawn.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));

    if options.labels {
        // Bodies hidden behind a nearer, bigger one get no label
        for (k, (_, target, label)) in drawn.iter().enumerate() {
            let hidden = drawn[k + 1..].iter().any(|(_, nearer, _)| {
                nearer.radius > target.radius
                    && nearer.center.distance(target.center) < nearer.radius
            });
            if !hidden {
                scene.labels.push(label.clone());
            }
        }
        scene.labels.sort_by(|a, b| {
            b.selected
                .cmp(&a.selected)
                .then(b.priority.total_cmp(&a.priority))
        });
    }
    targets.extend(drawn.into_iter().map(|(_, target, _)| target));

    let prominence = |i: usize| {
        if selected == Some(bodies.info[i].id) {
            0
        } else if bodies.radius[i] >= ROUND_RADIUS {
            1
        } else {
            2
        }
    };
    let line = |position: DVec3, color: [f32; 3], alpha: f32| LineVertex {
        position: camera.to_view(&rotation, position).as_vec3().into(),
        color: [color[0], color[1], color[2], alpha],
    };
    // Lines are cut into pieces fine enough to stay this close to the true curve (radians as seen
    // from the camera)
    let tolerance = LINE_TOLERANCE / screen_scale;
    let pieces = |center: DVec3, a: DVec3, b: DVec3, deviation: f64| {
        curves::pieces(
            camera.to_view(&rotation, center + a),
            camera.to_view(&rotation, center + b),
            deviation,
            tolerance,
        )
    };

    if options.traces != TraceMode::Off {
        for i in 0..bodies.len() {
            let Some(trace) = traces.get(bodies.info[i].id) else {
                continue;
            };
            let (center, points) = match (options.traces, parents[i]) {
                (TraceMode::Relative, Some(parent)) => (
                    bodies.position[parent],
                    trace.relative(bodies.info[parent].id),
                ),
                _ => (DVec3::ZERO, trace.absolute()),
            };
            if points.len() < 2 {
                continue;
            }
            let color = color_of(i);
            let bright = TRACE_ALPHA[prominence(i)];
            let mut previous: Option<LineVertex> = None;
            let mut to = |position: DVec3, age: f64| {
                let alpha = bright * traces::brightness(age) as f32;
                let vertex = line(center + position, color, alpha);
                if let Some(previous) = previous.replace(vertex) {
                    scene.lines.extend([previous, vertex]);
                }
            };
            for pair in points.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                if b.age >= traces::LIFETIME_SECONDS {
                    continue;
                }
                // Only the part that hasn't faded out yet
                let start = if a.age > traces::LIFETIME_SECONDS {
                    (a.age - traces::LIFETIME_SECONDS) / (a.age - b.age)
                } else {
                    0.0
                };
                let deviation = curves::hermite_deviation(&a.point, &b.point);
                let shape = pieces(center, a.point.position, b.point.position, deviation);
                // Short enough in time for the fade to look smooth
                let fade = ((a.age.min(traces::LIFETIME_SECONDS) - b.age) / FADE_PIECE_SECONDS)
                    .ceil() as usize;
                let n = shape.max(fade).max(1);
                for j in 0..n {
                    let s = start + (1.0 - start) * j as f64 / n as f64;
                    let age = a.age + (b.age - a.age) * s;
                    to(curves::hermite(&a.point, &b.point, s), age);
                }
            }
            let newest = points[points.len() - 1];
            to(newest.point.position, newest.age);
        }
    }

    if options.orbits {
        for (i, &parent) in parents.iter().enumerate() {
            let Some(parent) = parent else {
                continue;
            };
            let center = bodies.position[parent];
            let Some(points) = orbits::ellipse(
                bodies.position[i] - center,
                bodies.velocity[i] - bodies.velocity[parent],
                G * (bodies.mass[i] + bodies.mass[parent]),
                |a, b, deviation| pieces(center, a, b, deviation),
            ) else {
                continue;
            };
            let alpha = ORBIT_ALPHA[prominence(i)];
            let color = color_of(i);
            for k in 0..points.len() {
                let next = (k + 1) % points.len();
                scene.lines.push(line(center + points[k], color, alpha));
                scene.lines.push(line(center + points[next], color, alpha));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_rust::astronomy::{Emission, NewBody};
    use solar_rust::ephemeris::solar_system;

    const DAYS: f64 = 9496.5;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(800.0, 600.0))
    }

    fn build_with(bodies: &Bodies, camera: &Camera, options: ViewOptions) -> (Scene, Vec<Target>) {
        let traces = Traces::default();
        let parents = bodies.orbital_parents();
        let view = View {
            bodies,
            camera,
            viewport: viewport(),
            days: DAYS,
            options,
            selected: None,
            traces: &traces,
            parents: &parents,
        };
        let (mut scene, mut targets) = (Scene::default(), Vec::new());
        build(&view, &mut scene, &mut targets);
        (scene, targets)
    }

    fn index(bodies: &Bodies, name: &str) -> usize {
        bodies.info.iter().position(|b| &*b.name == name).unwrap()
    }

    /// The GPU projection and the CPU's screen positions for clicking must agree
    #[test]
    fn targets_match_projection() {
        let bodies = solar_system();
        let camera = Camera::default();
        let (scene, targets) = build_with(&bodies, &camera, ViewOptions::default());
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
        let earth = index(&bodies, "Earth");

        // From the default view everything is small, so magnified within the limits
        let (_, targets) = build_with(&bodies, &camera, ViewOptions::default());
        for target in &targets {
            assert!(
                (MIN_RADIUS as f32 - 1e-3..=MAX_MAGNIFIED_RADIUS as f32 + 1e-3)
                    .contains(&target.radius),
                "{}",
                target.radius
            );
        }

        // At true scale, they're dots
        let true_scale = ViewOptions {
            true_scale: true,
            ..Default::default()
        };
        let (scene, targets) = build_with(&bodies, &camera, true_scale);
        assert!(targets
            .iter()
            .all(|t| (t.radius - DOT_RADIUS as f32).abs() < 1e-3));
        // and the Earth, far under a point wide from 2 AU, is a flat dot
        let instance = scene.names.iter().position(|n| &**n == "Earth").unwrap();
        assert_eq!(scene.instances[instance].dot, 1.0);

        // Close up, the Earth is drawn at its true size, shaded
        camera.target = bodies.position[earth];
        camera.distance = 10.0 * bodies.radius[earth];
        let (scene, targets) = build_with(&bodies, &camera, ViewOptions::default());
        let target = targets
            .iter()
            .find(|t| t.id == bodies.info[earth].id)
            .unwrap();
        let screen_scale = 400.0 / (camera.fov / 2.0).to_radians().tan();
        let expected = bodies.radius[earth] / camera.distance * screen_scale;
        assert!((target.radius as f64 - expected).abs() < 1e-3 * expected);
        let instance = scene.names.iter().position(|n| &**n == "Earth").unwrap();
        assert_eq!(scene.instances[instance].dot, 0.0);
    }

    #[test]
    fn lit_by_the_sun() {
        let bodies = solar_system();
        let (scene, _) = build_with(&bodies, &Camera::default(), ViewOptions::default());

        let emissive = scene.instances.iter().filter(|i| i.emissive == 1.0).count();
        assert_eq!(emissive, 1);
        assert_eq!(scene.glows.len(), 1);
        for instance in scene.instances.iter().filter(|i| i.emissive == 0.0) {
            let length = glam::Vec3::from(instance.light_direction).length();
            assert!((length - 1.0).abs() < 1e-5);
            // All the light is the Sun's, which is white
            assert_eq!(instance.light_color, [1.0; 3]);
            assert_eq!(instance.light_color_2, [0.0; 3]);
        }
    }

    /// A second star, a red dwarf 1 AU beyond the Earth, shines and shares the lighting
    #[test]
    fn added_star_shines() {
        let mut bodies = solar_system();
        let earth = index(&bodies, "Earth");
        let position = bodies.position[earth] * 2.0;
        bodies.push(NewBody {
            name: "Red dwarf".into(),
            mass: 0.3 * 1.989E30,
            radius: 0.3 * 6.957E8,
            position,
            velocity: DVec3::ZERO,
            oblateness: None,
            magnification: 100.0,
            color: [0; 3],
            emission: Emission::default(),
        });
        let (scene, _) = build_with(&bodies, &Camera::default(), ViewOptions::default());

        assert_eq!(scene.glows.len(), 2);
        let dwarf = scene
            .names
            .iter()
            .position(|n| &**n == "Red dwarf")
            .unwrap();
        let [r, g, b] = scene.instances[dwarf].color;
        assert!(scene.instances[dwarf].emissive == 1.0 && r > g && g > b);

        // The Earth, halfway between, gets most of its light from the Sun but some reddish light
        // from the dwarf, from the other side
        let earth = scene.names.iter().position(|n| &**n == "Earth").unwrap();
        let instance = &scene.instances[earth];
        let total = instance.light_color[1] + instance.light_color_2[1];
        assert!(instance.light_color[0] > 0.9 && instance.light_color_2[0] > 0.0);
        assert!(instance.light_color_2[2] < instance.light_color_2[0]);
        assert!(total <= 1.0 + 1e-6);
        let towards = |d: [f32; 3]| glam::Vec3::from(d);
        assert!(towards(instance.light_direction).dot(towards(instance.light_direction_2)) < -0.9);
    }

    /// A black hole is a black disk with no glow; a neutron star is a faint point of light
    #[test]
    fn remnants() {
        let mut bodies = solar_system();
        let earth = index(&bodies, "Earth");
        for (name, mass, radius, distance) in [
            ("Black hole", 10.0 * 1.989E30, 29.5E3, 2.0),
            ("Neutron star", 1.4 * 1.989E30, 12.0E3, 3.0),
        ] {
            bodies.push(NewBody {
                name: name.into(),
                mass,
                radius,
                position: bodies.position[earth] * distance,
                velocity: DVec3::ZERO,
                oblateness: None,
                magnification: 1.0E7,
                color: [200; 3],
                emission: Emission::default(),
            });
        }
        let (scene, _) = build_with(&bodies, &Camera::default(), ViewOptions::default());
        let instance = |name: &str| {
            let i = scene.names.iter().position(|n| &**n == name).unwrap();
            scene.instances[i]
        };
        let hole = instance("Black hole");
        assert_eq!((hole.color, hole.emissive), ([0.0; 3], 1.0));
        assert_eq!(instance("Neutron star").emissive, 1.0);

        // Glows for the Sun and the neutron star only, the neutron star's far fainter
        assert_eq!(scene.glows.len(), 2);
        let faintest = scene
            .glows
            .iter()
            .map(|g| glam::Vec3::from(g.color).max_element())
            .fold(f32::MAX, f32::min);
        assert!(faintest <= 0.31, "{faintest}");
    }
}
