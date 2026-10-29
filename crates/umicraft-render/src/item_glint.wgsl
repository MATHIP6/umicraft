// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Enchantment glint over world and hand items. Appended to `glint_common.wgsl`
// and `terrain.wgsl` at build time: the vertex stage is the terrain one (same
// `Vertex` layout and camera at group 0), only the fragment stage differs.
// The item mesh is drawn again; the sprite-local position is the vertex uv
// (0..1 inside the sprite's texture-array layer), and the sprite alpha masks
// the glow like the terrain cutout. MCP `RenderItem.renderEffect`.

@group(1) @binding(0) var glint_texture: texture_2d<f32>;
@group(1) @binding(1) var glint_sampler: sampler;
@group(1) @binding(2) var<uniform> glint: Glint;

fn glow(layer: Layer, local: vec2<f32>) -> vec3<f32> {
    return layer_glow(textureSample(glint_texture, glint_sampler, layer_uv(layer, local)).rgb);
}

@fragment
fn fs_glint(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(block_textures, block_sampler, in.uv, i32(in.layer));
    if (texel.a < 0.5) {
        discard;
    }
    return vec4<f32>(glow(glint.layers[0], in.uv) + glow(glint.layers[1], in.uv), 0.0);
}
