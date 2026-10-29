// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Maths shared by the GUI glint (`gui_glint.wgsl`) and the world/hand glint
// (`item_glint.wgsl`), concatenated in front of each at build time because
// WGSL has no includes. The caller passes the sprite-local position (0..1
// over the item sprite) and gets the additive contribution of both scrolling
// layers, which is what the two `blendFunc(SRC_COLOR, ONE)` draws of the
// original renderer produce.

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

// The texture coordinate of one layer for a sprite-local position.
fn layer_uv(layer: Layer, local: vec2<f32>) -> vec2<f32> {
    return mat2x2<f32>(layer.rotation.xy, layer.rotation.zw) * (local * GLINT_SPAN)
        + layer.offset.xy;
}

// The squared tinted glint texel: tint, then the blend multiplies by itself.
fn layer_glow(texel: vec3<f32>) -> vec3<f32> {
    let tinted = texel * GLINT_COLOR;
    return tinted * tinted;
}
