// SPDX-License-Identifier: MIT OR Apache-2.0

// The fog falloff shared by the terrain, entity, cloud and sky shaders
// (P7g). A density above zero selects `GL_EXP` (water, lava), as
// `EntityRenderer.setupFog` does; otherwise `GL_LINEAR` between `start` and
// `end`. The result is the share of the fog colour in `0..=1`.
fn fog_amount(distance: f32, start: f32, end: f32, density: f32) -> f32 {
    if (density > 0.0) {
        return 1.0 - exp(-density * distance);
    }
    return clamp((distance - start) / max(end - start, 0.001), 0.0, 1.0);
}
