// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block hardness and material of the 1.8 blocks, by block id.
//!
//! Generated from the `Block.registerBlocks` table of the reference (MCP
//! `net/minecraft/block/Block.java`, `setHardness` / `setBlockUnbreakable`
//! and the constructor material of each block class), see `docs/sources.md`.

/// A block material (MCP `net/minecraft/block/material/Material`), reduced to
/// the ones that decide how a block is dug.
/// Some materials name no block of the table (air has its own id check), but
/// the tool rules match on them.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Material {
    /// MCP `Material.air`.
    Air,
    /// MCP `Material.anvil`.
    Anvil,
    /// MCP `Material.barrier`.
    Barrier,
    /// MCP `Material.cactus`.
    Cactus,
    /// MCP `Material.cake`.
    Cake,
    /// MCP `Material.carpet`.
    Carpet,
    /// MCP `Material.circuits`.
    Circuits,
    /// MCP `Material.clay`.
    Clay,
    /// MCP `Material.cloth`.
    Cloth,
    /// MCP `Material.coral`.
    Coral,
    /// MCP `Material.craftedSnow`.
    CraftedSnow,
    /// MCP `Material.dragonEgg`.
    DragonEgg,
    /// MCP `Material.fire`.
    Fire,
    /// MCP `Material.glass`.
    Glass,
    /// MCP `Material.gourd`.
    Gourd,
    /// MCP `Material.grass`.
    Grass,
    /// MCP `Material.ground`.
    Ground,
    /// MCP `Material.ice`.
    Ice,
    /// MCP `Material.iron`.
    Iron,
    /// MCP `Material.lava`.
    Lava,
    /// MCP `Material.leaves`.
    Leaves,
    /// MCP `Material.packedIce`.
    PackedIce,
    /// MCP `Material.piston`.
    Piston,
    /// MCP `Material.plants`.
    Plants,
    /// MCP `Material.portal`.
    Portal,
    /// MCP `Material.redstoneLight`.
    RedstoneLight,
    /// MCP `Material.rock`.
    Rock,
    /// MCP `Material.sand`.
    Sand,
    /// MCP `Material.snow`.
    Snow,
    /// MCP `Material.sponge`.
    Sponge,
    /// MCP `Material.tnt`.
    Tnt,
    /// MCP `Material.vine`.
    Vine,
    /// MCP `Material.water`.
    Water,
    /// MCP `Material.web`.
    Web,
    /// MCP `Material.wood`.
    Wood,
    /// A block without a table entry.
    #[default]
    Other,
}

/// The digging data of one block.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlockData {
    /// MCP `Block.blockHardness`; negative means unbreakable.
    pub hardness: f32,
    /// The block material.
    pub material: Material,
}

const NONE: BlockData = BlockData {
    hardness: 0.0,
    material: Material::Other,
};

/// The data of every block id below 256; unknown ids are [`Material::Other`]
/// with hardness 0.
pub(crate) static BLOCKS: [BlockData; 256] = {
    let mut table = [NONE; 256];
    table[1] = BlockData {
        hardness: 1.5,
        material: Material::Rock,
    }; // stone
    table[2] = BlockData {
        hardness: 0.6,
        material: Material::Grass,
    }; // grass
    table[3] = BlockData {
        hardness: 0.5,
        material: Material::Ground,
    }; // dirt
    table[4] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // cobblestone
    table[5] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // planks
    table[6] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // sapling
    table[7] = BlockData {
        hardness: -1.0,
        material: Material::Rock,
    }; // bedrock
    table[8] = BlockData {
        hardness: 100.0,
        material: Material::Water,
    }; // flowing_water
    table[9] = BlockData {
        hardness: 100.0,
        material: Material::Water,
    }; // water
    table[10] = BlockData {
        hardness: 100.0,
        material: Material::Lava,
    }; // flowing_lava
    table[11] = BlockData {
        hardness: 100.0,
        material: Material::Lava,
    }; // lava
    table[12] = BlockData {
        hardness: 0.5,
        material: Material::Sand,
    }; // sand
    table[13] = BlockData {
        hardness: 0.6,
        material: Material::Sand,
    }; // gravel
    table[14] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // gold_ore
    table[15] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // iron_ore
    table[16] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // coal_ore
    table[17] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // log
    table[18] = BlockData {
        hardness: 0.2,
        material: Material::Leaves,
    }; // leaves
    table[19] = BlockData {
        hardness: 0.6,
        material: Material::Sponge,
    }; // sponge
    table[20] = BlockData {
        hardness: 0.3,
        material: Material::Glass,
    }; // glass
    table[21] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // lapis_ore
    table[22] = BlockData {
        hardness: 3.0,
        material: Material::Iron,
    }; // lapis_block
    table[23] = BlockData {
        hardness: 3.5,
        material: Material::Rock,
    }; // dispenser
    table[24] = BlockData {
        hardness: 0.8,
        material: Material::Rock,
    }; // sandstone
    table[25] = BlockData {
        hardness: 0.8,
        material: Material::Wood,
    }; // noteblock
    table[26] = BlockData {
        hardness: 0.2,
        material: Material::Cloth,
    }; // bed
    table[27] = BlockData {
        hardness: 0.7,
        material: Material::Circuits,
    }; // golden_rail
    table[28] = BlockData {
        hardness: 0.7,
        material: Material::Circuits,
    }; // detector_rail
    table[29] = BlockData {
        hardness: 0.5,
        material: Material::Piston,
    }; // sticky_piston
    table[30] = BlockData {
        hardness: 4.0,
        material: Material::Web,
    }; // web
    table[31] = BlockData {
        hardness: 0.0,
        material: Material::Vine,
    }; // tallgrass
    table[32] = BlockData {
        hardness: 0.0,
        material: Material::Vine,
    }; // deadbush
    table[33] = BlockData {
        hardness: 0.5,
        material: Material::Piston,
    }; // piston
    table[34] = BlockData {
        hardness: 0.5,
        material: Material::Piston,
    }; // piston_head
    table[35] = BlockData {
        hardness: 0.8,
        material: Material::Cloth,
    }; // wool
    table[36] = BlockData {
        hardness: -1.0,
        material: Material::Piston,
    }; // piston_extension
    table[37] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // yellow_flower
    table[38] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // red_flower
    table[39] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // brown_mushroom
    table[40] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // red_mushroom
    table[41] = BlockData {
        hardness: 3.0,
        material: Material::Iron,
    }; // gold_block
    table[42] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // iron_block
    table[43] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // double_stone_slab
    table[44] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // stone_slab
    table[45] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // brick_block
    table[46] = BlockData {
        hardness: 0.0,
        material: Material::Tnt,
    }; // tnt
    table[47] = BlockData {
        hardness: 1.5,
        material: Material::Wood,
    }; // bookshelf
    table[48] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // mossy_cobblestone
    table[49] = BlockData {
        hardness: 50.0,
        material: Material::Rock,
    }; // obsidian
    table[50] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // torch
    table[51] = BlockData {
        hardness: 0.0,
        material: Material::Fire,
    }; // fire
    table[52] = BlockData {
        hardness: 5.0,
        material: Material::Rock,
    }; // mob_spawner
    table[53] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // oak_stairs
    table[54] = BlockData {
        hardness: 2.5,
        material: Material::Wood,
    }; // chest
    table[55] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // redstone_wire
    table[56] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // diamond_ore
    table[57] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // diamond_block
    table[58] = BlockData {
        hardness: 2.5,
        material: Material::Wood,
    }; // crafting_table
    table[59] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // wheat
    table[60] = BlockData {
        hardness: 0.6,
        material: Material::Ground,
    }; // farmland
    table[61] = BlockData {
        hardness: 3.5,
        material: Material::Rock,
    }; // furnace
    table[62] = BlockData {
        hardness: 3.5,
        material: Material::Rock,
    }; // lit_furnace
    table[63] = BlockData {
        hardness: 1.0,
        material: Material::Wood,
    }; // standing_sign
    table[64] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // wooden_door
    table[65] = BlockData {
        hardness: 0.4,
        material: Material::Circuits,
    }; // ladder
    table[66] = BlockData {
        hardness: 0.7,
        material: Material::Circuits,
    }; // rail
    table[67] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // stone_stairs
    table[68] = BlockData {
        hardness: 1.0,
        material: Material::Wood,
    }; // wall_sign
    table[69] = BlockData {
        hardness: 0.5,
        material: Material::Circuits,
    }; // lever
    table[70] = BlockData {
        hardness: 0.5,
        material: Material::Rock,
    }; // stone_pressure_plate
    table[71] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // iron_door
    table[72] = BlockData {
        hardness: 0.5,
        material: Material::Wood,
    }; // wooden_pressure_plate
    table[73] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // redstone_ore
    table[74] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // lit_redstone_ore
    table[75] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // unlit_redstone_torch
    table[76] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // redstone_torch
    table[77] = BlockData {
        hardness: 0.5,
        material: Material::Circuits,
    }; // stone_button
    table[78] = BlockData {
        hardness: 0.1,
        material: Material::Snow,
    }; // snow_layer
    table[79] = BlockData {
        hardness: 0.5,
        material: Material::Ice,
    }; // ice
    table[80] = BlockData {
        hardness: 0.2,
        material: Material::CraftedSnow,
    }; // snow
    table[81] = BlockData {
        hardness: 0.4,
        material: Material::Cactus,
    }; // cactus
    table[82] = BlockData {
        hardness: 0.6,
        material: Material::Clay,
    }; // clay
    table[83] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // reeds
    table[84] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // jukebox
    table[85] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // fence
    table[86] = BlockData {
        hardness: 1.0,
        material: Material::Gourd,
    }; // pumpkin
    table[87] = BlockData {
        hardness: 0.4,
        material: Material::Rock,
    }; // netherrack
    table[88] = BlockData {
        hardness: 0.5,
        material: Material::Sand,
    }; // soul_sand
    table[89] = BlockData {
        hardness: 0.3,
        material: Material::Glass,
    }; // glowstone
    table[90] = BlockData {
        hardness: -1.0,
        material: Material::Portal,
    }; // portal
    table[91] = BlockData {
        hardness: 1.0,
        material: Material::Gourd,
    }; // lit_pumpkin
    table[92] = BlockData {
        hardness: 0.5,
        material: Material::Cake,
    }; // cake
    table[93] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // unpowered_repeater
    table[94] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // powered_repeater
    table[95] = BlockData {
        hardness: 0.3,
        material: Material::Glass,
    }; // stained_glass
    table[96] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // trapdoor
    table[97] = BlockData {
        hardness: 0.75,
        material: Material::Clay,
    }; // monster_egg
    table[98] = BlockData {
        hardness: 1.5,
        material: Material::Rock,
    }; // stonebrick
    table[99] = BlockData {
        hardness: 0.2,
        material: Material::Wood,
    }; // brown_mushroom_block
    table[100] = BlockData {
        hardness: 0.2,
        material: Material::Wood,
    }; // red_mushroom_block
    table[101] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // iron_bars
    table[102] = BlockData {
        hardness: 0.3,
        material: Material::Glass,
    }; // glass_pane
    table[103] = BlockData {
        hardness: 1.0,
        material: Material::Gourd,
    }; // melon_block
    table[104] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // pumpkin_stem
    table[105] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // melon_stem
    table[106] = BlockData {
        hardness: 0.2,
        material: Material::Vine,
    }; // vine
    table[107] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // fence_gate
    table[108] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // brick_stairs
    table[109] = BlockData {
        hardness: 1.5,
        material: Material::Rock,
    }; // stone_brick_stairs
    table[110] = BlockData {
        hardness: 0.6,
        material: Material::Grass,
    }; // mycelium
    table[111] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // waterlily
    table[112] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // nether_brick
    table[113] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // nether_brick_fence
    table[114] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // nether_brick_stairs
    table[115] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // nether_wart
    table[116] = BlockData {
        hardness: 5.0,
        material: Material::Rock,
    }; // enchanting_table
    table[117] = BlockData {
        hardness: 0.5,
        material: Material::Iron,
    }; // brewing_stand
    table[118] = BlockData {
        hardness: 2.0,
        material: Material::Iron,
    }; // cauldron
    table[119] = BlockData {
        hardness: -1.0,
        material: Material::Portal,
    }; // end_portal
    table[120] = BlockData {
        hardness: -1.0,
        material: Material::Rock,
    }; // end_portal_frame
    table[121] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // end_stone
    table[122] = BlockData {
        hardness: 3.0,
        material: Material::DragonEgg,
    }; // dragon_egg
    table[123] = BlockData {
        hardness: 0.3,
        material: Material::RedstoneLight,
    }; // redstone_lamp
    table[124] = BlockData {
        hardness: 0.3,
        material: Material::RedstoneLight,
    }; // lit_redstone_lamp
    table[125] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // double_wooden_slab
    table[126] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // wooden_slab
    table[127] = BlockData {
        hardness: 0.2,
        material: Material::Plants,
    }; // cocoa
    table[128] = BlockData {
        hardness: 0.8,
        material: Material::Rock,
    }; // sandstone_stairs
    table[129] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // emerald_ore
    table[130] = BlockData {
        hardness: 22.5,
        material: Material::Rock,
    }; // ender_chest
    table[131] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // tripwire_hook
    table[132] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // tripwire
    table[133] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // emerald_block
    table[134] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // spruce_stairs
    table[135] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // birch_stairs
    table[136] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // jungle_stairs
    table[137] = BlockData {
        hardness: -1.0,
        material: Material::Iron,
    }; // command_block
    table[138] = BlockData {
        hardness: 3.0,
        material: Material::Glass,
    }; // beacon
    table[139] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // cobblestone_wall
    table[140] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // flower_pot
    table[141] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // carrots
    table[142] = BlockData {
        hardness: 0.0,
        material: Material::Plants,
    }; // potatoes
    table[143] = BlockData {
        hardness: 0.5,
        material: Material::Circuits,
    }; // wooden_button
    table[144] = BlockData {
        hardness: 1.0,
        material: Material::Circuits,
    }; // skull
    table[145] = BlockData {
        hardness: 5.0,
        material: Material::Anvil,
    }; // anvil
    table[146] = BlockData {
        hardness: 2.5,
        material: Material::Wood,
    }; // trapped_chest
    table[147] = BlockData {
        hardness: 0.5,
        material: Material::Iron,
    }; // light_weighted_pressure_plate
    table[148] = BlockData {
        hardness: 0.5,
        material: Material::Iron,
    }; // heavy_weighted_pressure_plate
    table[149] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // unpowered_comparator
    table[150] = BlockData {
        hardness: 0.0,
        material: Material::Circuits,
    }; // powered_comparator
    table[151] = BlockData {
        hardness: 0.2,
        material: Material::Wood,
    }; // daylight_detector
    table[152] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // redstone_block
    table[153] = BlockData {
        hardness: 3.0,
        material: Material::Rock,
    }; // quartz_ore
    table[154] = BlockData {
        hardness: 3.0,
        material: Material::Iron,
    }; // hopper
    table[155] = BlockData {
        hardness: 0.8,
        material: Material::Rock,
    }; // quartz_block
    table[156] = BlockData {
        hardness: 0.8,
        material: Material::Rock,
    }; // quartz_stairs
    table[157] = BlockData {
        hardness: 0.7,
        material: Material::Circuits,
    }; // activator_rail
    table[158] = BlockData {
        hardness: 3.5,
        material: Material::Rock,
    }; // dropper
    table[159] = BlockData {
        hardness: 1.25,
        material: Material::Rock,
    }; // stained_hardened_clay
    table[160] = BlockData {
        hardness: 0.3,
        material: Material::Glass,
    }; // stained_glass_pane
    table[161] = BlockData {
        hardness: 0.2,
        material: Material::Leaves,
    }; // leaves2
    table[162] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // log2
    table[163] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // acacia_stairs
    table[164] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // dark_oak_stairs
    table[165] = BlockData {
        hardness: 0.0,
        material: Material::Clay,
    }; // slime
    table[166] = BlockData {
        hardness: -1.0,
        material: Material::Barrier,
    }; // barrier
    table[167] = BlockData {
        hardness: 5.0,
        material: Material::Iron,
    }; // iron_trapdoor
    table[168] = BlockData {
        hardness: 1.5,
        material: Material::Rock,
    }; // prismarine
    table[169] = BlockData {
        hardness: 0.3,
        material: Material::Glass,
    }; // sea_lantern
    table[170] = BlockData {
        hardness: 0.5,
        material: Material::Grass,
    }; // hay_block
    table[171] = BlockData {
        hardness: 0.1,
        material: Material::Carpet,
    }; // carpet
    table[172] = BlockData {
        hardness: 1.25,
        material: Material::Rock,
    }; // hardened_clay
    table[173] = BlockData {
        hardness: 5.0,
        material: Material::Rock,
    }; // coal_block
    table[174] = BlockData {
        hardness: 0.5,
        material: Material::PackedIce,
    }; // packed_ice
    table[175] = BlockData {
        hardness: 0.0,
        material: Material::Vine,
    }; // double_plant
    table[176] = BlockData {
        hardness: 1.0,
        material: Material::Wood,
    }; // standing_banner
    table[177] = BlockData {
        hardness: 1.0,
        material: Material::Wood,
    }; // wall_banner
    table[178] = BlockData {
        hardness: 0.2,
        material: Material::Wood,
    }; // daylight_detector_inverted
    table[179] = BlockData {
        hardness: 0.8,
        material: Material::Rock,
    }; // red_sandstone
    table[180] = BlockData {
        hardness: 0.8,
        material: Material::Rock,
    }; // red_sandstone_stairs
    table[181] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // double_stone_slab2
    table[182] = BlockData {
        hardness: 2.0,
        material: Material::Rock,
    }; // stone_slab2
    table[183] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // spruce_fence_gate
    table[184] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // birch_fence_gate
    table[185] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // jungle_fence_gate
    table[186] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // dark_oak_fence_gate
    table[187] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // acacia_fence_gate
    table[188] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // spruce_fence
    table[189] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // birch_fence
    table[190] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // jungle_fence
    table[191] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // dark_oak_fence
    table[192] = BlockData {
        hardness: 2.0,
        material: Material::Wood,
    }; // acacia_fence
    table[193] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // spruce_door
    table[194] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // birch_door
    table[195] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // jungle_door
    table[196] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // acacia_door
    table[197] = BlockData {
        hardness: 3.0,
        material: Material::Wood,
    }; // dark_oak_door
    table
};
