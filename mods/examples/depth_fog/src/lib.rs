// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: depth fog.
//!
//! Shows the scene depth of API 1.1 in a post-process pass: the shader calls
//! `umi_linear_depth` to get the distance of each pixel in blocks and fades what
//! lies beyond a threshold toward a fog colour. It only hides detail, so it gives
//! no advantage, and it needs no permission. See `docs/modding/post.md` for the
//! shader contract and `mods/examples/README.md` for how to build and install it.

use umicraft_modapi::{umicraft_mod, InitCx, Mod, ModError, PreTick, Setting};

/// The shader. Kept in its own file so editors highlight it as WGSL.
const SHADER: &str = include_str!("fog.wgsl");

/// Priority of the `PreTick` observer; lower values run first.
const PRE_TICK_PRIORITY: i32 = 0;

/// The mod has no state of its own: everything lives in the observer.
struct DepthFog;

impl Mod for DepthFog {
    /// Registers the pass, its two settings and the observer that feeds them.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        // The host compiles and validates the shader here; a mistake in it
        // makes `init` fail with the reason in the log.
        let pass = cx.post_pass("fog", "Depth fog", SHADER)?;
        let start = cx.slider("start", "Fog starts at (blocks)", 8.0, 256.0, 8.0, 64.0)?;
        let strength = cx.slider("strength", "Strength", 0.0, 1.0, 0.05, 0.6)?;
        let logger = cx.logger();

        // Settings have no change event, so the mod polls them each tick;
        // `set_param` works on any thread.
        cx.observe::<PreTick>(PRE_TICK_PRIORITY, move |_event| {
            if let Err(error) = pass.set_param(0, params(start, strength)) {
                logger.warn(&format!("could not set the shader parameters: {error}"));
            }
        })?;
        Ok(Self)
    }
}

/// `params[0]`: `x` is the fog start in blocks, `y` the strength.
fn params(start: Setting<f32>, strength: Setting<f32>) -> [f32; 4] {
    [start.get(), strength.get(), 0.0, 0.0]
}

umicraft_mod!(DepthFog);

#[cfg(test)]
mod tests {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    use naga::ShaderStage;

    use super::*;

    /// The host's prelude of API 1.1, copied verbatim from `docs/modding/post.md`;
    /// the host prepends it to the mod's source.
    const PRELUDE: &str = r"
struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
    depth: vec4<f32>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;
@group(0) @binding(3) var umi_depth: texture_depth_2d;

struct UmiVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn umi_vs(@builtin(vertex_index) index: u32) -> UmiVertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: UmiVertexOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn umi_linear_depth(uv: vec2<f32>) -> f32 {
    if umi.depth.z < 0.5 {
        return umi.depth.y;
    }
    let size = vec2<i32>(textureDimensions(umi_depth));
    let texel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
    let d = textureLoad(umi_depth, texel, 0);
    return umi.depth.x * umi.depth.y / (umi.depth.y - d * (umi.depth.y - umi.depth.x));
}
";

    #[test]
    fn composed_shader_validates_and_uses_only_the_prelude_resources() {
        let source = format!("{PRELUDE}\n{SHADER}");
        let module = naga::front::wgsl::parse_str(&source).expect("the shader parses");
        Validator::new(ValidationFlags::all(), Capabilities::default())
            .validate(&module)
            .expect("the shader validates");

        let mut entry_points: Vec<_> = module
            .entry_points
            .iter()
            .map(|entry| (entry.name.as_str(), entry.stage))
            .collect();
        entry_points.sort_by_key(|(name, _)| *name);
        assert_eq!(
            entry_points,
            [
                ("fs_main", ShaderStage::Fragment),
                ("umi_vs", ShaderStage::Vertex)
            ]
        );
        assert_eq!(module.global_variables.len(), 4, "only the prelude's");
    }
}
