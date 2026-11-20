// SPDX-License-Identifier: MIT OR Apache-2.0

// The fragment shader of the `shader_pass` example.
//
// The host puts its prelude in front of this text, so these names exist
// already and are not declared here: `umi_scene` (the picture of the world),
// `umi_sampler` (linear, clamp to edge), `umi` (the uniform, with
// `umi.params`, four vec4 the mod sets) and `UmiVertexOut` (`position`, `uv`).
// A mod supplies `fs_main` and any helpers, nothing else: no new bindings and
// no vertex shader.

// How much of the picture is replaced by the toned one: 0 keeps it as it is.
fn strength() -> f32 {
    return clamp(umi.params[0].x, 0.0, 1.0);
}

// 0 gives a plain grey, 1 gives sepia.
fn tone() -> f32 {
    return clamp(umi.params[0].y, 0.0, 1.0);
}

// Rec. 601 luma: how bright a colour looks to the eye.
fn luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

// Brown-tinted version of a brightness: sepia is brightness times a warm tint.
fn sepia(brightness: f32) -> vec3<f32> {
    return vec3<f32>(brightness) * vec3<f32>(1.07, 0.74, 0.43);
}

@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    let scene = textureSample(umi_scene, umi_sampler, in.uv);
    let brightness = luma(scene.rgb);
    let toned = mix(vec3<f32>(brightness), sepia(brightness), tone());
    // Strength 0 returns exactly the sample, which is why `mix` is used
    // rather than adding a correction.
    return vec4<f32>(mix(scene.rgb, toned, strength()), scene.a);
}
