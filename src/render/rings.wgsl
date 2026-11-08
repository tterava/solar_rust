// Ring systems: a square in the body's equatorial plane, with the ring texture looked up by the
// distance from the body's center. Lit from the stars on either side: brightest seen from the lit
// side, dimmer through the rings from the other, and dark in the body's shadow.

struct Uniforms {
    projection: mat4x4<f32>,
    rotation: mat4x4<f32>,
    viewport: vec2<f32>,
    near: f32,
    pixel: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

// Runs from the inner edge (u = 0) to the outer edge (u = 1), opacity in alpha
@group(1) @binding(0) var strip: texture_2d<f32>;
@group(1) @binding(1) var strip_sampler: sampler;

struct Ring {
    @location(0) center: vec3<f32>,
    @location(1) inner: f32,
    @location(2) axis_x: vec3<f32>,
    @location(3) outer: f32,
    @location(4) axis_y: vec3<f32>,
    @location(5) body_radius: f32,
    @location(6) light: vec3<f32>,
    @location(7) textured: f32,
    @location(8) light_color: vec3<f32>,
    @location(9) light_2: vec3<f32>,
    @location(10) light_color_2: vec3<f32>,
}

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    // Meters from the body's center, in the ring plane
    @location(0) local: vec2<f32>,
    // Meters from the camera
    @location(1) position: vec3<f32>,
    @location(2) @interpolate(flat) axis_x: vec3<f32>,
    @location(3) @interpolate(flat) axis_y: vec3<f32>,
    // Inner and outer radius, body radius, textured
    @location(4) @interpolate(flat) params: vec4<f32>,
    @location(5) @interpolate(flat) light: vec3<f32>,
    @location(6) @interpolate(flat) light_color: vec3<f32>,
    @location(7) @interpolate(flat) light_2: vec3<f32>,
    @location(8) @interpolate(flat) light_color_2: vec3<f32>,
}

// Seen from the unlit side, light scattered forward through the rings
const THROUGH: f32 = 0.3;
// Without a texture: an even, half transparent band of this color
const PLAIN: vec4<f32> = vec4(0.65, 0.58, 0.48, 0.5);

@vertex
fn vs(@builtin(vertex_index) index: u32, ring: Ring) -> Varyings {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let local = corners[index] * ring.outer;
    let position = ring.center + local.x * ring.axis_x + local.y * ring.axis_y;

    var out: Varyings;
    out.clip = u.projection * vec4(position, 1.0);
    out.local = local;
    out.position = position;
    out.axis_x = ring.axis_x;
    out.axis_y = ring.axis_y;
    out.params = vec4(ring.inner, ring.outer, ring.body_radius, ring.textured);
    out.light = ring.light;
    out.light_color = ring.light_color;
    out.light_2 = ring.light_2;
    out.light_color_2 = ring.light_color_2;
    return out;
}

// How brightly a light lights this point of the rings, seen from the camera
fn lit(light: vec3<f32>, offset: vec3<f32>, normal: vec3<f32>, to_camera: vec3<f32>,
       body_radius: f32, opacity: f32) -> f32 {
    // In the body's shadow: behind it, and within its radius of the line towards the light
    let along = dot(offset, light);
    if along < 0.0 && length(offset - along * light) < body_radius {
        return 0.0;
    }
    let incidence = dot(normal, light);
    var side = 1.0;
    if incidence * dot(normal, to_camera) < 0.0 {
        side = THROUGH * (1.0 - 0.5 * opacity);
    }
    return side * sqrt(abs(incidence));
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    let inner = in.params.x;
    let outer = in.params.y;
    let radius = length(in.local);
    let along = (radius - inner) / (outer - inner);
    // Sampled before anything is discarded, which the derivatives need
    let texel = textureSample(strip, strip_sampler, vec2(clamp(along, 0.0, 1.0), 0.5));
    if along < 0.0 || along > 1.0 {
        discard;
    }

    var ring = PLAIN;
    if in.params.w > 0.5 {
        ring = texel;
    }

    let offset = in.local.x * in.axis_x + in.local.y * in.axis_y;
    let normal = normalize(cross(in.axis_x, in.axis_y));
    let to_camera = -in.position;
    let light = lit(in.light, offset, normal, to_camera, in.params.z, ring.a) * in.light_color
        + lit(in.light_2, offset, normal, to_camera, in.params.z, ring.a) * in.light_color_2;
    return vec4(ring.rgb * light, ring.a);
}
