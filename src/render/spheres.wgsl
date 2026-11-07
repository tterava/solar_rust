// Bodies as ray traced spheres. Each one is a square facing the camera, and every pixel of it
// works out where its ray hits the sphere, if it does: exact outlines, normals and depth at any
// size, with no mesh.
//
// Everything is in the view frame with the camera at the origin. The shader divides by the body's
// distance, so its numbers stay near 1 even for bodies 10^13 m away.

struct Uniforms {
    projection: mat4x4<f32>,
    near: f32,
    // The size of a pixel at distance 1
    pixel: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct Instance {
    @location(0) center: vec3<f32>,
    @location(1) radius: f32,
    @location(2) color: vec3<f32>,
    @location(3) emissive: f32,
    @location(4) light: vec3<f32>,
}

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    // The point on the square, in units of the body's distance
    @location(0) ray: vec3<f32>,
    // Unit vector to the body's center, and its radius in units of its distance
    @location(1) @interpolate(flat) axis: vec3<f32>,
    @location(2) @interpolate(flat) radius: f32,
    @location(3) @interpolate(flat) distance: f32,
    @location(4) @interpolate(flat) color: vec3<f32>,
    @location(5) @interpolate(flat) emissive: f32,
    @location(6) @interpolate(flat) light: vec3<f32>,
}

// Dim light on the night side, so it isn't completely black
const AMBIENT: f32 = 0.015;

@vertex
fn vs(@builtin(vertex_index) index: u32, body: Instance) -> Varyings {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let corner = corners[index];

    let distance = length(body.center);
    let axis = body.center / distance;
    let radius = body.radius / distance;

    // The rays touching the sphere form a cone, which is r / sqrt(1 - r^2) wide where it
    // passes the center. Two more pixels leave room for the smoothed outline.
    let half_size = radius / sqrt(max(1.0 - radius * radius, 1e-12)) + 2.0 * u.pixel;
    var hint = vec3(0.0, 1.0, 0.0);
    if abs(axis.y) > 0.99 {
        hint = vec3(1.0, 0.0, 0.0);
    }
    let right = normalize(cross(axis, hint));
    let up = cross(right, axis);
    let ray = axis + (corner.x * right + corner.y * up) * half_size;

    var out: Varyings;
    out.clip = u.projection * vec4(ray * distance, 1.0);
    out.ray = ray;
    out.axis = axis;
    out.radius = radius;
    out.distance = distance;
    out.color = body.color;
    out.emissive = body.emissive;
    out.light = body.light;
    return out;
}

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@fragment
fn fs(in: Varyings) -> Fragment {
    let direction = normalize(in.ray);

    // How far the ray passes from the center, relative to the radius: 1 at the outline.
    // The outline is smoothed over a pixel, which alpha to coverage turns into MSAA samples.
    let offset = length(cross(direction, in.axis)) / in.radius;
    let coverage = clamp((1.0 - offset) / max(fwidth(offset), 1e-6) + 0.5, 0.0, 1.0);
    if coverage <= 0.0 {
        discard;
    }

    // Where the ray enters the sphere, or just past the outline, the closest point to it
    let along = dot(direction, in.axis);
    let passing = min(offset, 1.0) * in.radius;
    let entry = along - sqrt(max(in.radius * in.radius - passing * passing, 0.0));
    let hit = direction * entry;
    if hit.z >= 0.0 {
        discard;
    }
    let normal = normalize(hit - in.axis);

    var color: vec3<f32>;
    if in.emissive > 0.5 {
        // Limb darkening: the Sun's edge is dimmer, since we see shallower, cooler layers there
        let facing = max(dot(normal, -direction), 0.0);
        color = in.color * (0.4 + 0.6 * facing);
    } else {
        color = in.color * (max(dot(normal, in.light), 0.0) + AMBIENT);
    }

    var out: Fragment;
    out.color = vec4(color, coverage);
    // Reversed-Z: near / view depth, 1 at the near plane and 0 at infinity
    out.depth = min(u.near / (-hit.z * in.distance), 1.0);
    return out;
}
