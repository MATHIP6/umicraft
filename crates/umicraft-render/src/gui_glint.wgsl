// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Enchantment glint over a GUI icon. The quad carries the icon's atlas UV
// rectangle in its vertex colour, so the fragment recovers its position inside
// the icon ("local", 0..1) without extra vertex data. The two scrolling glint
// layers are looked up through the CPU-built transforms in `glint` and summed
// as squares, which is what the two additive `blendFunc(SRC_COLOR, ONE)` draws
// of the original renderer produce. The icon alpha masks the result.

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Layer {
    // Column-major 2x2 matrix: (column 0, column 1).
    rotation: vec4<f32>,
    // Only xy is used.
    offset: vec4<f32>,
};

struct Glint {
    layers: array<Layer, 2>,
};

// 0x8040CC, the glint tint.
const GLINT_COLOR = vec3<f32>(0.5019608, 0.2509804, 0.8);
// Glint texture span covered by the icon: a 16 px sprite over a 64 px texture.
const GLINT_SPAN = 0.25;

@group(0) @binding(0) var icon_atlas: texture_2d<f32>;
@group(0) @binding(1) var icon_sampler: sampler;
@group(0) @binding(2) var glint_texture: texture_2d<f32>;
@group(0) @binding(3) var glint_sampler: sampler;
@group(0) @binding(4) var<uniform> glint: Glint;

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

fn layer_glow(layer: Layer, local: vec2<f32>) -> vec3<f32> {
    let uv = mat2x2<f32>(layer.rotation.xy, layer.rotation.zw) * (local * GLINT_SPAN)
        + layer.offset.xy;
    let tinted = textureSample(glint_texture, glint_sampler, uv).rgb * GLINT_COLOR;
    return tinted * tinted;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let mask = textureSample(icon_atlas, icon_sampler, in.uv).a;
    let local = (in.uv - in.color.xy) / (in.color.zw - in.color.xy);
    let glow = layer_glow(glint.layers[0], local) + layer_glow(glint.layers[1], local);
    return vec4<f32>(glow * mask, 0.0);
}
