// The vanilla lightmap (`EntityRenderer.updateLightmap`, built on the CPU in
// `lightmap.rs`): the colour of a (block, sky) light pair, sampled bilinearly
// like the 16 x 16 `GL_LINEAR` texture of the reference. The levels are in
// `0..=15` and may be fractional (smooth lighting interpolates them).
fn lightmap_sample(block: f32, sky: f32) -> vec3<f32> {
    let b = clamp(block, 0.0, 15.0);
    let s = clamp(sky, 0.0, 15.0);
    let b0 = u32(floor(b));
    let s0 = u32(floor(s));
    let b1 = min(b0 + 1u, 15u);
    let s1 = min(s0 + 1u, 15u);
    let fb = b - f32(b0);
    let fs = s - f32(s0);
    let c00 = camera.lightmap[s0 * 16u + b0].rgb;
    let c10 = camera.lightmap[s0 * 16u + b1].rgb;
    let c01 = camera.lightmap[s1 * 16u + b0].rgb;
    let c11 = camera.lightmap[s1 * 16u + b1].rgb;
    return mix(mix(c00, c10, fb), mix(c01, c11, fb), fs);
}
