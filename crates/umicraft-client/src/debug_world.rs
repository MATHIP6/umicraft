// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world facts the F3 screen prints (P9-h): entity and particle counts,
//! the chunk cache, biome, light, local difficulty and the block under the
//! crosshair.
//!
//! They live on the game thread (the world and the entities are there), so the
//! tick collects them into the frame and `debug_f3_view` only maps them to the
//! F3 input. Sources: MCP `GuiOverlayDebug.call` and
//! `getDebugInfoRight`; see `docs/sources.md`.

use umicraft_game::debug_f3::biomes::biome_name;
use umicraft_game::debug_f3::difficulty::local_difficulty;
use umicraft_game::debug_f3::{DebugLocation, LookedAtBlock, PropertyValue};
use umicraft_model::registry::definition;
use umicraft_world::{BlockPos, BlockState, World};

/// What the game thread knows for F3 and the render thread does not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DebugWorld {
    /// All loaded entities (`World.getDebugLoadedEntities`).
    pub loaded_entities: usize,
    /// The live particles (`EffectRenderer.getStatistics`).
    pub particles: usize,
    /// The columns in the chunk cache (`MultiplayerChunkCache` sizes).
    pub chunk_cache: (usize, usize),
    /// The feet position of the player (`boundingBox.minY` is the feet y).
    pub feet: [f64; 3],
    /// Biome and light at the feet; `None` while the column is not loaded.
    pub location: Option<DebugLocation>,
    /// The block under the crosshair.
    pub looking_at: Option<LookedAtBlock>,
}

/// The inputs of [`collect`] that are not in the world.
#[derive(Debug, Clone, Copy)]
pub struct DebugInputs {
    /// Entities in the store.
    pub loaded_entities: usize,
    /// Live particles.
    pub particles: usize,
    /// The feet position of the player.
    pub feet: [f64; 3],
    /// The block the crosshair points at.
    pub target: Option<BlockPos>,
    /// The server difficulty, 0 peaceful to 3 hard.
    pub difficulty: u8,
    /// The time of day in ticks.
    pub world_time: i64,
}

/// Gathers the facts of one tick.
#[must_use]
pub fn collect(world: &World, inputs: &DebugInputs) -> DebugWorld {
    let feet = BlockPos::new(
        inputs.feet[0].floor() as i32,
        inputs.feet[1].floor() as i32,
        inputs.feet[2].floor() as i32,
    );
    let columns = world.chunk_count();
    DebugWorld {
        loaded_entities: inputs.loaded_entities,
        particles: inputs.particles,
        chunk_cache: (columns, columns),
        feet: inputs.feet,
        location: location(world, feet, inputs),
        looking_at: inputs.target.map(|pos| looked_at(pos, world.block(pos))),
    }
}

/// Biome and light at `pos`, `None` while the column is not loaded.
fn location(world: &World, pos: BlockPos, inputs: &DebugInputs) -> Option<DebugLocation> {
    let biome = biome_name(world.biome(pos)?)?;
    let light = world.light(pos);
    Some(DebugLocation {
        biome,
        light: light.combined(),
        sky_light: light.sky,
        block_light: light.block,
        local_difficulty: local_difficulty(inputs.difficulty, inputs.world_time),
        day: (inputs.world_time.max(0) / 24_000) as u64,
    })
}

/// The registry name and properties of the looked-at block.
///
/// The properties come from the model registry's variant key
/// (`facing=north,half=upper`), which lists the actual-state properties in the
/// reference's alphabetical order. A key of `normal` has none, except that a
/// document named differently from the block (`granite` for `stone`) is the
/// `variant` property. Connections the world would add (fences, panes) are
/// not computed: this is an approximation of `getActualState`.
fn looked_at(pos: BlockPos, state: BlockState) -> LookedAtBlock {
    let definition = definition(state.id());
    let name = definition.map_or("air", |definition| definition.name);
    let rule = definition.and_then(|definition| definition.state(state.metadata()));
    let mut properties = Vec::new();
    if let Some(rule) = rule {
        if rule.variant == "normal" {
            if rule.blockstate != name {
                properties.push((
                    "variant".to_owned(),
                    PropertyValue::Text(rule.blockstate.to_owned()),
                ));
            }
        } else {
            properties.extend(rule.variant.split(',').filter_map(property));
        }
    }
    LookedAtBlock {
        pos: [pos.x, pos.y, pos.z],
        name: format!("minecraft:{name}"),
        properties,
    }
}

/// One `key=value` pair of a variant key.
fn property(pair: &str) -> Option<(String, PropertyValue)> {
    let (key, value) = pair.split_once('=')?;
    let value = match value {
        "true" => PropertyValue::Bool(true),
        "false" => PropertyValue::Bool(false),
        other => PropertyValue::Text(other.to_owned()),
    };
    Some((key.to_owned(), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(state: BlockState) -> LookedAtBlock {
        looked_at(BlockPos::new(1, 2, 3), state)
    }

    #[test]
    fn a_lever_lists_its_properties_in_order() {
        let block = at(BlockState::new(69, 1));
        assert_eq!(block.name, "minecraft:lever");
        assert_eq!(block.pos, [1, 2, 3]);
        assert_eq!(
            block.properties,
            [
                ("facing".to_owned(), PropertyValue::Text("east".to_owned())),
                ("powered".to_owned(), PropertyValue::Bool(false)),
            ]
        );
    }

    #[test]
    fn granite_is_the_variant_of_stone() {
        let block = at(BlockState::new(1, 1));
        assert_eq!(block.name, "minecraft:stone");
        assert_eq!(
            block.properties,
            [(
                "variant".to_owned(),
                PropertyValue::Text("granite".to_owned())
            )]
        );
    }

    #[test]
    fn a_plain_block_has_no_properties() {
        assert!(at(BlockState::new(1, 0)).properties.is_empty());
    }

    #[test]
    fn air_is_named_and_empty() {
        let block = at(BlockState::AIR);
        assert_eq!(block.name, "minecraft:air");
        assert!(block.properties.is_empty());
    }
}
