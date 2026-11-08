// Bodies as ray traced spheres. Each one is a square facing the camera, and every pixel of it
// works out where its ray hits the sphere, if it does: exact outlines, normals and depth at any
// size, with no mesh. Textures are looked up from the normal, so there's no seam geometry either.
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

// The body's map: equirectangular, east to the right, north up, 0° longitude in the middle
@group(1) @binding(0) var surface: texture_2d<f32>;
@group(1) @binding(1) var surface_sampler: sampler;

struct Instance {
    @location(0) center: vec3<f32>,
    @location(1) radius: f32,
    @location(2) color: vec3<f32>,
    @location(3) emissive: f32,
    @location(4) light: vec3<f32>,
    @location(5) textured: f32,
    // The body-fixed axes: prime meridian, 90° east, north pole
    @location(6) axis_x: vec3<f32>,
    @location(7) axis_y: vec3<f32>,
    @location(8) axis_z: vec3<f32>,
}

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    // The point on the square, in units of the body's distance
    @location(0) ray: vec3<f32>,
    // Unit vector to the body's center, and its radius in units of its distance
    @location(1) @interpolate(flat) axis: vec3<f32>,
    // Radius, distance, emissive and textured
    @location(2) @interpolate(flat) params: vec4<f32>,
    @location(3) @interpolate(flat) color: vec3<f32>,
    @location(4) @interpolate(flat) light: vec3<f32>,
    @location(5) @interpolate(flat) axis_x: vec3<f32>,
    @location(6) @interpolate(flat) axis_y: vec3<f32>,
    @location(7) @interpolate(flat) axis_z: vec3<f32>,
}

// Dim light on the night side, so it isn't completely black
const AMBIENT: f32 = 0.015;
const PI: f32 = 3.14159265358979;

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
    out.params = vec4(radius, distance, body.emissive, body.textured);
    out.color = body.color;
    out.light = body.light;
    out.axis_x = body.axis_x;
    out.axis_y = body.axis_y;
    out.axis_z = body.axis_z;
    return out;
}

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@fragment
fn fs(in: Varyings) -> Fragment {
    let radius = in.params.x;
    let distance = in.params.y;
    let direction = normalize(in.ray);

    // How far the ray passes from the center, relative to the radius: 1 at the outline.
    // The outline is smoothed over a pixel, which alpha to coverage turns into MSAA samples.
    let offset = length(cross(direction, in.axis)) / radius;
    let coverage = clamp((1.0 - offset) / max(fwidth(offset), 1e-6) + 0.5, 0.0, 1.0);

    // Where the ray enters the sphere, or just past the outline, the closest point to it
    let along = dot(direction, in.axis);
    let passing = min(offset, 1.0) * radius;
    let entry = along - sqrt(max(radius * radius - passing * passing, 0.0));
    let hit = direction * entry;
    let normal = normalize(hit - in.axis);

    // Longitude and latitude on the body, as texture coordinates
    let local = vec3(dot(normal, in.axis_x), dot(normal, in.axis_y), dot(normal, in.axis_z));
    let uv = vec2(
        atan2(local.y, local.x) / (2.0 * PI) + 0.5,
        0.5 - asin(clamp(local.z, -1.0, 1.0)) / PI,
    );
    // u jumps from 1 to 0 at 180° longitude, which would pick the smallest mip level along that
    // line. A copy of u that jumps at 0° instead has the right derivatives there, so use whichever
    // changes less (Tarini 2012). Derivatives are taken here, before any pixel is discarded.
    //
    // Towards the poles each row of the map is a smaller circle stretched over the full width, so
    // u changes fast while the surface hardly changes: scaling its derivatives by cos(latitude)
    // measures the actual distance, and keeps the poles from blurring.
    let wrapped = fract(uv.x + 0.5);
    let du = vec2(dpdx(uv.x), dpdy(uv.x));
    let dw = vec2(dpdx(wrapped), dpdy(wrapped));
    let pick = abs(dw) < abs(du);
    let stretch = max(sqrt(max(1.0 - local.z * local.z, 0.0)), 1.0 / 512.0);
    let ddx = vec2(select(du.x, dw.x, pick.x) * stretch, dpdx(uv.y));
    let ddy = vec2(select(du.y, dw.y, pick.y) * stretch, dpdy(uv.y));
    let texel = textureSampleGrad(surface, surface_sampler, uv, ddx, ddy).rgb;

    if coverage <= 0.0 || hit.z >= 0.0 {
        discard;
    }

    var base = in.color;
    if in.params.w > 0.5 {
        base = texel;
    }

    var color: vec3<f32>;
    if in.params.z > 0.5 {
        // Limb darkening: the Sun's edge is dimmer, since we see shallower, cooler layers there
        let facing = max(dot(normal, -direction), 0.0);
        color = base * (0.4 + 0.6 * facing);
    } else {
        color = base * (max(dot(normal, in.light), 0.0) + AMBIENT);
    }

    var out: Fragment;
    out.color = vec4(color, coverage);
    // Reversed-Z: near / view depth, 1 at the near plane and 0 at infinity
    out.depth = min(u.near / (-hit.z * distance), 1.0);
    return out;
}
