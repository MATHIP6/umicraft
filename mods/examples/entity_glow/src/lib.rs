// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: entity glow.
//!
//! Shows the entity mask of API 1.4. A world layer marks the living entities near
//! the player with [`WorldCx::highlight`], always with [`Depth::Tested`], so only
//! the parts the player can see land in the mask; a post pass of the mod draws an
//! outline or a glow around them. The colour comes from the facts of 1.4: blue for
//! a team mate, red for an entity that was just hit, the team's colour or white
//! otherwise. It shows nothing the player does not already see (spec §10). See
//! `docs/modding/post.md` for the mask and `mods/examples/README.md` to build it.

use umicraft_modapi::{
    umicraft_mod, Depth, Entity, InitCx, Mod, ModError, PreTick, Setting, World,
};

/// The shader. Kept in its own file so editors highlight it as WGSL.
const SHADER: &str = include_str!("glow.wgsl");

/// Priority of the `PreTick` observer; lower values run first.
const PRE_TICK_PRIORITY: i32 = 0;

/// The colour of a team mate.
const TEAMMATE: u32 = 0x55_55FF;
/// The colour of an entity flashing from a hit.
const HURT: u32 = 0xFF_5555;
/// The colour of anyone else without a team colour.
const OTHER: u32 = 0xFF_FFFF;

/// The mod has no state of its own: everything lives in its callbacks.
struct EntityGlow;

impl Mod for EntityGlow {
    /// Registers the pass, the settings, the observer that feeds the pass and the
    /// world layer that marks the entities.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let pass = cx.post_pass("glow", "Entity glow", SHADER)?;
        let style = cx.choice("style", "Style", &["Outline", "Glow"], 0)?;
        let radius = cx.slider("radius", "Width (pixels)", 1.0, 8.0, 1.0, 2.0)?;
        let range = cx.slider("range", "Range (blocks)", 4.0, 64.0, 4.0, 24.0)?;
        let logger = cx.logger();

        // Settings have no change event, so the mod pushes them each tick.
        cx.observe::<PreTick>(PRE_TICK_PRIORITY, move |_event| {
            if let Err(error) = pass.set_param(0, params(style, radius)) {
                logger.warn(&format!("could not set the shader parameters: {error}"));
            }
        })?;

        let world = cx.world();
        cx.world_layer("marks", move |frame| mark_nearby(world, range, frame))?;
        Ok(Self)
    }
}

/// Marks the living entities within `range` of the local player.
fn mark_nearby(world: World, range: Setting<f32>, frame: &mut umicraft_modapi::WorldCx<'_>) {
    let Ok(me) = world.local_player(|me| me.render_position()) else {
        return;
    };
    let range = f64::from(range.get());
    // Outside a world there is no snapshot: then nothing is marked.
    let _ = world.snapshot(|entity| {
        if entity.is_living() && within(me, entity.render_position(), range) {
            frame.highlight(entity.id(), color_of(entity), 1.0, Depth::Tested);
        }
        true
    });
}

/// The colour an entity is marked with.
fn color_of(entity: &Entity<'_>) -> u32 {
    mark_color(entity.teammate(), entity.is_hurt(), entity.team_color())
}

/// Blue for a team mate, red while hurt, else the team colour or white.
fn mark_color(teammate: bool, hurt: bool, team_color: Option<u32>) -> u32 {
    if teammate {
        TEAMMATE
    } else if hurt {
        HURT
    } else {
        team_color.unwrap_or(OTHER)
    }
}

/// Whether `b` is at most `range` blocks from `a`.
fn within(a: [f64; 3], b: [f64; 3], range: f64) -> bool {
    let squared: f64 = a.iter().zip(&b).map(|(a, b)| (a - b) * (a - b)).sum();
    squared <= range * range
}

/// `params[0]`: `x` the style, `y` the radius in pixels, `z` the glow strength.
fn params(style: Setting<u32>, radius: Setting<f32>) -> [f32; 4] {
    [style.get() as f32, radius.get(), 1.5, 0.0]
}

umicraft_mod!(EntityGlow);

#[cfg(test)]
mod tests {
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    use super::*;

    /// The host's prelude of API 1.4, copied verbatim from `docs/modding/post.md`;
    /// the host prepends it to the mod's source.
    const PRELUDE: &str = r"
struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
    depth: vec4<f32>,
    mask: vec4<f32>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;
@group(0) @binding(3) var umi_depth: texture_depth_2d;
@group(0) @binding(4) var umi_entity_mask: texture_2d<f32>;

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
    fn the_composed_shader_validates() {
        let source = format!("{PRELUDE}\n{SHADER}");
        let module = naga::front::wgsl::parse_str(&source).expect("the shader parses");
        Validator::new(ValidationFlags::all(), Capabilities::default())
            .validate(&module)
            .expect("the shader validates");
        assert!(module
            .entry_points
            .iter()
            .any(|entry| entry.name == "fs_main"));
    }

    #[test]
    fn team_mates_are_blue_hurt_ones_red_and_the_rest_their_team_colour() {
        assert_eq!(mark_color(true, true, Some(0xFF_AA00)), TEAMMATE);
        assert_eq!(mark_color(false, true, Some(0xFF_AA00)), HURT);
        assert_eq!(mark_color(false, false, Some(0xFF_AA00)), 0xFF_AA00);
        assert_eq!(mark_color(false, false, None), OTHER);
    }

    #[test]
    fn the_range_is_a_sphere_around_the_player() {
        let me = [0.0, 64.0, 0.0];
        assert!(within(me, [3.0, 64.0, 4.0], 5.0));
        assert!(!within(me, [3.0, 64.0, 4.1], 5.0));
        assert!(within(me, me, 0.0));
    }
}
