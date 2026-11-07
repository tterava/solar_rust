// Copies the rendered scene into egui's frame, as a triangle covering the viewport

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Varyings {
    // (0, 0), (2, 0), (0, 2): one triangle that covers the whole viewport
    let uv = vec2(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Varyings;
    out.position = vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// For an sRGB target, which encodes the linear colors itself
@fragment
fn fs_linear(in: Varyings) -> @location(0) vec4<f32> {
    return textureSample(scene, scene_sampler, in.uv);
}

// For a plain target: encode to sRGB here
@fragment
fn fs_encode_srgb(in: Varyings) -> @location(0) vec4<f32> {
    let color = textureSample(scene, scene_sampler, in.uv);
    let low = color.rgb * 12.92;
    let high = 1.055 * pow(color.rgb, vec3(1.0 / 2.4)) - 0.055;
    return vec4(select(high, low, color.rgb <= vec3(0.0031308)), color.a);
}
