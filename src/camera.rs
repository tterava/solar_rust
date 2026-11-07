use std::f64::consts::PI;
use std::time::Instant;

use glam::{DAffine3, DVec3};

use solar_rust::astronomy::AU;

// For drawing camera is assumed to be situated on the positive side of the Z-axis at (0,0,1), with target being origin.
// Matrix operations are used to transform the simulation space into camera space.
// Camera is always kept in line with Y-axis (Y-axis is directly up). In other words camera only has pitch and yaw.

const FLY_TO_SECONDS: f64 = 1.5;
/// How far the camera settles from a followed body, in body radii
const FOLLOW_DISTANCE_RADII: f64 = 100.0;
const ROTATE_SENSITIVITY: f64 = 0.003;
/// egui reports one mouse wheel notch as 40 points of scroll
const ZOOM_PER_NOTCH: f64 = 1.1;
const SCROLL_POINTS_PER_NOTCH: f64 = 40.0;

pub struct Camera {
    pub target: DVec3,
    pub distance: f64,
    yaw: f64,
    pitch: f64,
    pub fov: f64,
    fly_to: Option<FlyTo>,
}

/// Animated transition from the current view to a newly selected body
struct FlyTo {
    started: Instant,
    from_target: DVec3,
    from_distance: f64,
    /// Where the body was when the animation began
    body_start: DVec3,
}

impl Camera {
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.set_yaw(self.yaw - dx as f64 * ROTATE_SENSITIVITY);
        self.set_pitch(self.pitch - dy as f64 * ROTATE_SENSITIVITY);
    }

    fn set_pitch(&mut self, angle: f64) {
        // Limit pitch between -89.9 degrees and 89.9 degrees or things go upside down.
        self.pitch = angle.clamp(-PI / 180.0 * 89.9, PI / 180.0 * 89.9);
    }

    fn set_yaw(&mut self, angle: f64) {
        self.yaw = angle % (2.0 * PI);
    }

    /// Zooms proportionally to the scroll amount, so one wheel notch is always one zoom step
    /// regardless of how egui spreads it over frames, and touchpads zoom smoothly.
    pub fn zoom(&mut self, scroll_points: f32) {
        self.distance /= ZOOM_PER_NOTCH.powf(scroll_points as f64 / SCROLL_POINTS_PER_NOTCH);
    }

    pub fn view_transform(&self) -> DAffine3 {
        let translation = DAffine3::from_translation(-self.target);

        let rot_y = DAffine3::from_rotation_y(-self.yaw);
        let rot_x = DAffine3::from_rotation_x(-self.pitch);

        let scale = DAffine3::from_scale(DVec3::ONE / self.distance);

        scale * rot_x * rot_y * translation
    }

    pub fn position(&self) -> DVec3 {
        let scale = DAffine3::from_scale(DVec3::ONE * self.distance);
        let rot_y = DAffine3::from_rotation_y(self.yaw);
        let rot_x = DAffine3::from_rotation_x(self.pitch);

        let direction_vec = (scale * rot_y * rot_x).transform_point3(DVec3::new(0.0, 0.0, 1.0));

        self.target + direction_vec
    }

    /// Starts flying from the current view to a body. Call [`Camera::follow`] every frame after.
    pub fn fly_to(&mut self, body_position: DVec3) {
        self.fly_to = Some(FlyTo {
            started: Instant::now(),
            from_target: self.target,
            from_distance: self.distance,
            body_start: body_position,
        });
    }

    pub fn is_animating(&self) -> bool {
        self.fly_to.is_some()
    }

    /// Keeps the followed body centered, and advances the fly-to animation if one is running.
    pub fn follow(&mut self, body_position: DVec3, body_radius: f64) {
        let Some(fly_to) = &self.fly_to else {
            self.target = body_position;
            return;
        };

        let end_distance = body_radius * FOLLOW_DISTANCE_RADII;
        let progress = fly_to.started.elapsed().as_secs_f64() / FLY_TO_SECONDS;

        if progress >= 1.0 {
            self.target = body_position;
            self.distance = end_distance;
            self.fly_to = None;
            return;
        }

        // Interpolate along a fixed path between the camera's start point and where the
        // body was when the animation began, then add the body's own motion 1:1.
        // This way a moving (or fast-orbiting) body never makes the camera overshoot.
        let eased = ease_in_out_cubic(progress);
        let base = fly_to.from_target.lerp(fly_to.body_start, eased);
        let body_motion = body_position - fly_to.body_start;

        self.target = base + body_motion;
        self.distance = fly_to.from_distance + (end_distance - fly_to.from_distance) * eased;
    }

    /// Stops following a body and looks back at the origin
    pub fn release(&mut self) {
        self.fly_to = None;
        self.target = DVec3::ZERO;
    }
}

fn ease_in_out_cubic(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = 2.0 * t - 2.0;
        u * u * u / 2.0 + 1.0
    }
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            target: DVec3::ZERO,
            distance: 2.0 * AU,
            yaw: 0.0,
            pitch: 0.0,
            fov: 80.0,
            fly_to: None,
        }
    }
}
