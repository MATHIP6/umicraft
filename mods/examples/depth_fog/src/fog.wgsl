// SPDX-License-Identifier: MIT OR Apache-2.0

// The fragment shader of the `depth_fog` example.
//
// Besides the names of the shader_pass example, the prelude of API 1.1 gives
// `umi_linear_depth(uv)`: the distance of the scene at `uv` along the view axis,
// in blocks. Pixels of the sky read the far plane (512). The first-person hand
// is squeezed into the nearest part of the depth range, so it reads as very
// near and is never fogged.

// How far past the start the fog is complete, in blocks.
const FOG_RANGE: f32 = 64.0;

// The fog colour: a pale blue-grey. Fixed on purpose, so the pass hides detail
// and cannot paint the scene in a colour that works as a signal.
const FOG_COLOR: vec3<f32> = vec3<f32>(0.70, 0.78, 0.86);

// The distance in blocks where the fog starts, from `umi.params[0].x`.
fn fog_start() -> f32 {
    return max(umi.params[0].x, 0.0);
}

@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    let scene = textureSample(umi_scene, umi_sampler, in.uv);
    let blocks = umi_linear_depth(in.uv);
    // 0 up to the start, then rising to the strength (`params[0].y`) over FOG_RANGE.
    let amount = clamp((blocks - fog_start()) / FOG_RANGE, 0.0, 1.0)
        * clamp(umi.params[0].y, 0.0, 1.0);
    return vec4<f32>(mix(scene.rgb, FOG_COLOR, amount), scene.a);
}
