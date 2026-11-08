// Traces and orbits: line segments with their own color and opacity, behind anything in front
// of them

struct Uniforms {
    projection: mat4x4<f32>,
    rotation: mat4x4<f32>,
    viewport: vec2<f32>,
    near: f32,
    pixel: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct Vertex {
    // Meters from the camera
    @location(0) position: vec3<f32>,
    // Linear RGB and opacity
    @location(1) color: vec4<f32>,
}

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs(vertex: Vertex) -> Varyings {
    var out: Varyings;
    out.clip = u.projection * vec4(vertex.position, 1.0);
    out.color = vertex.color;
    return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    return in.color;
}
