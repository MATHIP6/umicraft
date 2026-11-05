// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sound type of every 1.8 block id.
//!
//! Source: MCP `net/minecraft/block/Block.java`, `registerBlocks`. A block
//! names its sound with `setStepSound` in its registration chain. A block
//! that names none takes the sound its class sets in the constructor
//! (`BlockLog`, `BlockLeaves`, `BlockCrops`, `BlockDoublePlant`,
//! `BlockDaylightDetector`, `BlockPistonBase`, `BlockPistonExtension`), and
//! `BlockStairs` and `BlockWall` copy the sound of their model block. Every
//! other block keeps the `Block` default, [`STONE`](super::STONE). The table
//! is derived from the reference; the derivation is recorded in
//! `docs/sources.md`.

use super::{
    SoundType, ANVIL, CLOTH, GLASS, GRASS, GRAVEL, LADDER, METAL, PISTON, SAND, SLIME, SNOW, STONE,
    WOOD,
};

/// The sound type of each block id `0..=197`, indexed by the id.
///
/// The length is the size of the 1.8 registry, so a missing or extra row does
/// not compile.
static SOUND_BY_ID: [SoundType; 198] = [
    STONE,  // 0 air
    PISTON, // 1 stone
    GRASS,  // 2 grass
    GRAVEL, // 3 dirt
    PISTON, // 4 cobblestone
    WOOD,   // 5 planks
    GRASS,  // 6 sapling
    PISTON, // 7 bedrock
    STONE,  // 8 flowing_water
    STONE,  // 9 water
    STONE,  // 10 flowing_lava
    STONE,  // 11 lava
    SAND,   // 12 sand
    GRAVEL, // 13 gravel
    PISTON, // 14 gold_ore
    PISTON, // 15 iron_ore
    PISTON, // 16 coal_ore
    WOOD,   // 17 log
    GRASS,  // 18 leaves
    GRASS,  // 19 sponge
    GLASS,  // 20 glass
    PISTON, // 21 lapis_ore
    PISTON, // 22 lapis_block
    PISTON, // 23 dispenser
    PISTON, // 24 sandstone
    STONE,  // 25 noteblock
    WOOD,   // 26 bed
    METAL,  // 27 golden_rail
    METAL,  // 28 detector_rail
    PISTON, // 29 sticky_piston
    STONE,  // 30 web
    GRASS,  // 31 tallgrass
    GRASS,  // 32 deadbush
    PISTON, // 33 piston
    PISTON, // 34 piston_head
    CLOTH,  // 35 wool
    STONE,  // 36 piston_extension
    GRASS,  // 37 yellow_flower
    GRASS,  // 38 red_flower
    GRASS,  // 39 brown_mushroom
    GRASS,  // 40 red_mushroom
    METAL,  // 41 gold_block
    METAL,  // 42 iron_block
    PISTON, // 43 double_stone_slab
    PISTON, // 44 stone_slab
    PISTON, // 45 brick_block
    GRASS,  // 46 tnt
    WOOD,   // 47 bookshelf
    PISTON, // 48 mossy_cobblestone
    PISTON, // 49 obsidian
    WOOD,   // 50 torch
    CLOTH,  // 51 fire
    METAL,  // 52 mob_spawner
    WOOD,   // 53 oak_stairs
    WOOD,   // 54 chest
    STONE,  // 55 redstone_wire
    PISTON, // 56 diamond_ore
    METAL,  // 57 diamond_block
    WOOD,   // 58 crafting_table
    GRASS,  // 59 wheat
    GRAVEL, // 60 farmland
    PISTON, // 61 furnace
    PISTON, // 62 lit_furnace
    WOOD,   // 63 standing_sign
    WOOD,   // 64 wooden_door
    LADDER, // 65 ladder
    METAL,  // 66 rail
    PISTON, // 67 stone_stairs
    WOOD,   // 68 wall_sign
    WOOD,   // 69 lever
    PISTON, // 70 stone_pressure_plate
    METAL,  // 71 iron_door
    WOOD,   // 72 wooden_pressure_plate
    PISTON, // 73 redstone_ore
    PISTON, // 74 lit_redstone_ore
    WOOD,   // 75 unlit_redstone_torch
    WOOD,   // 76 redstone_torch
    PISTON, // 77 stone_button
    SNOW,   // 78 snow_layer
    GLASS,  // 79 ice
    SNOW,   // 80 snow
    CLOTH,  // 81 cactus
    GRAVEL, // 82 clay
    GRASS,  // 83 reeds
    PISTON, // 84 jukebox
    WOOD,   // 85 fence
    WOOD,   // 86 pumpkin
    PISTON, // 87 netherrack
    SAND,   // 88 soul_sand
    GLASS,  // 89 glowstone
    GLASS,  // 90 portal
    WOOD,   // 91 lit_pumpkin
    CLOTH,  // 92 cake
    WOOD,   // 93 unpowered_repeater
    WOOD,   // 94 powered_repeater
    GLASS,  // 95 stained_glass
    WOOD,   // 96 trapdoor
    STONE,  // 97 monster_egg
    PISTON, // 98 stonebrick
    WOOD,   // 99 brown_mushroom_block
    WOOD,   // 100 red_mushroom_block
    METAL,  // 101 iron_bars
    GLASS,  // 102 glass_pane
    WOOD,   // 103 melon_block
    WOOD,   // 104 pumpkin_stem
    WOOD,   // 105 melon_stem
    GRASS,  // 106 vine
    WOOD,   // 107 fence_gate
    PISTON, // 108 brick_stairs
    PISTON, // 109 stone_brick_stairs
    GRASS,  // 110 mycelium
    GRASS,  // 111 waterlily
    PISTON, // 112 nether_brick
    PISTON, // 113 nether_brick_fence
    PISTON, // 114 nether_brick_stairs
    STONE,  // 115 nether_wart
    STONE,  // 116 enchanting_table
    STONE,  // 117 brewing_stand
    STONE,  // 118 cauldron
    STONE,  // 119 end_portal
    GLASS,  // 120 end_portal_frame
    PISTON, // 121 end_stone
    PISTON, // 122 dragon_egg
    GLASS,  // 123 redstone_lamp
    GLASS,  // 124 lit_redstone_lamp
    WOOD,   // 125 double_wooden_slab
    WOOD,   // 126 wooden_slab
    WOOD,   // 127 cocoa
    PISTON, // 128 sandstone_stairs
    PISTON, // 129 emerald_ore
    PISTON, // 130 ender_chest
    STONE,  // 131 tripwire_hook
    STONE,  // 132 tripwire
    METAL,  // 133 emerald_block
    WOOD,   // 134 spruce_stairs
    WOOD,   // 135 birch_stairs
    WOOD,   // 136 jungle_stairs
    STONE,  // 137 command_block
    STONE,  // 138 beacon
    PISTON, // 139 cobblestone_wall
    STONE,  // 140 flower_pot
    STONE,  // 141 carrots
    STONE,  // 142 potatoes
    WOOD,   // 143 wooden_button
    PISTON, // 144 skull
    ANVIL,  // 145 anvil
    WOOD,   // 146 trapped_chest
    WOOD,   // 147 light_weighted_pressure_plate
    WOOD,   // 148 heavy_weighted_pressure_plate
    WOOD,   // 149 unpowered_comparator
    WOOD,   // 150 powered_comparator
    WOOD,   // 151 daylight_detector
    METAL,  // 152 redstone_block
    PISTON, // 153 quartz_ore
    METAL,  // 154 hopper
    PISTON, // 155 quartz_block
    PISTON, // 156 quartz_stairs
    METAL,  // 157 activator_rail
    PISTON, // 158 dropper
    PISTON, // 159 stained_hardened_clay
    GLASS,  // 160 stained_glass_pane
    GRASS,  // 161 leaves2
    WOOD,   // 162 log2
    WOOD,   // 163 acacia_stairs
    WOOD,   // 164 dark_oak_stairs
    SLIME,  // 165 slime
    STONE,  // 166 barrier
    METAL,  // 167 iron_trapdoor
    PISTON, // 168 prismarine
    GLASS,  // 169 sea_lantern
    GRASS,  // 170 hay_block
    CLOTH,  // 171 carpet
    PISTON, // 172 hardened_clay
    PISTON, // 173 coal_block
    GLASS,  // 174 packed_ice
    GRASS,  // 175 double_plant
    WOOD,   // 176 standing_banner
    WOOD,   // 177 wall_banner
    WOOD,   // 178 daylight_detector_inverted
    PISTON, // 179 red_sandstone
    PISTON, // 180 red_sandstone_stairs
    PISTON, // 181 double_stone_slab2
    PISTON, // 182 stone_slab2
    WOOD,   // 183 spruce_fence_gate
    WOOD,   // 184 birch_fence_gate
    WOOD,   // 185 jungle_fence_gate
    WOOD,   // 186 dark_oak_fence_gate
    WOOD,   // 187 acacia_fence_gate
    WOOD,   // 188 spruce_fence
    WOOD,   // 189 birch_fence
    WOOD,   // 190 jungle_fence
    WOOD,   // 191 dark_oak_fence
    WOOD,   // 192 acacia_fence
    WOOD,   // 193 spruce_door
    WOOD,   // 194 birch_door
    WOOD,   // 195 jungle_door
    WOOD,   // 196 acacia_door
    WOOD,   // 197 dark_oak_door
];

/// The sound type of a block id.
///
/// Ids above 197 are not in the 1.8 registry. They get [`STONE`](super::STONE),
/// the default of `Block`, so a corrupt or unknown id never panics.
///
/// ```
/// use umicraft_game::sound::{sound_type, GRASS};
///
/// assert_eq!(sound_type(2), GRASS);
/// ```
#[must_use]
pub fn sound_type(block_id: u16) -> SoundType {
    SOUND_BY_ID
        .get(usize::from(block_id))
        .copied()
        .unwrap_or(STONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name of the event a sound plays, as a string.
    fn event_names(sound: &SoundType) -> [String; 3] {
        [
            sound.break_sound().to_string(),
            sound.step_sound().to_string(),
            sound.place_sound().to_string(),
        ]
    }

    #[test]
    fn grass_and_leaves_use_the_grass_sound() {
        assert_eq!(sound_type(2), GRASS);
        assert_eq!(
            event_names(&sound_type(2)),
            ["dig.grass", "step.grass", "dig.grass"]
        );
        // Leaves get the grass sound from `BlockLeaves`' constructor.
        assert_eq!(sound_type(18), GRASS);
        assert_eq!(sound_type(161), GRASS);
    }

    #[test]
    fn stone_and_cobblestone_use_the_stone_sound() {
        assert_eq!(sound_type(1), PISTON);
        assert_eq!(
            event_names(&sound_type(1)),
            ["dig.stone", "step.stone", "dig.stone"]
        );
        assert_eq!(sound_type(4), PISTON);
        assert_eq!(sound_type(48), PISTON);
    }

    #[test]
    fn water_and_air_keep_the_default_stone_sound() {
        assert_eq!(sound_type(0), STONE);
        assert_eq!(sound_type(8), STONE);
        assert_eq!(sound_type(9), STONE);
    }

    #[test]
    fn planks_and_logs_use_the_wood_sound() {
        assert_eq!(sound_type(5), WOOD);
        assert_eq!(sound_type(17), WOOD);
        assert_eq!(sound_type(162), WOOD);
    }

    #[test]
    fn gravel_and_dirt_use_the_gravel_sound() {
        assert_eq!(sound_type(3), GRAVEL);
        assert_eq!(sound_type(13), GRAVEL);
        assert_eq!(
            event_names(&sound_type(13)),
            ["dig.gravel", "step.gravel", "dig.gravel"]
        );
    }

    #[test]
    fn sand_and_soul_sand_use_the_sand_sound() {
        assert_eq!(sound_type(12), SAND);
        assert_eq!(sound_type(88), SAND);
    }

    #[test]
    fn glass_and_ice_use_the_glass_sound() {
        assert_eq!(sound_type(20), GLASS);
        assert_eq!(
            event_names(&sound_type(20)),
            ["dig.glass", "step.stone", "step.stone"]
        );
        assert_eq!(sound_type(79), GLASS);
    }

    #[test]
    fn wool_uses_the_cloth_sound() {
        assert_eq!(sound_type(35), CLOTH);
        assert_eq!(
            event_names(&sound_type(35)),
            ["dig.cloth", "step.cloth", "dig.cloth"]
        );
    }

    #[test]
    fn snow_layer_and_snow_block_use_the_snow_sound() {
        assert_eq!(sound_type(78), SNOW);
        assert_eq!(sound_type(80), SNOW);
        assert_eq!(
            event_names(&sound_type(80)),
            ["dig.snow", "step.snow", "dig.snow"]
        );
    }

    #[test]
    fn ladders_break_and_place_with_the_wood_event() {
        assert_eq!(sound_type(65), LADDER);
        assert_eq!(
            event_names(&sound_type(65)),
            ["dig.wood", "step.ladder", "dig.wood"]
        );
    }

    #[test]
    fn anvil_lands_with_its_own_event_and_is_quiet() {
        assert_eq!(sound_type(145), ANVIL);
        assert_eq!(
            event_names(&sound_type(145)),
            ["dig.stone", "step.anvil", "random.anvil_land"]
        );
    }

    #[test]
    fn slime_block_uses_the_slime_sounds() {
        assert_eq!(sound_type(165), SLIME);
        assert_eq!(
            event_names(&sound_type(165)),
            ["mob.slime.big", "mob.slime.small", "mob.slime.big"]
        );
    }

    #[test]
    fn iron_and_gold_blocks_use_the_metal_sound() {
        assert_eq!(sound_type(42), METAL);
        assert_eq!(sound_type(41), METAL);
        assert_eq!(
            event_names(&sound_type(42)),
            ["dig.stone", "step.stone", "dig.stone"]
        );
    }

    #[test]
    fn wood_stairs_and_stone_stairs_copy_their_model_block() {
        assert_eq!(sound_type(53), WOOD);
        assert_eq!(sound_type(67), PISTON);
        assert_eq!(sound_type(114), PISTON);
    }

    #[test]
    fn cobblestone_wall_copies_the_cobblestone_sound() {
        assert_eq!(sound_type(139), PISTON);
    }

    #[test]
    fn crops_use_the_grass_sound_and_farmland_the_gravel_sound() {
        assert_eq!(sound_type(59), GRASS);
        assert_eq!(sound_type(60), GRAVEL);
    }

    #[test]
    fn pistons_use_the_piston_sound() {
        assert_eq!(sound_type(29), PISTON);
        assert_eq!(sound_type(33), PISTON);
        assert_eq!(sound_type(34), PISTON);
    }

    #[test]
    fn redstone_block_is_metal_and_the_daylight_detector_is_wood() {
        assert_eq!(sound_type(152), METAL);
        assert_eq!(sound_type(151), WOOD);
    }

    #[test]
    fn every_registry_id_has_a_valid_sound_type() {
        for id in 0..=197 {
            let sound = sound_type(id);
            assert!(!sound.name.is_empty(), "id {id} has no material name");
            assert!(sound.volume > 0.0, "id {id} has no volume");
            assert!(sound.pitch > 0.0, "id {id} has no pitch");
            let names = event_names(&sound);
            assert!(
                names.iter().all(|event| !event.is_empty()),
                "id {id} has an empty event name"
            );
        }
    }

    #[test]
    fn an_id_outside_the_registry_gets_the_default_sound() {
        assert_eq!(sound_type(198), STONE);
        assert_eq!(sound_type(u16::MAX), STONE);
    }
}
