// Dynamic Lights (OptiFine `DynamicLights.getLightLevel(BlockPos)`): the
// block-light level of a point is raised to the strongest nearby light. The
// reference scales each light linearly from its level at the source to zero
// at 7.5 blocks, and keeps the highest result. A light in water arrives with
// `distance_scale` sqrt(2) (its squared distance is doubled) and a level two
// lower, both decided on the CPU.
//
// The Rust mirror is `block_light_with` in `dynamic_lights.rs`; keep both in
// step. With no light (`light_count` 0, the Off option) the loop never runs.

const DYNAMIC_LIGHT_RADIUS: f32 = 7.5;

struct DynamicLight {
    // Relative to the render origin, like every position the shaders see.
    position: vec3<f32>,
    level: f32,
    distance_scale: f32,
};

// `block` raised by the lights that reach `point` (render-origin space).
fn dynamic_block_light(block: f32, point: vec3<f32>) -> f32 {
    var best = block;
    for (var i = 0u; i < camera.light_count; i++) {
        let light = camera.lights[i];
        let reach = distance(point, light.position) * light.distance_scale;
        if (reach <= DYNAMIC_LIGHT_RADIUS) {
            best = max(best, (1.0 - reach / DYNAMIC_LIGHT_RADIUS) * light.level);
        }
    }
    return best;
}
