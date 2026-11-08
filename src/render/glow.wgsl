// The glow around stars: a square facing the camera at the star's distance, brightest in the
// middle. It's added to what's behind it, and hidden by anything in front, including the star's
// own disk, so it shows around the star.

struct Uniforms {
    projection: mat4x4<f32>,
    rotation: mat4x4<f32>,
    viewport: vec2<f32>,
    near: f32,
    pixel: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct Glow {
    // Meters from the camera
    @location(0) center: vec3<f32>,
    // Meters
    @location(1) radius: f32,
    @location(2) color: vec3<f32>,
}

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    // Position on the square, -1 to 1
    @location(0) offset: vec2<f32>,
    @location(1) @interpolate(flat) color: vec3<f32>,
}

@vertex
fn vs(@builtin(vertex_index) index: u32, glow: Glow) -> Varyings {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let corner = corners[index];

    let axis = normalize(glow.center);
    var hint = vec3(0.0, 1.0, 0.0);
    if abs(axis.y) > 0.99 {
        hint = vec3(1.0, 0.0, 0.0);
    }
    let right = normalize(cross(axis, hint));
    let up = cross(right, axis);
    let position = glow.center + (corner.x * right + corner.y * up) * glow.radius;

    var out: Varyings;
    out.clip = u.projection * vec4(position, 1.0);
    out.offset = corner;
    out.color = glow.color;
    return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    let r = length(in.offset);
    let fade = max(1.0 - r, 0.0);
    let brightness = 1.5 * fade * fade * exp(-3.0 * r);
    return vec4(in.color * brightness, 1.0);
}
