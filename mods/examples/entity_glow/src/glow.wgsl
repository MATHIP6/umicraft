// The entity mask (API 1.4) turned into an outline or a glow around the marked
// entities. The host prepends its prelude: `umi_entity_mask` holds each marked
// entity's colour where its model is, `umi.mask.x` is 1.0 when something is marked.
//
// umi.params[0]: x the style (0 outline, 1 glow), y the radius in pixels,
// z the strength of the glow.

// Directions sampled around a pixel.
const RAYS: i32 = 16;
// Samples along each direction.
const STEPS: i32 = 3;

@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    let scene = textureSample(umi_scene, umi_sampler, in.uv);
    if umi.mask.x < 0.5 {
        return scene;
    }
    let texel = 1.0 / umi.resolution;
    let radius = max(umi.params[0].y, 1.0);
    // Inside a silhouette nothing changes: the effect lies around it.
    let outside = 1.0 - textureSample(umi_entity_mask, umi_sampler, in.uv).a;
    var nearest = vec4<f32>(0.0);
    var sum = vec4<f32>(0.0);
    var weight = 0.0;
    for (var ray = 0; ray < RAYS; ray++) {
        let angle = f32(ray) * 6.2831853 / f32(RAYS);
        let direction = vec2<f32>(cos(angle), sin(angle)) * texel * radius;
        for (var step = 1; step <= STEPS; step++) {
            let share = f32(step) / f32(STEPS);
            let mark = textureSample(umi_entity_mask, umi_sampler, in.uv + direction * share);
            // Nearer samples weigh more: a soft falloff for the glow.
            let w = 1.0 - share * 0.75;
            sum += mark * w;
            weight += w;
            if mark.a > nearest.a {
                nearest = mark;
            }
        }
    }
    if umi.params[0].x < 0.5 {
        return vec4<f32>(mix(scene.rgb, nearest.rgb, nearest.a * outside), scene.a);
    }
    let glow = sum / max(weight, 0.0001);
    let light = glow.rgb * glow.a * umi.params[0].z * outside;
    return vec4<f32>(scene.rgb + light, scene.a);
}
