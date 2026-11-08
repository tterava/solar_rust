// Background stars: points infinitely far away, each a small soft dot facing the camera. Drawn
// first, behind everything, adding up where they overlap.

struct Uniforms {
    projection: mat4x4<f32>,
    rotation: mat4x4<f32>,
    viewport: vec2<f32>,
    near: f32,
    pixel: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct Star {
    // Unit vector in the ecliptic frame
    @location(0) direction: vec3<f32>,
    // Radius in pixels
    @location(1) size: f32,
    @location(2) color: vec3<f32>,
}

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    // Offset from the star's center, in units of its size
    @location(0) offset: vec2<f32>,
    @location(1) @interpolate(flat) color: vec3<f32>,
}

// The square reaches this many times the star's size, where its glow has faded
const EXTENT: f32 = 2.0;

@vertex
fn vs(@builtin(vertex_index) index: u32, star: Star) -> Varyings {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let corner = corners[index] * EXTENT;

    // A direction (w = 0) projects to where the point at infinity is
    let direction = (u.rotation * vec4(star.direction, 0.0)).xyz;
    var clip = u.projection * vec4(direction, 0.0);
    var out: Varyings;
    if clip.w <= 0.0 {
        // Behind the camera: outside the clip volume, so nothing is drawn
        out.clip = vec4(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    // Depth doesn't matter (no depth test or writes), but has to be inside the clip volume
    clip.z = 0.5 * clip.w;
    clip = vec4(clip.xy + corner * star.size * 2.0 / u.viewport * clip.w, clip.zw);

    out.clip = clip;
    out.offset = corner;
    out.color = star.color;
    return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    let distance = length(in.offset);
    let brightness = exp(-2.0 * distance * distance);
    return vec4(in.color * brightness, 1.0);
}
