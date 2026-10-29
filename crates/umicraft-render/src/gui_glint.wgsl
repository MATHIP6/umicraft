// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Enchantment glint over a GUI icon. The quad carries the icon's atlas UV
// rectangle in its vertex colour, so the fragment recovers its position inside
// the icon ("local", 0..1) without extra vertex data. The two scrolling glint
// layers are looked up through the CPU-built transforms in `glint` and summed
// as squares, which is what the two additive `blendFunc(SRC_COLOR, ONE)` draws
// of the original renderer produce. The icon alpha masks the result. The
// shared maths lives in `glint_common.wgsl`, prepended at build time.

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

// Cells per side of the 2048 px icon atlas of 64 px cells.
const ICON_CELLS = 32.0;

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

fn glow(layer: Layer, local: vec2<f32>, cell: vec2<f32>) -> vec3<f32> {
    return layer_glow(textureSample(glint_texture, glint_sampler, layer_uv(layer, local, cell)).rgb);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let mask = textureSample(icon_atlas, icon_sampler, in.uv).a;
    let local = (in.uv - in.color.xy) / (in.color.zw - in.color.xy);
    // The icon atlas holds 32x32 cells, so the rectangle origin is the cell:
    // each icon takes its own part of the glint texture.
    let cell = round(in.color.xy * ICON_CELLS);
    let glow = glow(glint.layers[0], local, cell) + glow(glint.layers[1], local, cell);
    return vec4<f32>(glow * mask, 0.0);
}
