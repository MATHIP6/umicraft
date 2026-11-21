// SPDX-License-Identifier: MIT OR Apache-2.0

// The fog falloff shared by the terrain, entity, cloud and sky shaders
// (P7g). A density above zero selects `GL_EXP` (water, lava), as
// `EntityRenderer.setupFog` does; otherwise `GL_LINEAR` between `start` and
// `end`. The result is the share of the fog colour in `0..=1`.
//
// `fog_distance` picks what the falloff is measured on: the distance to the
// eye (`radial`) or, when `planar` is set, the depth along the view. A
// fragment's `@builtin(position).w` is `1 / w_clip`, and `w_clip` is that
// depth for a perspective projection.
fn fog_distance(radial: f32, inv_w: f32, planar: f32) -> f32 {
    if (planar > 0.5) {
        return 1.0 / max(inv_w, 0.000001);
    }
    return radial;
}

fn fog_amount(distance: f32, start: f32, end: f32, density: f32) -> f32 {
    if (density > 0.0) {
        return 1.0 - exp(-density * distance);
    }
    return clamp((distance - start) / max(end - start, 0.001), 0.0, 1.0);
}
