// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: shader pass.
//!
//! Shows the post-process API of 0.6: a WGSL fragment shader the host runs
//! over the picture of the world (the HUD and menus stay sharp), steered
//! through `umi.params`. It needs no permission: a pass only transforms the
//! picture it is given. See `docs/modding/post.md` for the shader contract and
//! `mods/examples/README.md` for how to build and install it.

use umicraft_modapi::{umicraft_mod, InitCx, Mod, ModError, PreTick, Setting};

/// The shader. Kept in its own file so editors highlight it as WGSL.
const SHADER: &str = include_str!("sepia.wgsl");

/// Priority of the `PreTick` observer; lower values run first.
const PRE_TICK_PRIORITY: i32 = 0;

/// Names of the "Tone" choice, in the order the shader reads them: the index
/// is the value of `umi.params[0].y` (0 grey, 1 sepia).
const TONES: [&str; 2] = ["Grey", "Sepia"];

/// The mod has no state of its own: everything lives in the observer.
struct ShaderPass;

impl Mod for ShaderPass {
    /// Runs once when the client loads the library: register the pass, the two
    /// settings and the observer that feeds them to the shader.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        // The host compiles and validates the shader here; a mistake in it
        // makes `init` fail with the reason in the log.
        let pass = cx.post_pass("sepia", "Sepia", SHADER)?;
        let strength = cx.slider("strength", "Strength", 0.0, 1.0, 0.05, 0.5)?;
        let tone = cx.choice("tone", "Tone", &TONES, 1)?;
        let logger = cx.logger();

        // Settings have no change event, so the mod polls them. A tick is
        // cheap for that: reading two settings and writing one parameter vector
        // costs far less than the frame, and the shader sees a change within a
        // tick. An observer is enough because `set_param` works on any thread.
        cx.observe::<PreTick>(PRE_TICK_PRIORITY, move |_event| {
            if let Err(error) = pass.set_param(0, params(strength, tone)) {
                logger.warn(&format!("could not set the shader parameters: {error}"));
            }
        })?;
        Ok(Self)
    }
}

/// `params[0]`: `x` is the strength, `y` the tone, as the shader reads them.
fn params(strength: Setting<f32>, tone: Setting<u32>) -> [f32; 4] {
    // The choice index is 0 or 1, so the conversion is exact.
    [strength.get(), tone.get() as f32, 0.0, 0.0]
}

umicraft_mod!(ShaderPass);

#[cfg(test)]
mod tests {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    use naga::ShaderStage;

    use super::*;

    /// The host's prelude, copied verbatim from `docs/tasks/f3/README.md`
    /// ("The shader contract"); the host prepends it to the mod's source.
    const PRELUDE: &str = r"
struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;

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
";

    #[test]
    fn composed_shader_validates_with_the_two_entry_points() {
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
        assert_eq!(module.global_variables.len(), 3, "only the prelude's");
    }
}
