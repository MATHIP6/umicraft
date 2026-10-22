// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 block-id to blockstate/model table.
//!
//! 1.8 has no registry file: the table below is derived from the reference
//! implementation. Each [`BlockDefinition`] owns an ordered list of
//! [`StateDef`] rules; the first rule whose `metadata & mask == value` applies.
//! Bits outside a rule's mask are ignored.
//!
//! # Sources (ADR-0001)
//!
//! - `Block.java` `func_176219_a(id, name, ...)` registrations (ids `1..=197`).
//! - Each block class's `func_176203_a(int)` / `func_176201_c(IBlockState)`
//!   metadata conversion (`BlockOldLog`, `BlockDoor`, `BlockStairs`, ...).
//! - `BlockModelShapes.func_178119_d` and `StateMap.func_178132_a` for document
//!   suffixes (`_log`, `_leaves`, `_planks`, `_slab`, ...), removed properties
//!   (`powered`, `age`, `triggered`, ...) and the custom quartz, dirt, stem and
//!   double-slab mappers.
//! - The 340 `assets/minecraft/blockstates/*.json` documents for canonical
//!   variant keys (alphabetical `name=value` pairs, or a bare key such as
//!   `normal` / `all`).
//!
//! # Coverage and approximation
//!
//! Ids `8..=11`, `36`, `54`, `63`, `68`, `119`, `130`, `144`, `146`, `166`,
//! `176` and `177` have no blockstate document and are omitted; the mesher uses
//! its legacy cube fallback. `item_frame` is an entity document, not a block.
//!
//! Metadata is four bits and cannot carry properties that 1.8 computes from
//! neighbours or block entities. Those states use one real key: connections and
//! `up` are `false`, `shape=straight`, `short=false`, `in_wall=false`,
//! `locked=false`, `suspended=false`, `contents=empty`, and upper double plants
//! use the sunflower document. Metadata values that 1.8 itself cannot produce
//! are folded onto the nearest shipped variant instead of inventing a key:
//! piston facing `6/7` and hopper facing `up` become `facing=down`, powered
//! rails with a curved shape (`6/7`) become `north_south`, and out-of-range
//! enum indices clamp exactly as the reference enum helpers do.
//!
//! `grass` and `mycelium` are a deliberate task-level deviation from 1.8: the
//! game computes `snowy` from the block above, but the M5 acceptance table
//! requires metadata bit 0 to select `snowy=false` / `snowy=true`, so both
//! definitions do that.

/// One metadata range that selects a blockstate document and variant key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateDef {
    /// Metadata mask; `0` matches every metadata value.
    pub mask: u8,
    /// Value the masked metadata must equal.
    pub value: u8,
    /// Blockstate document name without `.json`.
    pub blockstate: &'static str,
    /// Canonical variant key inside that document.
    pub variant: &'static str,
}

/// One 1.8 block id and the state mapping used by the model set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDefinition {
    /// Numeric block id.
    pub id: u16,
    /// Registry name (also the blockstate document name for simple blocks).
    pub name: &'static str,
    /// Metadata-to-state rules, first match wins.
    pub states: &'static [StateDef],
}

impl BlockDefinition {
    /// State rule for a metadata value.
    #[must_use]
    pub fn state(&self, metadata: u8) -> Option<&StateDef> {
        self.states
            .iter()
            .find(|state| metadata & state.mask == state.value)
    }

    /// Blockstate document name for a metadata value.
    #[must_use]
    pub fn blockstate(&self, metadata: u8) -> Option<&'static str> {
        self.state(metadata).map(|state| state.blockstate)
    }
}

/// Shorthand for a [`StateDef`] in the generated table below.
macro_rules! state {
    ($mask:expr, $value:expr, $blockstate:literal, $variant:literal) => {
        StateDef {
            mask: $mask,
            value: $value,
            blockstate: $blockstate,
            variant: $variant,
        }
    };
}

/// Every known 1.8 block definition, ordered by numeric id.
///
/// This is data derived from the reference sources cited in the module
/// documentation. `tests/registry.rs` checks every document name and variant
/// key against a reviewed fixture, so a key that the real document does not
/// contain fails the test suite instead of silently reaching the runtime
/// fallback.
static DEFINITIONS: &[BlockDefinition] = &[
    BlockDefinition {
        id: 1,
        name: "stone",
        states: &[
            state!(15, 0, "stone", "normal"),
            state!(15, 1, "granite", "normal"),
            state!(15, 2, "smooth_granite", "normal"),
            state!(15, 3, "diorite", "normal"),
            state!(15, 4, "smooth_diorite", "normal"),
            state!(15, 5, "andesite", "normal"),
            state!(15, 6, "smooth_andesite", "normal"),
            state!(0, 0, "stone", "normal"),
        ],
    },
    BlockDefinition {
        id: 2,
        name: "grass",
        states: &[
            state!(1, 0, "grass", "snowy=false"),
            state!(1, 1, "grass", "snowy=true"),
        ],
    },
    BlockDefinition {
        id: 3,
        name: "dirt",
        states: &[
            state!(15, 0, "dirt", "normal"),
            state!(15, 1, "coarse_dirt", "normal"),
            state!(15, 2, "podzol", "snowy=false"),
            state!(0, 0, "dirt", "normal"),
        ],
    },
    BlockDefinition {
        id: 4,
        name: "cobblestone",
        states: &[state!(0, 0, "cobblestone", "normal")],
    },
    BlockDefinition {
        id: 5,
        name: "planks",
        states: &[
            state!(15, 0, "oak_planks", "normal"),
            state!(15, 1, "spruce_planks", "normal"),
            state!(15, 2, "birch_planks", "normal"),
            state!(15, 3, "jungle_planks", "normal"),
            state!(15, 4, "acacia_planks", "normal"),
            state!(15, 5, "dark_oak_planks", "normal"),
            state!(0, 0, "oak_planks", "normal"),
        ],
    },
    BlockDefinition {
        id: 6,
        name: "sapling",
        states: &[
            state!(15, 0, "oak_sapling", "stage=0"),
            state!(15, 1, "spruce_sapling", "stage=0"),
            state!(15, 2, "birch_sapling", "stage=0"),
            state!(15, 3, "jungle_sapling", "stage=0"),
            state!(15, 4, "acacia_sapling", "stage=0"),
            state!(15, 5, "dark_oak_sapling", "stage=0"),
            state!(15, 6, "oak_sapling", "stage=0"),
            state!(15, 7, "oak_sapling", "stage=0"),
            state!(15, 8, "oak_sapling", "stage=1"),
            state!(15, 9, "spruce_sapling", "stage=1"),
            state!(15, 10, "birch_sapling", "stage=1"),
            state!(15, 11, "jungle_sapling", "stage=1"),
            state!(15, 12, "acacia_sapling", "stage=1"),
            state!(15, 13, "dark_oak_sapling", "stage=1"),
            state!(15, 14, "oak_sapling", "stage=1"),
            state!(15, 15, "oak_sapling", "stage=1"),
        ],
    },
    BlockDefinition {
        id: 7,
        name: "bedrock",
        states: &[state!(0, 0, "bedrock", "normal")],
    },
    BlockDefinition {
        id: 12,
        name: "sand",
        states: &[
            state!(15, 0, "sand", "normal"),
            state!(15, 1, "red_sand", "normal"),
            state!(0, 0, "sand", "normal"),
        ],
    },
    BlockDefinition {
        id: 13,
        name: "gravel",
        states: &[state!(0, 0, "gravel", "normal")],
    },
    BlockDefinition {
        id: 14,
        name: "gold_ore",
        states: &[state!(0, 0, "gold_ore", "normal")],
    },
    BlockDefinition {
        id: 15,
        name: "iron_ore",
        states: &[state!(0, 0, "iron_ore", "normal")],
    },
    BlockDefinition {
        id: 16,
        name: "coal_ore",
        states: &[state!(0, 0, "coal_ore", "normal")],
    },
    BlockDefinition {
        id: 17,
        name: "log",
        states: &[
            state!(15, 0, "oak_log", "axis=y"),
            state!(15, 1, "spruce_log", "axis=y"),
            state!(15, 2, "birch_log", "axis=y"),
            state!(15, 3, "jungle_log", "axis=y"),
            state!(15, 4, "oak_log", "axis=x"),
            state!(15, 5, "spruce_log", "axis=x"),
            state!(15, 6, "birch_log", "axis=x"),
            state!(15, 7, "jungle_log", "axis=x"),
            state!(15, 8, "oak_log", "axis=z"),
            state!(15, 9, "spruce_log", "axis=z"),
            state!(15, 10, "birch_log", "axis=z"),
            state!(15, 11, "jungle_log", "axis=z"),
            state!(15, 12, "oak_log", "axis=none"),
            state!(15, 13, "spruce_log", "axis=none"),
            state!(15, 14, "birch_log", "axis=none"),
            state!(15, 15, "jungle_log", "axis=none"),
        ],
    },
    BlockDefinition {
        id: 18,
        name: "leaves",
        states: &[
            state!(3, 0, "oak_leaves", "normal"),
            state!(3, 1, "spruce_leaves", "normal"),
            state!(3, 2, "birch_leaves", "normal"),
            state!(3, 3, "jungle_leaves", "normal"),
        ],
    },
    BlockDefinition {
        id: 19,
        name: "sponge",
        states: &[
            state!(1, 0, "sponge", "wet=false"),
            state!(1, 1, "sponge", "wet=true"),
        ],
    },
    BlockDefinition {
        id: 20,
        name: "glass",
        states: &[state!(0, 0, "glass", "normal")],
    },
    BlockDefinition {
        id: 21,
        name: "lapis_ore",
        states: &[state!(0, 0, "lapis_ore", "normal")],
    },
    BlockDefinition {
        id: 22,
        name: "lapis_block",
        states: &[state!(0, 0, "lapis_block", "normal")],
    },
    BlockDefinition {
        id: 23,
        name: "dispenser",
        states: &[
            state!(7, 0, "dispenser", "facing=down"),
            state!(7, 1, "dispenser", "facing=up"),
            state!(7, 2, "dispenser", "facing=north"),
            state!(7, 3, "dispenser", "facing=south"),
            state!(7, 4, "dispenser", "facing=west"),
            state!(7, 5, "dispenser", "facing=east"),
            state!(7, 6, "dispenser", "facing=down"),
            state!(7, 7, "dispenser", "facing=up"),
        ],
    },
    BlockDefinition {
        id: 24,
        name: "sandstone",
        states: &[
            state!(15, 0, "sandstone", "normal"),
            state!(15, 1, "chiseled_sandstone", "normal"),
            state!(15, 2, "smooth_sandstone", "normal"),
            state!(0, 0, "sandstone", "normal"),
        ],
    },
    BlockDefinition {
        id: 25,
        name: "noteblock",
        states: &[state!(0, 0, "noteblock", "normal")],
    },
    BlockDefinition {
        id: 26,
        name: "bed",
        states: &[
            state!(11, 0, "bed", "facing=south,part=foot"),
            state!(11, 8, "bed", "facing=south,part=head"),
            state!(11, 1, "bed", "facing=west,part=foot"),
            state!(11, 9, "bed", "facing=west,part=head"),
            state!(11, 2, "bed", "facing=north,part=foot"),
            state!(11, 10, "bed", "facing=north,part=head"),
            state!(11, 3, "bed", "facing=east,part=foot"),
            state!(11, 11, "bed", "facing=east,part=head"),
        ],
    },
    BlockDefinition {
        id: 27,
        name: "golden_rail",
        states: &[
            state!(15, 0, "golden_rail", "powered=false,shape=north_south"),
            state!(15, 1, "golden_rail", "powered=false,shape=east_west"),
            state!(15, 2, "golden_rail", "powered=false,shape=ascending_east"),
            state!(15, 3, "golden_rail", "powered=false,shape=ascending_west"),
            state!(15, 4, "golden_rail", "powered=false,shape=ascending_north"),
            state!(15, 5, "golden_rail", "powered=false,shape=ascending_south"),
            state!(15, 6, "golden_rail", "powered=false,shape=north_south"),
            state!(15, 7, "golden_rail", "powered=false,shape=north_south"),
            state!(15, 8, "golden_rail", "powered=true,shape=north_south"),
            state!(15, 9, "golden_rail", "powered=true,shape=east_west"),
            state!(15, 10, "golden_rail", "powered=true,shape=ascending_east"),
            state!(15, 11, "golden_rail", "powered=true,shape=ascending_west"),
            state!(15, 12, "golden_rail", "powered=true,shape=ascending_north"),
            state!(15, 13, "golden_rail", "powered=true,shape=ascending_south"),
            state!(15, 14, "golden_rail", "powered=true,shape=north_south"),
            state!(15, 15, "golden_rail", "powered=true,shape=north_south"),
        ],
    },
    BlockDefinition {
        id: 28,
        name: "detector_rail",
        states: &[
            state!(15, 0, "detector_rail", "powered=false,shape=north_south"),
            state!(15, 1, "detector_rail", "powered=false,shape=east_west"),
            state!(15, 2, "detector_rail", "powered=false,shape=ascending_east"),
            state!(15, 3, "detector_rail", "powered=false,shape=ascending_west"),
            state!(
                15,
                4,
                "detector_rail",
                "powered=false,shape=ascending_north"
            ),
            state!(
                15,
                5,
                "detector_rail",
                "powered=false,shape=ascending_south"
            ),
            state!(15, 6, "detector_rail", "powered=false,shape=north_south"),
            state!(15, 7, "detector_rail", "powered=false,shape=north_south"),
            state!(15, 8, "detector_rail", "powered=true,shape=north_south"),
            state!(15, 9, "detector_rail", "powered=true,shape=east_west"),
            state!(15, 10, "detector_rail", "powered=true,shape=ascending_east"),
            state!(15, 11, "detector_rail", "powered=true,shape=ascending_west"),
            state!(
                15,
                12,
                "detector_rail",
                "powered=true,shape=ascending_north"
            ),
            state!(
                15,
                13,
                "detector_rail",
                "powered=true,shape=ascending_south"
            ),
            state!(15, 14, "detector_rail", "powered=true,shape=north_south"),
            state!(15, 15, "detector_rail", "powered=true,shape=north_south"),
        ],
    },
    BlockDefinition {
        id: 29,
        name: "sticky_piston",
        states: &[
            state!(15, 0, "sticky_piston", "extended=false,facing=down"),
            state!(15, 1, "sticky_piston", "extended=false,facing=up"),
            state!(15, 2, "sticky_piston", "extended=false,facing=north"),
            state!(15, 3, "sticky_piston", "extended=false,facing=south"),
            state!(15, 4, "sticky_piston", "extended=false,facing=west"),
            state!(15, 5, "sticky_piston", "extended=false,facing=east"),
            state!(15, 6, "sticky_piston", "extended=false,facing=down"),
            state!(15, 7, "sticky_piston", "extended=false,facing=down"),
            state!(15, 8, "sticky_piston", "extended=true,facing=down"),
            state!(15, 9, "sticky_piston", "extended=true,facing=up"),
            state!(15, 10, "sticky_piston", "extended=true,facing=north"),
            state!(15, 11, "sticky_piston", "extended=true,facing=south"),
            state!(15, 12, "sticky_piston", "extended=true,facing=west"),
            state!(15, 13, "sticky_piston", "extended=true,facing=east"),
            state!(15, 14, "sticky_piston", "extended=true,facing=down"),
            state!(15, 15, "sticky_piston", "extended=true,facing=down"),
        ],
    },
    BlockDefinition {
        id: 30,
        name: "web",
        states: &[state!(0, 0, "web", "normal")],
    },
    BlockDefinition {
        id: 31,
        name: "tallgrass",
        states: &[
            state!(15, 0, "dead_bush", "normal"),
            state!(15, 1, "tall_grass", "normal"),
            state!(15, 2, "fern", "normal"),
            state!(0, 0, "dead_bush", "normal"),
        ],
    },
    BlockDefinition {
        id: 32,
        name: "deadbush",
        states: &[state!(0, 0, "dead_bush", "normal")],
    },
    BlockDefinition {
        id: 33,
        name: "piston",
        states: &[
            state!(15, 0, "piston", "extended=false,facing=down"),
            state!(15, 1, "piston", "extended=false,facing=up"),
            state!(15, 2, "piston", "extended=false,facing=north"),
            state!(15, 3, "piston", "extended=false,facing=south"),
            state!(15, 4, "piston", "extended=false,facing=west"),
            state!(15, 5, "piston", "extended=false,facing=east"),
            state!(15, 6, "piston", "extended=false,facing=down"),
            state!(15, 7, "piston", "extended=false,facing=down"),
            state!(15, 8, "piston", "extended=true,facing=down"),
            state!(15, 9, "piston", "extended=true,facing=up"),
            state!(15, 10, "piston", "extended=true,facing=north"),
            state!(15, 11, "piston", "extended=true,facing=south"),
            state!(15, 12, "piston", "extended=true,facing=west"),
            state!(15, 13, "piston", "extended=true,facing=east"),
            state!(15, 14, "piston", "extended=true,facing=down"),
            state!(15, 15, "piston", "extended=true,facing=down"),
        ],
    },
    BlockDefinition {
        id: 34,
        name: "piston_head",
        states: &[
            state!(15, 0, "piston_head", "facing=down,short=false,type=normal"),
            state!(15, 1, "piston_head", "facing=up,short=false,type=normal"),
            state!(15, 2, "piston_head", "facing=north,short=false,type=normal"),
            state!(15, 3, "piston_head", "facing=south,short=false,type=normal"),
            state!(15, 4, "piston_head", "facing=west,short=false,type=normal"),
            state!(15, 5, "piston_head", "facing=east,short=false,type=normal"),
            state!(15, 6, "piston_head", "facing=down,short=false,type=normal"),
            state!(15, 7, "piston_head", "facing=down,short=false,type=normal"),
            state!(15, 8, "piston_head", "facing=down,short=false,type=sticky"),
            state!(15, 9, "piston_head", "facing=up,short=false,type=sticky"),
            state!(
                15,
                10,
                "piston_head",
                "facing=north,short=false,type=sticky"
            ),
            state!(
                15,
                11,
                "piston_head",
                "facing=south,short=false,type=sticky"
            ),
            state!(15, 12, "piston_head", "facing=west,short=false,type=sticky"),
            state!(15, 13, "piston_head", "facing=east,short=false,type=sticky"),
            state!(15, 14, "piston_head", "facing=down,short=false,type=sticky"),
            state!(15, 15, "piston_head", "facing=down,short=false,type=sticky"),
        ],
    },
    BlockDefinition {
        id: 35,
        name: "wool",
        states: &[
            state!(15, 0, "white_wool", "normal"),
            state!(15, 1, "orange_wool", "normal"),
            state!(15, 2, "magenta_wool", "normal"),
            state!(15, 3, "light_blue_wool", "normal"),
            state!(15, 4, "yellow_wool", "normal"),
            state!(15, 5, "lime_wool", "normal"),
            state!(15, 6, "pink_wool", "normal"),
            state!(15, 7, "gray_wool", "normal"),
            state!(15, 8, "silver_wool", "normal"),
            state!(15, 9, "cyan_wool", "normal"),
            state!(15, 10, "purple_wool", "normal"),
            state!(15, 11, "blue_wool", "normal"),
            state!(15, 12, "brown_wool", "normal"),
            state!(15, 13, "green_wool", "normal"),
            state!(15, 14, "red_wool", "normal"),
            state!(15, 15, "black_wool", "normal"),
        ],
    },
    BlockDefinition {
        id: 37,
        name: "yellow_flower",
        states: &[state!(0, 0, "dandelion", "normal")],
    },
    BlockDefinition {
        id: 38,
        name: "red_flower",
        states: &[
            state!(15, 0, "poppy", "normal"),
            state!(15, 1, "blue_orchid", "normal"),
            state!(15, 2, "allium", "normal"),
            state!(15, 3, "houstonia", "normal"),
            state!(15, 4, "red_tulip", "normal"),
            state!(15, 5, "orange_tulip", "normal"),
            state!(15, 6, "white_tulip", "normal"),
            state!(15, 7, "pink_tulip", "normal"),
            state!(15, 8, "oxeye_daisy", "normal"),
            state!(0, 0, "poppy", "normal"),
        ],
    },
    BlockDefinition {
        id: 39,
        name: "brown_mushroom",
        states: &[state!(0, 0, "brown_mushroom", "normal")],
    },
    BlockDefinition {
        id: 40,
        name: "red_mushroom",
        states: &[state!(0, 0, "red_mushroom", "normal")],
    },
    BlockDefinition {
        id: 41,
        name: "gold_block",
        states: &[state!(0, 0, "gold_block", "normal")],
    },
    BlockDefinition {
        id: 42,
        name: "iron_block",
        states: &[state!(0, 0, "iron_block", "normal")],
    },
    BlockDefinition {
        id: 43,
        name: "double_stone_slab",
        states: &[
            state!(15, 0, "stone_double_slab", "normal"),
            state!(15, 1, "sandstone_double_slab", "normal"),
            state!(15, 2, "wood_old_double_slab", "normal"),
            state!(15, 3, "cobblestone_double_slab", "normal"),
            state!(15, 4, "brick_double_slab", "normal"),
            state!(15, 5, "stone_brick_double_slab", "normal"),
            state!(15, 6, "nether_brick_double_slab", "normal"),
            state!(15, 7, "quartz_double_slab", "normal"),
            state!(15, 8, "stone_double_slab", "all"),
            state!(15, 9, "sandstone_double_slab", "all"),
            state!(15, 10, "wood_old_double_slab", "all"),
            state!(15, 11, "cobblestone_double_slab", "all"),
            state!(15, 12, "brick_double_slab", "all"),
            state!(15, 13, "stone_brick_double_slab", "all"),
            state!(15, 14, "nether_brick_double_slab", "all"),
            state!(15, 15, "quartz_double_slab", "all"),
        ],
    },
    BlockDefinition {
        id: 44,
        name: "stone_slab",
        states: &[
            state!(15, 0, "stone_slab", "half=bottom"),
            state!(15, 1, "sandstone_slab", "half=bottom"),
            state!(15, 2, "wood_old_slab", "half=bottom"),
            state!(15, 3, "cobblestone_slab", "half=bottom"),
            state!(15, 4, "brick_slab", "half=bottom"),
            state!(15, 5, "stone_brick_slab", "half=bottom"),
            state!(15, 6, "nether_brick_slab", "half=bottom"),
            state!(15, 7, "quartz_slab", "half=bottom"),
            state!(15, 8, "stone_slab", "half=top"),
            state!(15, 9, "sandstone_slab", "half=top"),
            state!(15, 10, "wood_old_slab", "half=top"),
            state!(15, 11, "cobblestone_slab", "half=top"),
            state!(15, 12, "brick_slab", "half=top"),
            state!(15, 13, "stone_brick_slab", "half=top"),
            state!(15, 14, "nether_brick_slab", "half=top"),
            state!(15, 15, "quartz_slab", "half=top"),
        ],
    },
    BlockDefinition {
        id: 45,
        name: "brick_block",
        states: &[state!(0, 0, "brick_block", "normal")],
    },
    BlockDefinition {
        id: 46,
        name: "tnt",
        states: &[state!(0, 0, "tnt", "normal")],
    },
    BlockDefinition {
        id: 47,
        name: "bookshelf",
        states: &[state!(0, 0, "bookshelf", "normal")],
    },
    BlockDefinition {
        id: 48,
        name: "mossy_cobblestone",
        states: &[state!(0, 0, "mossy_cobblestone", "normal")],
    },
    BlockDefinition {
        id: 49,
        name: "obsidian",
        states: &[state!(0, 0, "obsidian", "normal")],
    },
    BlockDefinition {
        id: 50,
        name: "torch",
        states: &[
            state!(15, 0, "torch", "facing=up"),
            state!(15, 1, "torch", "facing=east"),
            state!(15, 2, "torch", "facing=west"),
            state!(15, 3, "torch", "facing=south"),
            state!(15, 4, "torch", "facing=north"),
            state!(0, 0, "torch", "facing=up"),
        ],
    },
    BlockDefinition {
        id: 51,
        name: "fire",
        states: &[state!(
            0,
            0,
            "fire",
            "alt=false,east=false,flip=false,north=false,south=false,upper=0,west=false"
        )],
    },
    BlockDefinition {
        id: 52,
        name: "mob_spawner",
        states: &[state!(0, 0, "mob_spawner", "normal")],
    },
    BlockDefinition {
        id: 53,
        name: "oak_stairs",
        states: &[
            state!(7, 0, "oak_stairs", "facing=east,half=bottom,shape=straight"),
            state!(7, 1, "oak_stairs", "facing=west,half=bottom,shape=straight"),
            state!(
                7,
                2,
                "oak_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "oak_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "oak_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "oak_stairs", "facing=west,half=top,shape=straight"),
            state!(7, 6, "oak_stairs", "facing=south,half=top,shape=straight"),
            state!(7, 7, "oak_stairs", "facing=north,half=top,shape=straight"),
        ],
    },
    BlockDefinition {
        id: 55,
        name: "redstone_wire",
        states: &[state!(
            0,
            0,
            "redstone_wire",
            "east=none,north=none,south=none,west=none"
        )],
    },
    BlockDefinition {
        id: 56,
        name: "diamond_ore",
        states: &[state!(0, 0, "diamond_ore", "normal")],
    },
    BlockDefinition {
        id: 57,
        name: "diamond_block",
        states: &[state!(0, 0, "diamond_block", "normal")],
    },
    BlockDefinition {
        id: 58,
        name: "crafting_table",
        states: &[state!(0, 0, "crafting_table", "normal")],
    },
    BlockDefinition {
        id: 59,
        name: "wheat",
        states: &[
            state!(7, 0, "wheat", "age=0"),
            state!(7, 1, "wheat", "age=1"),
            state!(7, 2, "wheat", "age=2"),
            state!(7, 3, "wheat", "age=3"),
            state!(7, 4, "wheat", "age=4"),
            state!(7, 5, "wheat", "age=5"),
            state!(7, 6, "wheat", "age=6"),
            state!(7, 7, "wheat", "age=7"),
        ],
    },
    BlockDefinition {
        id: 60,
        name: "farmland",
        states: &[
            state!(7, 0, "farmland", "moisture=0"),
            state!(7, 1, "farmland", "moisture=1"),
            state!(7, 2, "farmland", "moisture=2"),
            state!(7, 3, "farmland", "moisture=3"),
            state!(7, 4, "farmland", "moisture=4"),
            state!(7, 5, "farmland", "moisture=5"),
            state!(7, 6, "farmland", "moisture=6"),
            state!(7, 7, "farmland", "moisture=7"),
        ],
    },
    BlockDefinition {
        id: 61,
        name: "furnace",
        states: &[
            state!(15, 0, "furnace", "facing=north"),
            state!(15, 1, "furnace", "facing=north"),
            state!(15, 2, "furnace", "facing=north"),
            state!(15, 3, "furnace", "facing=south"),
            state!(15, 4, "furnace", "facing=west"),
            state!(15, 5, "furnace", "facing=east"),
            state!(15, 6, "furnace", "facing=north"),
            state!(15, 7, "furnace", "facing=north"),
            state!(15, 8, "furnace", "facing=north"),
            state!(15, 9, "furnace", "facing=south"),
            state!(15, 10, "furnace", "facing=west"),
            state!(15, 11, "furnace", "facing=east"),
            state!(15, 12, "furnace", "facing=north"),
            state!(15, 13, "furnace", "facing=north"),
            state!(15, 14, "furnace", "facing=north"),
            state!(15, 15, "furnace", "facing=south"),
        ],
    },
    BlockDefinition {
        id: 62,
        name: "lit_furnace",
        states: &[
            state!(15, 0, "lit_furnace", "facing=north"),
            state!(15, 1, "lit_furnace", "facing=north"),
            state!(15, 2, "lit_furnace", "facing=north"),
            state!(15, 3, "lit_furnace", "facing=south"),
            state!(15, 4, "lit_furnace", "facing=west"),
            state!(15, 5, "lit_furnace", "facing=east"),
            state!(15, 6, "lit_furnace", "facing=north"),
            state!(15, 7, "lit_furnace", "facing=north"),
            state!(15, 8, "lit_furnace", "facing=north"),
            state!(15, 9, "lit_furnace", "facing=south"),
            state!(15, 10, "lit_furnace", "facing=west"),
            state!(15, 11, "lit_furnace", "facing=east"),
            state!(15, 12, "lit_furnace", "facing=north"),
            state!(15, 13, "lit_furnace", "facing=north"),
            state!(15, 14, "lit_furnace", "facing=north"),
            state!(15, 15, "lit_furnace", "facing=south"),
        ],
    },
    BlockDefinition {
        id: 64,
        name: "wooden_door",
        states: &[
            state!(
                15,
                0,
                "wooden_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "wooden_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "wooden_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "wooden_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "wooden_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "wooden_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "wooden_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "wooden_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "wooden_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "wooden_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "wooden_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "wooden_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "wooden_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "wooden_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "wooden_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "wooden_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
    BlockDefinition {
        id: 65,
        name: "ladder",
        states: &[
            state!(15, 0, "ladder", "facing=north"),
            state!(15, 1, "ladder", "facing=north"),
            state!(15, 2, "ladder", "facing=north"),
            state!(15, 3, "ladder", "facing=south"),
            state!(15, 4, "ladder", "facing=west"),
            state!(15, 5, "ladder", "facing=east"),
            state!(15, 6, "ladder", "facing=north"),
            state!(15, 7, "ladder", "facing=north"),
            state!(15, 8, "ladder", "facing=north"),
            state!(15, 9, "ladder", "facing=south"),
            state!(15, 10, "ladder", "facing=west"),
            state!(15, 11, "ladder", "facing=east"),
            state!(15, 12, "ladder", "facing=north"),
            state!(15, 13, "ladder", "facing=north"),
            state!(15, 14, "ladder", "facing=north"),
            state!(15, 15, "ladder", "facing=south"),
        ],
    },
    BlockDefinition {
        id: 66,
        name: "rail",
        states: &[
            state!(15, 0, "rail", "shape=north_south"),
            state!(15, 1, "rail", "shape=east_west"),
            state!(15, 2, "rail", "shape=ascending_east"),
            state!(15, 3, "rail", "shape=ascending_west"),
            state!(15, 4, "rail", "shape=ascending_north"),
            state!(15, 5, "rail", "shape=ascending_south"),
            state!(15, 6, "rail", "shape=south_east"),
            state!(15, 7, "rail", "shape=south_west"),
            state!(15, 8, "rail", "shape=north_west"),
            state!(15, 9, "rail", "shape=north_east"),
            state!(0, 0, "rail", "shape=north_south"),
        ],
    },
    BlockDefinition {
        id: 67,
        name: "stone_stairs",
        states: &[
            state!(
                7,
                0,
                "stone_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "stone_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "stone_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "stone_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "stone_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "stone_stairs", "facing=west,half=top,shape=straight"),
            state!(7, 6, "stone_stairs", "facing=south,half=top,shape=straight"),
            state!(7, 7, "stone_stairs", "facing=north,half=top,shape=straight"),
        ],
    },
    BlockDefinition {
        id: 69,
        name: "lever",
        states: &[
            state!(15, 0, "lever", "facing=down_x,powered=false"),
            state!(15, 1, "lever", "facing=east,powered=false"),
            state!(15, 2, "lever", "facing=west,powered=false"),
            state!(15, 3, "lever", "facing=south,powered=false"),
            state!(15, 4, "lever", "facing=north,powered=false"),
            state!(15, 5, "lever", "facing=up_z,powered=false"),
            state!(15, 6, "lever", "facing=up_x,powered=false"),
            state!(15, 7, "lever", "facing=down_z,powered=false"),
            state!(15, 8, "lever", "facing=down_x,powered=true"),
            state!(15, 9, "lever", "facing=east,powered=true"),
            state!(15, 10, "lever", "facing=west,powered=true"),
            state!(15, 11, "lever", "facing=south,powered=true"),
            state!(15, 12, "lever", "facing=north,powered=true"),
            state!(15, 13, "lever", "facing=up_z,powered=true"),
            state!(15, 14, "lever", "facing=up_x,powered=true"),
            state!(15, 15, "lever", "facing=down_z,powered=true"),
        ],
    },
    BlockDefinition {
        id: 70,
        name: "stone_pressure_plate",
        states: &[
            state!(15, 1, "stone_pressure_plate", "powered=true"),
            state!(0, 0, "stone_pressure_plate", "powered=false"),
        ],
    },
    BlockDefinition {
        id: 71,
        name: "iron_door",
        states: &[
            state!(
                15,
                0,
                "iron_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "iron_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "iron_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "iron_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "iron_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "iron_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "iron_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "iron_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "iron_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "iron_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "iron_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "iron_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "iron_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "iron_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "iron_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "iron_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
    BlockDefinition {
        id: 72,
        name: "wooden_pressure_plate",
        states: &[
            state!(15, 1, "wooden_pressure_plate", "powered=true"),
            state!(0, 0, "wooden_pressure_plate", "powered=false"),
        ],
    },
    BlockDefinition {
        id: 73,
        name: "redstone_ore",
        states: &[state!(0, 0, "redstone_ore", "normal")],
    },
    BlockDefinition {
        id: 74,
        name: "lit_redstone_ore",
        states: &[state!(0, 0, "lit_redstone_ore", "normal")],
    },
    BlockDefinition {
        id: 75,
        name: "unlit_redstone_torch",
        states: &[
            state!(15, 0, "unlit_redstone_torch", "facing=up"),
            state!(15, 1, "unlit_redstone_torch", "facing=east"),
            state!(15, 2, "unlit_redstone_torch", "facing=west"),
            state!(15, 3, "unlit_redstone_torch", "facing=south"),
            state!(15, 4, "unlit_redstone_torch", "facing=north"),
            state!(0, 0, "unlit_redstone_torch", "facing=up"),
        ],
    },
    BlockDefinition {
        id: 76,
        name: "redstone_torch",
        states: &[
            state!(15, 0, "redstone_torch", "facing=up"),
            state!(15, 1, "redstone_torch", "facing=east"),
            state!(15, 2, "redstone_torch", "facing=west"),
            state!(15, 3, "redstone_torch", "facing=south"),
            state!(15, 4, "redstone_torch", "facing=north"),
            state!(0, 0, "redstone_torch", "facing=up"),
        ],
    },
    BlockDefinition {
        id: 77,
        name: "stone_button",
        states: &[
            state!(15, 0, "stone_button", "facing=down,powered=false"),
            state!(15, 1, "stone_button", "facing=east,powered=false"),
            state!(15, 2, "stone_button", "facing=west,powered=false"),
            state!(15, 3, "stone_button", "facing=south,powered=false"),
            state!(15, 4, "stone_button", "facing=north,powered=false"),
            state!(15, 5, "stone_button", "facing=up,powered=false"),
            state!(15, 6, "stone_button", "facing=up,powered=false"),
            state!(15, 7, "stone_button", "facing=up,powered=false"),
            state!(15, 8, "stone_button", "facing=down,powered=true"),
            state!(15, 9, "stone_button", "facing=east,powered=true"),
            state!(15, 10, "stone_button", "facing=west,powered=true"),
            state!(15, 11, "stone_button", "facing=south,powered=true"),
            state!(15, 12, "stone_button", "facing=north,powered=true"),
            state!(15, 13, "stone_button", "facing=up,powered=true"),
            state!(15, 14, "stone_button", "facing=up,powered=true"),
            state!(15, 15, "stone_button", "facing=up,powered=true"),
        ],
    },
    BlockDefinition {
        id: 78,
        name: "snow_layer",
        states: &[
            state!(7, 0, "snow_layer", "layers=1"),
            state!(7, 1, "snow_layer", "layers=2"),
            state!(7, 2, "snow_layer", "layers=3"),
            state!(7, 3, "snow_layer", "layers=4"),
            state!(7, 4, "snow_layer", "layers=5"),
            state!(7, 5, "snow_layer", "layers=6"),
            state!(7, 6, "snow_layer", "layers=7"),
            state!(7, 7, "snow_layer", "layers=8"),
        ],
    },
    BlockDefinition {
        id: 79,
        name: "ice",
        states: &[state!(0, 0, "ice", "normal")],
    },
    BlockDefinition {
        id: 80,
        name: "snow",
        states: &[state!(0, 0, "snow", "normal")],
    },
    BlockDefinition {
        id: 81,
        name: "cactus",
        states: &[state!(0, 0, "cactus", "normal")],
    },
    BlockDefinition {
        id: 82,
        name: "clay",
        states: &[state!(0, 0, "clay", "normal")],
    },
    BlockDefinition {
        id: 83,
        name: "reeds",
        states: &[state!(0, 0, "reeds", "normal")],
    },
    BlockDefinition {
        id: 84,
        name: "jukebox",
        states: &[state!(0, 0, "jukebox", "normal")],
    },
    BlockDefinition {
        id: 85,
        name: "fence",
        states: &[state!(
            0,
            0,
            "fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 86,
        name: "pumpkin",
        states: &[
            state!(3, 0, "pumpkin", "facing=south"),
            state!(3, 1, "pumpkin", "facing=west"),
            state!(3, 2, "pumpkin", "facing=north"),
            state!(3, 3, "pumpkin", "facing=east"),
        ],
    },
    BlockDefinition {
        id: 87,
        name: "netherrack",
        states: &[state!(0, 0, "netherrack", "normal")],
    },
    BlockDefinition {
        id: 88,
        name: "soul_sand",
        states: &[state!(0, 0, "soul_sand", "normal")],
    },
    BlockDefinition {
        id: 89,
        name: "glowstone",
        states: &[state!(0, 0, "glowstone", "normal")],
    },
    BlockDefinition {
        id: 90,
        name: "portal",
        states: &[
            state!(3, 2, "portal", "axis=z"),
            state!(0, 0, "portal", "axis=x"),
        ],
    },
    BlockDefinition {
        id: 91,
        name: "lit_pumpkin",
        states: &[
            state!(3, 0, "lit_pumpkin", "facing=south"),
            state!(3, 1, "lit_pumpkin", "facing=west"),
            state!(3, 2, "lit_pumpkin", "facing=north"),
            state!(3, 3, "lit_pumpkin", "facing=east"),
        ],
    },
    BlockDefinition {
        id: 92,
        name: "cake",
        states: &[
            state!(15, 0, "cake", "bites=0"),
            state!(15, 1, "cake", "bites=1"),
            state!(15, 2, "cake", "bites=2"),
            state!(15, 3, "cake", "bites=3"),
            state!(15, 4, "cake", "bites=4"),
            state!(15, 5, "cake", "bites=5"),
            state!(15, 6, "cake", "bites=6"),
            state!(0, 0, "cake", "bites=6"),
        ],
    },
    BlockDefinition {
        id: 93,
        name: "unpowered_repeater",
        states: &[
            state!(
                15,
                0,
                "unpowered_repeater",
                "delay=1,facing=south,locked=false"
            ),
            state!(
                15,
                1,
                "unpowered_repeater",
                "delay=1,facing=west,locked=false"
            ),
            state!(
                15,
                2,
                "unpowered_repeater",
                "delay=1,facing=north,locked=false"
            ),
            state!(
                15,
                3,
                "unpowered_repeater",
                "delay=1,facing=east,locked=false"
            ),
            state!(
                15,
                4,
                "unpowered_repeater",
                "delay=2,facing=south,locked=false"
            ),
            state!(
                15,
                5,
                "unpowered_repeater",
                "delay=2,facing=west,locked=false"
            ),
            state!(
                15,
                6,
                "unpowered_repeater",
                "delay=2,facing=north,locked=false"
            ),
            state!(
                15,
                7,
                "unpowered_repeater",
                "delay=2,facing=east,locked=false"
            ),
            state!(
                15,
                8,
                "unpowered_repeater",
                "delay=3,facing=south,locked=false"
            ),
            state!(
                15,
                9,
                "unpowered_repeater",
                "delay=3,facing=west,locked=false"
            ),
            state!(
                15,
                10,
                "unpowered_repeater",
                "delay=3,facing=north,locked=false"
            ),
            state!(
                15,
                11,
                "unpowered_repeater",
                "delay=3,facing=east,locked=false"
            ),
            state!(
                15,
                12,
                "unpowered_repeater",
                "delay=4,facing=south,locked=false"
            ),
            state!(
                15,
                13,
                "unpowered_repeater",
                "delay=4,facing=west,locked=false"
            ),
            state!(
                15,
                14,
                "unpowered_repeater",
                "delay=4,facing=north,locked=false"
            ),
            state!(
                15,
                15,
                "unpowered_repeater",
                "delay=4,facing=east,locked=false"
            ),
        ],
    },
    BlockDefinition {
        id: 94,
        name: "powered_repeater",
        states: &[
            state!(
                15,
                0,
                "powered_repeater",
                "delay=1,facing=south,locked=false"
            ),
            state!(
                15,
                1,
                "powered_repeater",
                "delay=1,facing=west,locked=false"
            ),
            state!(
                15,
                2,
                "powered_repeater",
                "delay=1,facing=north,locked=false"
            ),
            state!(
                15,
                3,
                "powered_repeater",
                "delay=1,facing=east,locked=false"
            ),
            state!(
                15,
                4,
                "powered_repeater",
                "delay=2,facing=south,locked=false"
            ),
            state!(
                15,
                5,
                "powered_repeater",
                "delay=2,facing=west,locked=false"
            ),
            state!(
                15,
                6,
                "powered_repeater",
                "delay=2,facing=north,locked=false"
            ),
            state!(
                15,
                7,
                "powered_repeater",
                "delay=2,facing=east,locked=false"
            ),
            state!(
                15,
                8,
                "powered_repeater",
                "delay=3,facing=south,locked=false"
            ),
            state!(
                15,
                9,
                "powered_repeater",
                "delay=3,facing=west,locked=false"
            ),
            state!(
                15,
                10,
                "powered_repeater",
                "delay=3,facing=north,locked=false"
            ),
            state!(
                15,
                11,
                "powered_repeater",
                "delay=3,facing=east,locked=false"
            ),
            state!(
                15,
                12,
                "powered_repeater",
                "delay=4,facing=south,locked=false"
            ),
            state!(
                15,
                13,
                "powered_repeater",
                "delay=4,facing=west,locked=false"
            ),
            state!(
                15,
                14,
                "powered_repeater",
                "delay=4,facing=north,locked=false"
            ),
            state!(
                15,
                15,
                "powered_repeater",
                "delay=4,facing=east,locked=false"
            ),
        ],
    },
    BlockDefinition {
        id: 95,
        name: "stained_glass",
        states: &[
            state!(15, 0, "white_stained_glass", "normal"),
            state!(15, 1, "orange_stained_glass", "normal"),
            state!(15, 2, "magenta_stained_glass", "normal"),
            state!(15, 3, "light_blue_stained_glass", "normal"),
            state!(15, 4, "yellow_stained_glass", "normal"),
            state!(15, 5, "lime_stained_glass", "normal"),
            state!(15, 6, "pink_stained_glass", "normal"),
            state!(15, 7, "gray_stained_glass", "normal"),
            state!(15, 8, "silver_stained_glass", "normal"),
            state!(15, 9, "cyan_stained_glass", "normal"),
            state!(15, 10, "purple_stained_glass", "normal"),
            state!(15, 11, "blue_stained_glass", "normal"),
            state!(15, 12, "brown_stained_glass", "normal"),
            state!(15, 13, "green_stained_glass", "normal"),
            state!(15, 14, "red_stained_glass", "normal"),
            state!(15, 15, "black_stained_glass", "normal"),
        ],
    },
    BlockDefinition {
        id: 96,
        name: "trapdoor",
        states: &[
            state!(15, 0, "trapdoor", "facing=north,half=bottom,open=false"),
            state!(15, 1, "trapdoor", "facing=south,half=bottom,open=false"),
            state!(15, 2, "trapdoor", "facing=west,half=bottom,open=false"),
            state!(15, 3, "trapdoor", "facing=east,half=bottom,open=false"),
            state!(15, 4, "trapdoor", "facing=north,half=bottom,open=true"),
            state!(15, 5, "trapdoor", "facing=south,half=bottom,open=true"),
            state!(15, 6, "trapdoor", "facing=west,half=bottom,open=true"),
            state!(15, 7, "trapdoor", "facing=east,half=bottom,open=true"),
            state!(15, 8, "trapdoor", "facing=north,half=top,open=false"),
            state!(15, 9, "trapdoor", "facing=south,half=top,open=false"),
            state!(15, 10, "trapdoor", "facing=west,half=top,open=false"),
            state!(15, 11, "trapdoor", "facing=east,half=top,open=false"),
            state!(15, 12, "trapdoor", "facing=north,half=top,open=true"),
            state!(15, 13, "trapdoor", "facing=south,half=top,open=true"),
            state!(15, 14, "trapdoor", "facing=west,half=top,open=true"),
            state!(15, 15, "trapdoor", "facing=east,half=top,open=true"),
        ],
    },
    BlockDefinition {
        id: 97,
        name: "monster_egg",
        states: &[
            state!(15, 0, "stone_monster_egg", "normal"),
            state!(15, 1, "cobblestone_monster_egg", "normal"),
            state!(15, 2, "stone_brick_monster_egg", "normal"),
            state!(15, 3, "mossy_brick_monster_egg", "normal"),
            state!(15, 4, "cracked_brick_monster_egg", "normal"),
            state!(15, 5, "chiseled_brick_monster_egg", "normal"),
            state!(0, 0, "stone_monster_egg", "normal"),
        ],
    },
    BlockDefinition {
        id: 98,
        name: "stonebrick",
        states: &[
            state!(15, 0, "stonebrick", "normal"),
            state!(15, 1, "mossy_stonebrick", "normal"),
            state!(15, 2, "cracked_stonebrick", "normal"),
            state!(15, 3, "chiseled_stonebrick", "normal"),
            state!(0, 0, "stonebrick", "normal"),
        ],
    },
    BlockDefinition {
        id: 99,
        name: "brown_mushroom_block",
        states: &[
            state!(15, 0, "brown_mushroom_block", "variant=all_inside"),
            state!(15, 1, "brown_mushroom_block", "variant=north_west"),
            state!(15, 2, "brown_mushroom_block", "variant=north"),
            state!(15, 3, "brown_mushroom_block", "variant=north_east"),
            state!(15, 4, "brown_mushroom_block", "variant=west"),
            state!(15, 5, "brown_mushroom_block", "variant=center"),
            state!(15, 6, "brown_mushroom_block", "variant=east"),
            state!(15, 7, "brown_mushroom_block", "variant=south_west"),
            state!(15, 8, "brown_mushroom_block", "variant=south"),
            state!(15, 9, "brown_mushroom_block", "variant=south_east"),
            state!(15, 10, "brown_mushroom_block", "variant=stem"),
            state!(15, 11, "brown_mushroom_block", "variant=all_inside"),
            state!(15, 12, "brown_mushroom_block", "variant=all_inside"),
            state!(15, 13, "brown_mushroom_block", "variant=all_inside"),
            state!(15, 14, "brown_mushroom_block", "variant=all_outside"),
            state!(15, 15, "brown_mushroom_block", "variant=all_stem"),
        ],
    },
    BlockDefinition {
        id: 100,
        name: "red_mushroom_block",
        states: &[
            state!(15, 0, "red_mushroom_block", "variant=all_inside"),
            state!(15, 1, "red_mushroom_block", "variant=north_west"),
            state!(15, 2, "red_mushroom_block", "variant=north"),
            state!(15, 3, "red_mushroom_block", "variant=north_east"),
            state!(15, 4, "red_mushroom_block", "variant=west"),
            state!(15, 5, "red_mushroom_block", "variant=center"),
            state!(15, 6, "red_mushroom_block", "variant=east"),
            state!(15, 7, "red_mushroom_block", "variant=south_west"),
            state!(15, 8, "red_mushroom_block", "variant=south"),
            state!(15, 9, "red_mushroom_block", "variant=south_east"),
            state!(15, 10, "red_mushroom_block", "variant=stem"),
            state!(15, 11, "red_mushroom_block", "variant=all_inside"),
            state!(15, 12, "red_mushroom_block", "variant=all_inside"),
            state!(15, 13, "red_mushroom_block", "variant=all_inside"),
            state!(15, 14, "red_mushroom_block", "variant=all_outside"),
            state!(15, 15, "red_mushroom_block", "variant=all_stem"),
        ],
    },
    BlockDefinition {
        id: 101,
        name: "iron_bars",
        states: &[state!(
            0,
            0,
            "iron_bars",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 102,
        name: "glass_pane",
        states: &[state!(
            0,
            0,
            "glass_pane",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 103,
        name: "melon_block",
        states: &[state!(0, 0, "melon_block", "normal")],
    },
    BlockDefinition {
        id: 104,
        name: "pumpkin_stem",
        states: &[
            state!(7, 0, "pumpkin_stem", "age=0,facing=up"),
            state!(7, 1, "pumpkin_stem", "age=1,facing=up"),
            state!(7, 2, "pumpkin_stem", "age=2,facing=up"),
            state!(7, 3, "pumpkin_stem", "age=3,facing=up"),
            state!(7, 4, "pumpkin_stem", "age=4,facing=up"),
            state!(7, 5, "pumpkin_stem", "age=5,facing=up"),
            state!(7, 6, "pumpkin_stem", "age=6,facing=up"),
            state!(7, 7, "pumpkin_stem", "age=7,facing=up"),
        ],
    },
    BlockDefinition {
        id: 105,
        name: "melon_stem",
        states: &[
            state!(7, 0, "melon_stem", "age=0,facing=up"),
            state!(7, 1, "melon_stem", "age=1,facing=up"),
            state!(7, 2, "melon_stem", "age=2,facing=up"),
            state!(7, 3, "melon_stem", "age=3,facing=up"),
            state!(7, 4, "melon_stem", "age=4,facing=up"),
            state!(7, 5, "melon_stem", "age=5,facing=up"),
            state!(7, 6, "melon_stem", "age=6,facing=up"),
            state!(7, 7, "melon_stem", "age=7,facing=up"),
        ],
    },
    BlockDefinition {
        id: 106,
        name: "vine",
        states: &[
            state!(
                15,
                0,
                "vine",
                "east=false,north=false,south=false,up=false,west=false"
            ),
            state!(
                15,
                1,
                "vine",
                "east=false,north=false,south=true,up=false,west=false"
            ),
            state!(
                15,
                2,
                "vine",
                "east=false,north=false,south=false,up=false,west=true"
            ),
            state!(
                15,
                3,
                "vine",
                "east=false,north=false,south=true,up=false,west=true"
            ),
            state!(
                15,
                4,
                "vine",
                "east=false,north=true,south=false,up=false,west=false"
            ),
            state!(
                15,
                5,
                "vine",
                "east=false,north=true,south=true,up=false,west=false"
            ),
            state!(
                15,
                6,
                "vine",
                "east=false,north=true,south=false,up=false,west=true"
            ),
            state!(
                15,
                7,
                "vine",
                "east=false,north=true,south=true,up=false,west=true"
            ),
            state!(
                15,
                8,
                "vine",
                "east=true,north=false,south=false,up=false,west=false"
            ),
            state!(
                15,
                9,
                "vine",
                "east=true,north=false,south=true,up=false,west=false"
            ),
            state!(
                15,
                10,
                "vine",
                "east=true,north=false,south=false,up=false,west=true"
            ),
            state!(
                15,
                11,
                "vine",
                "east=true,north=false,south=true,up=false,west=true"
            ),
            state!(
                15,
                12,
                "vine",
                "east=true,north=true,south=false,up=false,west=false"
            ),
            state!(
                15,
                13,
                "vine",
                "east=true,north=true,south=true,up=false,west=false"
            ),
            state!(
                15,
                14,
                "vine",
                "east=true,north=true,south=false,up=false,west=true"
            ),
            state!(
                15,
                15,
                "vine",
                "east=true,north=true,south=true,up=false,west=true"
            ),
        ],
    },
    BlockDefinition {
        id: 107,
        name: "fence_gate",
        states: &[
            state!(7, 0, "fence_gate", "facing=south,in_wall=false,open=false"),
            state!(7, 1, "fence_gate", "facing=west,in_wall=false,open=false"),
            state!(7, 2, "fence_gate", "facing=north,in_wall=false,open=false"),
            state!(7, 3, "fence_gate", "facing=east,in_wall=false,open=false"),
            state!(7, 4, "fence_gate", "facing=south,in_wall=false,open=true"),
            state!(7, 5, "fence_gate", "facing=west,in_wall=false,open=true"),
            state!(7, 6, "fence_gate", "facing=north,in_wall=false,open=true"),
            state!(7, 7, "fence_gate", "facing=east,in_wall=false,open=true"),
        ],
    },
    BlockDefinition {
        id: 108,
        name: "brick_stairs",
        states: &[
            state!(
                7,
                0,
                "brick_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "brick_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "brick_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "brick_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "brick_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "brick_stairs", "facing=west,half=top,shape=straight"),
            state!(7, 6, "brick_stairs", "facing=south,half=top,shape=straight"),
            state!(7, 7, "brick_stairs", "facing=north,half=top,shape=straight"),
        ],
    },
    BlockDefinition {
        id: 109,
        name: "stone_brick_stairs",
        states: &[
            state!(
                7,
                0,
                "stone_brick_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "stone_brick_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "stone_brick_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "stone_brick_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(
                7,
                4,
                "stone_brick_stairs",
                "facing=east,half=top,shape=straight"
            ),
            state!(
                7,
                5,
                "stone_brick_stairs",
                "facing=west,half=top,shape=straight"
            ),
            state!(
                7,
                6,
                "stone_brick_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "stone_brick_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 110,
        name: "mycelium",
        states: &[
            state!(1, 0, "mycelium", "snowy=false"),
            state!(1, 1, "mycelium", "snowy=true"),
        ],
    },
    BlockDefinition {
        id: 111,
        name: "waterlily",
        states: &[state!(0, 0, "waterlily", "normal")],
    },
    BlockDefinition {
        id: 112,
        name: "nether_brick",
        states: &[state!(0, 0, "nether_brick", "normal")],
    },
    BlockDefinition {
        id: 113,
        name: "nether_brick_fence",
        states: &[state!(
            0,
            0,
            "nether_brick_fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 114,
        name: "nether_brick_stairs",
        states: &[
            state!(
                7,
                0,
                "nether_brick_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "nether_brick_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "nether_brick_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "nether_brick_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(
                7,
                4,
                "nether_brick_stairs",
                "facing=east,half=top,shape=straight"
            ),
            state!(
                7,
                5,
                "nether_brick_stairs",
                "facing=west,half=top,shape=straight"
            ),
            state!(
                7,
                6,
                "nether_brick_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "nether_brick_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 115,
        name: "nether_wart",
        states: &[
            state!(3, 0, "nether_wart", "age=0"),
            state!(3, 1, "nether_wart", "age=1"),
            state!(3, 2, "nether_wart", "age=2"),
            state!(3, 3, "nether_wart", "age=3"),
        ],
    },
    BlockDefinition {
        id: 116,
        name: "enchanting_table",
        states: &[state!(0, 0, "enchanting_table", "normal")],
    },
    BlockDefinition {
        id: 117,
        name: "brewing_stand",
        states: &[
            state!(
                7,
                0,
                "brewing_stand",
                "has_bottle_0=false,has_bottle_1=false,has_bottle_2=false"
            ),
            state!(
                7,
                1,
                "brewing_stand",
                "has_bottle_0=true,has_bottle_1=false,has_bottle_2=false"
            ),
            state!(
                7,
                2,
                "brewing_stand",
                "has_bottle_0=false,has_bottle_1=true,has_bottle_2=false"
            ),
            state!(
                7,
                3,
                "brewing_stand",
                "has_bottle_0=true,has_bottle_1=true,has_bottle_2=false"
            ),
            state!(
                7,
                4,
                "brewing_stand",
                "has_bottle_0=false,has_bottle_1=false,has_bottle_2=true"
            ),
            state!(
                7,
                5,
                "brewing_stand",
                "has_bottle_0=true,has_bottle_1=false,has_bottle_2=true"
            ),
            state!(
                7,
                6,
                "brewing_stand",
                "has_bottle_0=false,has_bottle_1=true,has_bottle_2=true"
            ),
            state!(
                7,
                7,
                "brewing_stand",
                "has_bottle_0=true,has_bottle_1=true,has_bottle_2=true"
            ),
        ],
    },
    BlockDefinition {
        id: 118,
        name: "cauldron",
        states: &[
            state!(3, 0, "cauldron", "level=0"),
            state!(3, 1, "cauldron", "level=1"),
            state!(3, 2, "cauldron", "level=2"),
            state!(3, 3, "cauldron", "level=3"),
        ],
    },
    BlockDefinition {
        id: 120,
        name: "end_portal_frame",
        states: &[
            state!(7, 0, "end_portal_frame", "eye=false,facing=south"),
            state!(7, 1, "end_portal_frame", "eye=false,facing=west"),
            state!(7, 2, "end_portal_frame", "eye=false,facing=north"),
            state!(7, 3, "end_portal_frame", "eye=false,facing=east"),
            state!(7, 4, "end_portal_frame", "eye=true,facing=south"),
            state!(7, 5, "end_portal_frame", "eye=true,facing=west"),
            state!(7, 6, "end_portal_frame", "eye=true,facing=north"),
            state!(7, 7, "end_portal_frame", "eye=true,facing=east"),
        ],
    },
    BlockDefinition {
        id: 121,
        name: "end_stone",
        states: &[state!(0, 0, "end_stone", "normal")],
    },
    BlockDefinition {
        id: 122,
        name: "dragon_egg",
        states: &[state!(0, 0, "dragon_egg", "normal")],
    },
    BlockDefinition {
        id: 123,
        name: "redstone_lamp",
        states: &[state!(0, 0, "redstone_lamp", "normal")],
    },
    BlockDefinition {
        id: 124,
        name: "lit_redstone_lamp",
        states: &[state!(0, 0, "lit_redstone_lamp", "normal")],
    },
    BlockDefinition {
        id: 125,
        name: "double_wooden_slab",
        states: &[
            state!(15, 0, "oak_double_slab", "normal"),
            state!(15, 1, "spruce_double_slab", "normal"),
            state!(15, 2, "birch_double_slab", "normal"),
            state!(15, 3, "jungle_double_slab", "normal"),
            state!(15, 4, "acacia_double_slab", "normal"),
            state!(15, 5, "dark_oak_double_slab", "normal"),
            state!(15, 6, "oak_double_slab", "normal"),
            state!(15, 7, "oak_double_slab", "normal"),
            state!(15, 8, "oak_double_slab", "normal"),
            state!(15, 9, "spruce_double_slab", "normal"),
            state!(15, 10, "birch_double_slab", "normal"),
            state!(15, 11, "jungle_double_slab", "normal"),
            state!(15, 12, "acacia_double_slab", "normal"),
            state!(15, 13, "dark_oak_double_slab", "normal"),
            state!(15, 14, "oak_double_slab", "normal"),
            state!(15, 15, "oak_double_slab", "normal"),
        ],
    },
    BlockDefinition {
        id: 126,
        name: "wooden_slab",
        states: &[
            state!(15, 0, "oak_slab", "half=bottom"),
            state!(15, 1, "spruce_slab", "half=bottom"),
            state!(15, 2, "birch_slab", "half=bottom"),
            state!(15, 3, "jungle_slab", "half=bottom"),
            state!(15, 4, "acacia_slab", "half=bottom"),
            state!(15, 5, "dark_oak_slab", "half=bottom"),
            state!(15, 6, "oak_slab", "half=bottom"),
            state!(15, 7, "oak_slab", "half=bottom"),
            state!(15, 8, "oak_slab", "half=top"),
            state!(15, 9, "spruce_slab", "half=top"),
            state!(15, 10, "birch_slab", "half=top"),
            state!(15, 11, "jungle_slab", "half=top"),
            state!(15, 12, "acacia_slab", "half=top"),
            state!(15, 13, "dark_oak_slab", "half=top"),
            state!(15, 14, "oak_slab", "half=top"),
            state!(15, 15, "oak_slab", "half=top"),
        ],
    },
    BlockDefinition {
        id: 127,
        name: "cocoa",
        states: &[
            state!(15, 0, "cocoa", "age=0,facing=south"),
            state!(15, 1, "cocoa", "age=0,facing=west"),
            state!(15, 2, "cocoa", "age=0,facing=north"),
            state!(15, 3, "cocoa", "age=0,facing=east"),
            state!(15, 4, "cocoa", "age=1,facing=south"),
            state!(15, 5, "cocoa", "age=1,facing=west"),
            state!(15, 6, "cocoa", "age=1,facing=north"),
            state!(15, 7, "cocoa", "age=1,facing=east"),
            state!(15, 8, "cocoa", "age=2,facing=south"),
            state!(15, 9, "cocoa", "age=2,facing=west"),
            state!(15, 10, "cocoa", "age=2,facing=north"),
            state!(15, 11, "cocoa", "age=2,facing=east"),
            state!(15, 12, "cocoa", "age=2,facing=south"),
            state!(15, 13, "cocoa", "age=2,facing=west"),
            state!(15, 14, "cocoa", "age=2,facing=north"),
            state!(15, 15, "cocoa", "age=2,facing=east"),
        ],
    },
    BlockDefinition {
        id: 128,
        name: "sandstone_stairs",
        states: &[
            state!(
                7,
                0,
                "sandstone_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "sandstone_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "sandstone_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "sandstone_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(
                7,
                4,
                "sandstone_stairs",
                "facing=east,half=top,shape=straight"
            ),
            state!(
                7,
                5,
                "sandstone_stairs",
                "facing=west,half=top,shape=straight"
            ),
            state!(
                7,
                6,
                "sandstone_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "sandstone_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 129,
        name: "emerald_ore",
        states: &[state!(0, 0, "emerald_ore", "normal")],
    },
    BlockDefinition {
        id: 131,
        name: "tripwire_hook",
        states: &[
            state!(
                15,
                0,
                "tripwire_hook",
                "attached=false,facing=south,powered=false,suspended=false"
            ),
            state!(
                15,
                1,
                "tripwire_hook",
                "attached=false,facing=west,powered=false,suspended=false"
            ),
            state!(
                15,
                2,
                "tripwire_hook",
                "attached=false,facing=north,powered=false,suspended=false"
            ),
            state!(
                15,
                3,
                "tripwire_hook",
                "attached=false,facing=east,powered=false,suspended=false"
            ),
            state!(
                15,
                4,
                "tripwire_hook",
                "attached=true,facing=south,powered=false,suspended=false"
            ),
            state!(
                15,
                5,
                "tripwire_hook",
                "attached=true,facing=west,powered=false,suspended=false"
            ),
            state!(
                15,
                6,
                "tripwire_hook",
                "attached=true,facing=north,powered=false,suspended=false"
            ),
            state!(
                15,
                7,
                "tripwire_hook",
                "attached=true,facing=east,powered=false,suspended=false"
            ),
            state!(
                15,
                8,
                "tripwire_hook",
                "attached=false,facing=south,powered=true,suspended=false"
            ),
            state!(
                15,
                9,
                "tripwire_hook",
                "attached=false,facing=west,powered=true,suspended=false"
            ),
            state!(
                15,
                10,
                "tripwire_hook",
                "attached=false,facing=north,powered=true,suspended=false"
            ),
            state!(
                15,
                11,
                "tripwire_hook",
                "attached=false,facing=east,powered=true,suspended=false"
            ),
            state!(
                15,
                12,
                "tripwire_hook",
                "attached=true,facing=south,powered=true,suspended=false"
            ),
            state!(
                15,
                13,
                "tripwire_hook",
                "attached=true,facing=west,powered=true,suspended=false"
            ),
            state!(
                15,
                14,
                "tripwire_hook",
                "attached=true,facing=north,powered=true,suspended=false"
            ),
            state!(
                15,
                15,
                "tripwire_hook",
                "attached=true,facing=east,powered=true,suspended=false"
            ),
        ],
    },
    BlockDefinition {
        id: 132,
        name: "tripwire",
        states: &[
            state!(
                6,
                0,
                "tripwire",
                "attached=false,east=false,north=false,south=false,suspended=false,west=false"
            ),
            state!(
                6,
                2,
                "tripwire",
                "attached=false,east=false,north=false,south=false,suspended=true,west=false"
            ),
            state!(
                6,
                4,
                "tripwire",
                "attached=true,east=false,north=false,south=false,suspended=false,west=false"
            ),
            state!(
                6,
                6,
                "tripwire",
                "attached=true,east=false,north=false,south=false,suspended=true,west=false"
            ),
        ],
    },
    BlockDefinition {
        id: 133,
        name: "emerald_block",
        states: &[state!(0, 0, "emerald_block", "normal")],
    },
    BlockDefinition {
        id: 134,
        name: "spruce_stairs",
        states: &[
            state!(
                7,
                0,
                "spruce_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "spruce_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "spruce_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "spruce_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "spruce_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "spruce_stairs", "facing=west,half=top,shape=straight"),
            state!(
                7,
                6,
                "spruce_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "spruce_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 135,
        name: "birch_stairs",
        states: &[
            state!(
                7,
                0,
                "birch_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "birch_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "birch_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "birch_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "birch_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "birch_stairs", "facing=west,half=top,shape=straight"),
            state!(7, 6, "birch_stairs", "facing=south,half=top,shape=straight"),
            state!(7, 7, "birch_stairs", "facing=north,half=top,shape=straight"),
        ],
    },
    BlockDefinition {
        id: 136,
        name: "jungle_stairs",
        states: &[
            state!(
                7,
                0,
                "jungle_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "jungle_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "jungle_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "jungle_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "jungle_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "jungle_stairs", "facing=west,half=top,shape=straight"),
            state!(
                7,
                6,
                "jungle_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "jungle_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 137,
        name: "command_block",
        states: &[state!(0, 0, "command_block", "normal")],
    },
    BlockDefinition {
        id: 138,
        name: "beacon",
        states: &[state!(0, 0, "beacon", "normal")],
    },
    BlockDefinition {
        id: 139,
        name: "cobblestone_wall",
        states: &[
            state!(
                15,
                0,
                "cobblestone_wall",
                "east=false,north=false,south=false,up=false,west=false"
            ),
            state!(
                15,
                1,
                "mossy_cobblestone_wall",
                "east=false,north=false,south=false,up=false,west=false"
            ),
            state!(
                0,
                0,
                "cobblestone_wall",
                "east=false,north=false,south=false,up=false,west=false"
            ),
        ],
    },
    BlockDefinition {
        id: 140,
        name: "flower_pot",
        states: &[state!(0, 0, "flower_pot", "contents=empty")],
    },
    BlockDefinition {
        id: 141,
        name: "carrots",
        states: &[
            state!(7, 0, "carrots", "age=0"),
            state!(7, 1, "carrots", "age=1"),
            state!(7, 2, "carrots", "age=2"),
            state!(7, 3, "carrots", "age=3"),
            state!(7, 4, "carrots", "age=4"),
            state!(7, 5, "carrots", "age=5"),
            state!(7, 6, "carrots", "age=6"),
            state!(7, 7, "carrots", "age=7"),
        ],
    },
    BlockDefinition {
        id: 142,
        name: "potatoes",
        states: &[
            state!(7, 0, "potatoes", "age=0"),
            state!(7, 1, "potatoes", "age=1"),
            state!(7, 2, "potatoes", "age=2"),
            state!(7, 3, "potatoes", "age=3"),
            state!(7, 4, "potatoes", "age=4"),
            state!(7, 5, "potatoes", "age=5"),
            state!(7, 6, "potatoes", "age=6"),
            state!(7, 7, "potatoes", "age=7"),
        ],
    },
    BlockDefinition {
        id: 143,
        name: "wooden_button",
        states: &[
            state!(15, 0, "wooden_button", "facing=down,powered=false"),
            state!(15, 1, "wooden_button", "facing=east,powered=false"),
            state!(15, 2, "wooden_button", "facing=west,powered=false"),
            state!(15, 3, "wooden_button", "facing=south,powered=false"),
            state!(15, 4, "wooden_button", "facing=north,powered=false"),
            state!(15, 5, "wooden_button", "facing=up,powered=false"),
            state!(15, 6, "wooden_button", "facing=up,powered=false"),
            state!(15, 7, "wooden_button", "facing=up,powered=false"),
            state!(15, 8, "wooden_button", "facing=down,powered=true"),
            state!(15, 9, "wooden_button", "facing=east,powered=true"),
            state!(15, 10, "wooden_button", "facing=west,powered=true"),
            state!(15, 11, "wooden_button", "facing=south,powered=true"),
            state!(15, 12, "wooden_button", "facing=north,powered=true"),
            state!(15, 13, "wooden_button", "facing=up,powered=true"),
            state!(15, 14, "wooden_button", "facing=up,powered=true"),
            state!(15, 15, "wooden_button", "facing=up,powered=true"),
        ],
    },
    BlockDefinition {
        id: 145,
        name: "anvil",
        states: &[
            state!(15, 0, "anvil", "damage=0,facing=south"),
            state!(15, 1, "anvil", "damage=0,facing=west"),
            state!(15, 2, "anvil", "damage=0,facing=north"),
            state!(15, 3, "anvil", "damage=0,facing=east"),
            state!(15, 4, "anvil", "damage=1,facing=south"),
            state!(15, 5, "anvil", "damage=1,facing=west"),
            state!(15, 6, "anvil", "damage=1,facing=north"),
            state!(15, 7, "anvil", "damage=1,facing=east"),
            state!(15, 8, "anvil", "damage=2,facing=south"),
            state!(15, 9, "anvil", "damage=2,facing=west"),
            state!(15, 10, "anvil", "damage=2,facing=north"),
            state!(15, 11, "anvil", "damage=2,facing=east"),
            state!(15, 12, "anvil", "damage=2,facing=south"),
            state!(15, 13, "anvil", "damage=2,facing=west"),
            state!(15, 14, "anvil", "damage=2,facing=north"),
            state!(15, 15, "anvil", "damage=2,facing=east"),
        ],
    },
    BlockDefinition {
        id: 147,
        name: "light_weighted_pressure_plate",
        states: &[
            state!(15, 0, "light_weighted_pressure_plate", "power=0"),
            state!(15, 1, "light_weighted_pressure_plate", "power=1"),
            state!(15, 2, "light_weighted_pressure_plate", "power=2"),
            state!(15, 3, "light_weighted_pressure_plate", "power=3"),
            state!(15, 4, "light_weighted_pressure_plate", "power=4"),
            state!(15, 5, "light_weighted_pressure_plate", "power=5"),
            state!(15, 6, "light_weighted_pressure_plate", "power=6"),
            state!(15, 7, "light_weighted_pressure_plate", "power=7"),
            state!(15, 8, "light_weighted_pressure_plate", "power=8"),
            state!(15, 9, "light_weighted_pressure_plate", "power=9"),
            state!(15, 10, "light_weighted_pressure_plate", "power=10"),
            state!(15, 11, "light_weighted_pressure_plate", "power=11"),
            state!(15, 12, "light_weighted_pressure_plate", "power=12"),
            state!(15, 13, "light_weighted_pressure_plate", "power=13"),
            state!(15, 14, "light_weighted_pressure_plate", "power=14"),
            state!(15, 15, "light_weighted_pressure_plate", "power=15"),
        ],
    },
    BlockDefinition {
        id: 148,
        name: "heavy_weighted_pressure_plate",
        states: &[
            state!(15, 0, "heavy_weighted_pressure_plate", "power=0"),
            state!(15, 1, "heavy_weighted_pressure_plate", "power=1"),
            state!(15, 2, "heavy_weighted_pressure_plate", "power=2"),
            state!(15, 3, "heavy_weighted_pressure_plate", "power=3"),
            state!(15, 4, "heavy_weighted_pressure_plate", "power=4"),
            state!(15, 5, "heavy_weighted_pressure_plate", "power=5"),
            state!(15, 6, "heavy_weighted_pressure_plate", "power=6"),
            state!(15, 7, "heavy_weighted_pressure_plate", "power=7"),
            state!(15, 8, "heavy_weighted_pressure_plate", "power=8"),
            state!(15, 9, "heavy_weighted_pressure_plate", "power=9"),
            state!(15, 10, "heavy_weighted_pressure_plate", "power=10"),
            state!(15, 11, "heavy_weighted_pressure_plate", "power=11"),
            state!(15, 12, "heavy_weighted_pressure_plate", "power=12"),
            state!(15, 13, "heavy_weighted_pressure_plate", "power=13"),
            state!(15, 14, "heavy_weighted_pressure_plate", "power=14"),
            state!(15, 15, "heavy_weighted_pressure_plate", "power=15"),
        ],
    },
    BlockDefinition {
        id: 149,
        name: "unpowered_comparator",
        states: &[
            state!(
                15,
                0,
                "unpowered_comparator",
                "facing=south,mode=compare,powered=false"
            ),
            state!(
                15,
                1,
                "unpowered_comparator",
                "facing=west,mode=compare,powered=false"
            ),
            state!(
                15,
                2,
                "unpowered_comparator",
                "facing=north,mode=compare,powered=false"
            ),
            state!(
                15,
                3,
                "unpowered_comparator",
                "facing=east,mode=compare,powered=false"
            ),
            state!(
                15,
                4,
                "unpowered_comparator",
                "facing=south,mode=subtract,powered=false"
            ),
            state!(
                15,
                5,
                "unpowered_comparator",
                "facing=west,mode=subtract,powered=false"
            ),
            state!(
                15,
                6,
                "unpowered_comparator",
                "facing=north,mode=subtract,powered=false"
            ),
            state!(
                15,
                7,
                "unpowered_comparator",
                "facing=east,mode=subtract,powered=false"
            ),
            state!(
                15,
                8,
                "unpowered_comparator",
                "facing=south,mode=compare,powered=true"
            ),
            state!(
                15,
                9,
                "unpowered_comparator",
                "facing=west,mode=compare,powered=true"
            ),
            state!(
                15,
                10,
                "unpowered_comparator",
                "facing=north,mode=compare,powered=true"
            ),
            state!(
                15,
                11,
                "unpowered_comparator",
                "facing=east,mode=compare,powered=true"
            ),
            state!(
                15,
                12,
                "unpowered_comparator",
                "facing=south,mode=subtract,powered=true"
            ),
            state!(
                15,
                13,
                "unpowered_comparator",
                "facing=west,mode=subtract,powered=true"
            ),
            state!(
                15,
                14,
                "unpowered_comparator",
                "facing=north,mode=subtract,powered=true"
            ),
            state!(
                15,
                15,
                "unpowered_comparator",
                "facing=east,mode=subtract,powered=true"
            ),
        ],
    },
    BlockDefinition {
        id: 150,
        name: "powered_comparator",
        states: &[
            state!(
                15,
                0,
                "powered_comparator",
                "facing=south,mode=compare,powered=false"
            ),
            state!(
                15,
                1,
                "powered_comparator",
                "facing=west,mode=compare,powered=false"
            ),
            state!(
                15,
                2,
                "powered_comparator",
                "facing=north,mode=compare,powered=false"
            ),
            state!(
                15,
                3,
                "powered_comparator",
                "facing=east,mode=compare,powered=false"
            ),
            state!(
                15,
                4,
                "powered_comparator",
                "facing=south,mode=subtract,powered=false"
            ),
            state!(
                15,
                5,
                "powered_comparator",
                "facing=west,mode=subtract,powered=false"
            ),
            state!(
                15,
                6,
                "powered_comparator",
                "facing=north,mode=subtract,powered=false"
            ),
            state!(
                15,
                7,
                "powered_comparator",
                "facing=east,mode=subtract,powered=false"
            ),
            state!(
                15,
                8,
                "powered_comparator",
                "facing=south,mode=compare,powered=true"
            ),
            state!(
                15,
                9,
                "powered_comparator",
                "facing=west,mode=compare,powered=true"
            ),
            state!(
                15,
                10,
                "powered_comparator",
                "facing=north,mode=compare,powered=true"
            ),
            state!(
                15,
                11,
                "powered_comparator",
                "facing=east,mode=compare,powered=true"
            ),
            state!(
                15,
                12,
                "powered_comparator",
                "facing=south,mode=subtract,powered=true"
            ),
            state!(
                15,
                13,
                "powered_comparator",
                "facing=west,mode=subtract,powered=true"
            ),
            state!(
                15,
                14,
                "powered_comparator",
                "facing=north,mode=subtract,powered=true"
            ),
            state!(
                15,
                15,
                "powered_comparator",
                "facing=east,mode=subtract,powered=true"
            ),
        ],
    },
    BlockDefinition {
        id: 151,
        name: "daylight_detector",
        states: &[
            state!(15, 0, "daylight_detector", "power=0"),
            state!(15, 1, "daylight_detector", "power=1"),
            state!(15, 2, "daylight_detector", "power=2"),
            state!(15, 3, "daylight_detector", "power=3"),
            state!(15, 4, "daylight_detector", "power=4"),
            state!(15, 5, "daylight_detector", "power=5"),
            state!(15, 6, "daylight_detector", "power=6"),
            state!(15, 7, "daylight_detector", "power=7"),
            state!(15, 8, "daylight_detector", "power=8"),
            state!(15, 9, "daylight_detector", "power=9"),
            state!(15, 10, "daylight_detector", "power=10"),
            state!(15, 11, "daylight_detector", "power=11"),
            state!(15, 12, "daylight_detector", "power=12"),
            state!(15, 13, "daylight_detector", "power=13"),
            state!(15, 14, "daylight_detector", "power=14"),
            state!(15, 15, "daylight_detector", "power=15"),
        ],
    },
    BlockDefinition {
        id: 152,
        name: "redstone_block",
        states: &[state!(0, 0, "redstone_block", "normal")],
    },
    BlockDefinition {
        id: 153,
        name: "quartz_ore",
        states: &[state!(0, 0, "quartz_ore", "normal")],
    },
    BlockDefinition {
        id: 154,
        name: "hopper",
        states: &[
            state!(7, 0, "hopper", "facing=down"),
            state!(7, 1, "hopper", "facing=down"),
            state!(7, 2, "hopper", "facing=north"),
            state!(7, 3, "hopper", "facing=south"),
            state!(7, 4, "hopper", "facing=west"),
            state!(7, 5, "hopper", "facing=east"),
            state!(7, 6, "hopper", "facing=down"),
            state!(7, 7, "hopper", "facing=down"),
        ],
    },
    BlockDefinition {
        id: 155,
        name: "quartz_block",
        states: &[
            state!(15, 0, "quartz_block", "normal"),
            state!(15, 1, "chiseled_quartz_block", "normal"),
            state!(15, 2, "quartz_column", "axis=y"),
            state!(15, 3, "quartz_column", "axis=x"),
            state!(15, 4, "quartz_column", "axis=z"),
            state!(0, 0, "quartz_block", "normal"),
        ],
    },
    BlockDefinition {
        id: 156,
        name: "quartz_stairs",
        states: &[
            state!(
                7,
                0,
                "quartz_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "quartz_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "quartz_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "quartz_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "quartz_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "quartz_stairs", "facing=west,half=top,shape=straight"),
            state!(
                7,
                6,
                "quartz_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "quartz_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 157,
        name: "activator_rail",
        states: &[
            state!(15, 0, "activator_rail", "powered=false,shape=north_south"),
            state!(15, 1, "activator_rail", "powered=false,shape=east_west"),
            state!(
                15,
                2,
                "activator_rail",
                "powered=false,shape=ascending_east"
            ),
            state!(
                15,
                3,
                "activator_rail",
                "powered=false,shape=ascending_west"
            ),
            state!(
                15,
                4,
                "activator_rail",
                "powered=false,shape=ascending_north"
            ),
            state!(
                15,
                5,
                "activator_rail",
                "powered=false,shape=ascending_south"
            ),
            state!(15, 6, "activator_rail", "powered=false,shape=north_south"),
            state!(15, 7, "activator_rail", "powered=false,shape=north_south"),
            state!(15, 8, "activator_rail", "powered=true,shape=north_south"),
            state!(15, 9, "activator_rail", "powered=true,shape=east_west"),
            state!(
                15,
                10,
                "activator_rail",
                "powered=true,shape=ascending_east"
            ),
            state!(
                15,
                11,
                "activator_rail",
                "powered=true,shape=ascending_west"
            ),
            state!(
                15,
                12,
                "activator_rail",
                "powered=true,shape=ascending_north"
            ),
            state!(
                15,
                13,
                "activator_rail",
                "powered=true,shape=ascending_south"
            ),
            state!(15, 14, "activator_rail", "powered=true,shape=north_south"),
            state!(15, 15, "activator_rail", "powered=true,shape=north_south"),
        ],
    },
    BlockDefinition {
        id: 158,
        name: "dropper",
        states: &[
            state!(7, 0, "dropper", "facing=down"),
            state!(7, 1, "dropper", "facing=up"),
            state!(7, 2, "dropper", "facing=north"),
            state!(7, 3, "dropper", "facing=south"),
            state!(7, 4, "dropper", "facing=west"),
            state!(7, 5, "dropper", "facing=east"),
            state!(7, 6, "dropper", "facing=down"),
            state!(7, 7, "dropper", "facing=up"),
        ],
    },
    BlockDefinition {
        id: 159,
        name: "stained_hardened_clay",
        states: &[
            state!(15, 0, "white_stained_hardened_clay", "normal"),
            state!(15, 1, "orange_stained_hardened_clay", "normal"),
            state!(15, 2, "magenta_stained_hardened_clay", "normal"),
            state!(15, 3, "light_blue_stained_hardened_clay", "normal"),
            state!(15, 4, "yellow_stained_hardened_clay", "normal"),
            state!(15, 5, "lime_stained_hardened_clay", "normal"),
            state!(15, 6, "pink_stained_hardened_clay", "normal"),
            state!(15, 7, "gray_stained_hardened_clay", "normal"),
            state!(15, 8, "silver_stained_hardened_clay", "normal"),
            state!(15, 9, "cyan_stained_hardened_clay", "normal"),
            state!(15, 10, "purple_stained_hardened_clay", "normal"),
            state!(15, 11, "blue_stained_hardened_clay", "normal"),
            state!(15, 12, "brown_stained_hardened_clay", "normal"),
            state!(15, 13, "green_stained_hardened_clay", "normal"),
            state!(15, 14, "red_stained_hardened_clay", "normal"),
            state!(15, 15, "black_stained_hardened_clay", "normal"),
        ],
    },
    BlockDefinition {
        id: 160,
        name: "stained_glass_pane",
        states: &[
            state!(
                15,
                0,
                "white_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                1,
                "orange_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                2,
                "magenta_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                3,
                "light_blue_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                4,
                "yellow_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                5,
                "lime_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                6,
                "pink_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                7,
                "gray_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                8,
                "silver_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                9,
                "cyan_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                10,
                "purple_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                11,
                "blue_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                12,
                "brown_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                13,
                "green_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                14,
                "red_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
            state!(
                15,
                15,
                "black_stained_glass_pane",
                "east=false,north=false,south=false,west=false"
            ),
        ],
    },
    BlockDefinition {
        id: 161,
        name: "leaves2",
        states: &[
            state!(3, 0, "acacia_leaves", "normal"),
            state!(3, 1, "dark_oak_leaves", "normal"),
            state!(3, 2, "oak_leaves", "normal"),
            state!(3, 3, "oak_leaves", "normal"),
        ],
    },
    BlockDefinition {
        id: 162,
        name: "log2",
        states: &[
            state!(15, 0, "acacia_log", "axis=y"),
            state!(15, 1, "dark_oak_log", "axis=y"),
            state!(15, 2, "oak_log", "axis=y"),
            state!(15, 3, "oak_log", "axis=y"),
            state!(15, 4, "acacia_log", "axis=x"),
            state!(15, 5, "dark_oak_log", "axis=x"),
            state!(15, 6, "oak_log", "axis=x"),
            state!(15, 7, "oak_log", "axis=x"),
            state!(15, 8, "acacia_log", "axis=z"),
            state!(15, 9, "dark_oak_log", "axis=z"),
            state!(15, 10, "oak_log", "axis=z"),
            state!(15, 11, "oak_log", "axis=z"),
            state!(15, 12, "acacia_log", "axis=none"),
            state!(15, 13, "dark_oak_log", "axis=none"),
            state!(15, 14, "oak_log", "axis=none"),
            state!(15, 15, "oak_log", "axis=none"),
        ],
    },
    BlockDefinition {
        id: 163,
        name: "acacia_stairs",
        states: &[
            state!(
                7,
                0,
                "acacia_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "acacia_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "acacia_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "acacia_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(7, 4, "acacia_stairs", "facing=east,half=top,shape=straight"),
            state!(7, 5, "acacia_stairs", "facing=west,half=top,shape=straight"),
            state!(
                7,
                6,
                "acacia_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "acacia_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 164,
        name: "dark_oak_stairs",
        states: &[
            state!(
                7,
                0,
                "dark_oak_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "dark_oak_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "dark_oak_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "dark_oak_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(
                7,
                4,
                "dark_oak_stairs",
                "facing=east,half=top,shape=straight"
            ),
            state!(
                7,
                5,
                "dark_oak_stairs",
                "facing=west,half=top,shape=straight"
            ),
            state!(
                7,
                6,
                "dark_oak_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "dark_oak_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 165,
        name: "slime",
        states: &[state!(0, 0, "slime", "normal")],
    },
    BlockDefinition {
        id: 167,
        name: "iron_trapdoor",
        states: &[
            state!(
                15,
                0,
                "iron_trapdoor",
                "facing=north,half=bottom,open=false"
            ),
            state!(
                15,
                1,
                "iron_trapdoor",
                "facing=south,half=bottom,open=false"
            ),
            state!(15, 2, "iron_trapdoor", "facing=west,half=bottom,open=false"),
            state!(15, 3, "iron_trapdoor", "facing=east,half=bottom,open=false"),
            state!(15, 4, "iron_trapdoor", "facing=north,half=bottom,open=true"),
            state!(15, 5, "iron_trapdoor", "facing=south,half=bottom,open=true"),
            state!(15, 6, "iron_trapdoor", "facing=west,half=bottom,open=true"),
            state!(15, 7, "iron_trapdoor", "facing=east,half=bottom,open=true"),
            state!(15, 8, "iron_trapdoor", "facing=north,half=top,open=false"),
            state!(15, 9, "iron_trapdoor", "facing=south,half=top,open=false"),
            state!(15, 10, "iron_trapdoor", "facing=west,half=top,open=false"),
            state!(15, 11, "iron_trapdoor", "facing=east,half=top,open=false"),
            state!(15, 12, "iron_trapdoor", "facing=north,half=top,open=true"),
            state!(15, 13, "iron_trapdoor", "facing=south,half=top,open=true"),
            state!(15, 14, "iron_trapdoor", "facing=west,half=top,open=true"),
            state!(15, 15, "iron_trapdoor", "facing=east,half=top,open=true"),
        ],
    },
    BlockDefinition {
        id: 168,
        name: "prismarine",
        states: &[
            state!(15, 0, "prismarine", "normal"),
            state!(15, 1, "prismarine_bricks", "normal"),
            state!(15, 2, "dark_prismarine", "normal"),
            state!(0, 0, "prismarine", "normal"),
        ],
    },
    BlockDefinition {
        id: 169,
        name: "sea_lantern",
        states: &[state!(0, 0, "sea_lantern", "normal")],
    },
    BlockDefinition {
        id: 170,
        name: "hay_block",
        states: &[
            state!(12, 0, "hay_block", "axis=y"),
            state!(12, 4, "hay_block", "axis=x"),
            state!(12, 8, "hay_block", "axis=z"),
            state!(0, 0, "hay_block", "axis=y"),
        ],
    },
    BlockDefinition {
        id: 171,
        name: "carpet",
        states: &[
            state!(15, 0, "white_carpet", "normal"),
            state!(15, 1, "orange_carpet", "normal"),
            state!(15, 2, "magenta_carpet", "normal"),
            state!(15, 3, "light_blue_carpet", "normal"),
            state!(15, 4, "yellow_carpet", "normal"),
            state!(15, 5, "lime_carpet", "normal"),
            state!(15, 6, "pink_carpet", "normal"),
            state!(15, 7, "gray_carpet", "normal"),
            state!(15, 8, "silver_carpet", "normal"),
            state!(15, 9, "cyan_carpet", "normal"),
            state!(15, 10, "purple_carpet", "normal"),
            state!(15, 11, "blue_carpet", "normal"),
            state!(15, 12, "brown_carpet", "normal"),
            state!(15, 13, "green_carpet", "normal"),
            state!(15, 14, "red_carpet", "normal"),
            state!(15, 15, "black_carpet", "normal"),
        ],
    },
    BlockDefinition {
        id: 172,
        name: "hardened_clay",
        states: &[state!(0, 0, "hardened_clay", "normal")],
    },
    BlockDefinition {
        id: 173,
        name: "coal_block",
        states: &[state!(0, 0, "coal_block", "normal")],
    },
    BlockDefinition {
        id: 174,
        name: "packed_ice",
        states: &[state!(0, 0, "packed_ice", "normal")],
    },
    BlockDefinition {
        id: 175,
        name: "double_plant",
        states: &[
            state!(15, 0, "sunflower", "half=lower"),
            state!(15, 1, "syringa", "half=lower"),
            state!(15, 2, "double_grass", "half=lower"),
            state!(15, 3, "double_fern", "half=lower"),
            state!(15, 4, "double_rose", "half=lower"),
            state!(15, 5, "paeonia", "half=lower"),
            state!(15, 6, "sunflower", "half=lower"),
            state!(15, 7, "sunflower", "half=lower"),
            state!(15, 8, "sunflower", "half=upper"),
            state!(15, 9, "sunflower", "half=upper"),
            state!(15, 10, "sunflower", "half=upper"),
            state!(15, 11, "sunflower", "half=upper"),
            state!(15, 12, "sunflower", "half=upper"),
            state!(15, 13, "sunflower", "half=upper"),
            state!(15, 14, "sunflower", "half=upper"),
            state!(15, 15, "sunflower", "half=upper"),
        ],
    },
    BlockDefinition {
        id: 178,
        name: "daylight_detector_inverted",
        states: &[
            state!(15, 0, "daylight_detector_inverted", "power=0"),
            state!(15, 1, "daylight_detector_inverted", "power=1"),
            state!(15, 2, "daylight_detector_inverted", "power=2"),
            state!(15, 3, "daylight_detector_inverted", "power=3"),
            state!(15, 4, "daylight_detector_inverted", "power=4"),
            state!(15, 5, "daylight_detector_inverted", "power=5"),
            state!(15, 6, "daylight_detector_inverted", "power=6"),
            state!(15, 7, "daylight_detector_inverted", "power=7"),
            state!(15, 8, "daylight_detector_inverted", "power=8"),
            state!(15, 9, "daylight_detector_inverted", "power=9"),
            state!(15, 10, "daylight_detector_inverted", "power=10"),
            state!(15, 11, "daylight_detector_inverted", "power=11"),
            state!(15, 12, "daylight_detector_inverted", "power=12"),
            state!(15, 13, "daylight_detector_inverted", "power=13"),
            state!(15, 14, "daylight_detector_inverted", "power=14"),
            state!(15, 15, "daylight_detector_inverted", "power=15"),
        ],
    },
    BlockDefinition {
        id: 179,
        name: "red_sandstone",
        states: &[
            state!(15, 0, "red_sandstone", "normal"),
            state!(15, 1, "chiseled_red_sandstone", "normal"),
            state!(15, 2, "smooth_red_sandstone", "normal"),
            state!(0, 0, "red_sandstone", "normal"),
        ],
    },
    BlockDefinition {
        id: 180,
        name: "red_sandstone_stairs",
        states: &[
            state!(
                7,
                0,
                "red_sandstone_stairs",
                "facing=east,half=bottom,shape=straight"
            ),
            state!(
                7,
                1,
                "red_sandstone_stairs",
                "facing=west,half=bottom,shape=straight"
            ),
            state!(
                7,
                2,
                "red_sandstone_stairs",
                "facing=south,half=bottom,shape=straight"
            ),
            state!(
                7,
                3,
                "red_sandstone_stairs",
                "facing=north,half=bottom,shape=straight"
            ),
            state!(
                7,
                4,
                "red_sandstone_stairs",
                "facing=east,half=top,shape=straight"
            ),
            state!(
                7,
                5,
                "red_sandstone_stairs",
                "facing=west,half=top,shape=straight"
            ),
            state!(
                7,
                6,
                "red_sandstone_stairs",
                "facing=south,half=top,shape=straight"
            ),
            state!(
                7,
                7,
                "red_sandstone_stairs",
                "facing=north,half=top,shape=straight"
            ),
        ],
    },
    BlockDefinition {
        id: 181,
        name: "double_stone_slab2",
        states: &[
            state!(8, 0, "red_sandstone_double_slab", "normal"),
            state!(8, 8, "red_sandstone_double_slab", "all"),
        ],
    },
    BlockDefinition {
        id: 182,
        name: "stone_slab2",
        states: &[
            state!(8, 0, "red_sandstone_slab", "half=bottom"),
            state!(8, 8, "red_sandstone_slab", "half=top"),
        ],
    },
    BlockDefinition {
        id: 183,
        name: "spruce_fence_gate",
        states: &[
            state!(
                7,
                0,
                "spruce_fence_gate",
                "facing=south,in_wall=false,open=false"
            ),
            state!(
                7,
                1,
                "spruce_fence_gate",
                "facing=west,in_wall=false,open=false"
            ),
            state!(
                7,
                2,
                "spruce_fence_gate",
                "facing=north,in_wall=false,open=false"
            ),
            state!(
                7,
                3,
                "spruce_fence_gate",
                "facing=east,in_wall=false,open=false"
            ),
            state!(
                7,
                4,
                "spruce_fence_gate",
                "facing=south,in_wall=false,open=true"
            ),
            state!(
                7,
                5,
                "spruce_fence_gate",
                "facing=west,in_wall=false,open=true"
            ),
            state!(
                7,
                6,
                "spruce_fence_gate",
                "facing=north,in_wall=false,open=true"
            ),
            state!(
                7,
                7,
                "spruce_fence_gate",
                "facing=east,in_wall=false,open=true"
            ),
        ],
    },
    BlockDefinition {
        id: 184,
        name: "birch_fence_gate",
        states: &[
            state!(
                7,
                0,
                "birch_fence_gate",
                "facing=south,in_wall=false,open=false"
            ),
            state!(
                7,
                1,
                "birch_fence_gate",
                "facing=west,in_wall=false,open=false"
            ),
            state!(
                7,
                2,
                "birch_fence_gate",
                "facing=north,in_wall=false,open=false"
            ),
            state!(
                7,
                3,
                "birch_fence_gate",
                "facing=east,in_wall=false,open=false"
            ),
            state!(
                7,
                4,
                "birch_fence_gate",
                "facing=south,in_wall=false,open=true"
            ),
            state!(
                7,
                5,
                "birch_fence_gate",
                "facing=west,in_wall=false,open=true"
            ),
            state!(
                7,
                6,
                "birch_fence_gate",
                "facing=north,in_wall=false,open=true"
            ),
            state!(
                7,
                7,
                "birch_fence_gate",
                "facing=east,in_wall=false,open=true"
            ),
        ],
    },
    BlockDefinition {
        id: 185,
        name: "jungle_fence_gate",
        states: &[
            state!(
                7,
                0,
                "jungle_fence_gate",
                "facing=south,in_wall=false,open=false"
            ),
            state!(
                7,
                1,
                "jungle_fence_gate",
                "facing=west,in_wall=false,open=false"
            ),
            state!(
                7,
                2,
                "jungle_fence_gate",
                "facing=north,in_wall=false,open=false"
            ),
            state!(
                7,
                3,
                "jungle_fence_gate",
                "facing=east,in_wall=false,open=false"
            ),
            state!(
                7,
                4,
                "jungle_fence_gate",
                "facing=south,in_wall=false,open=true"
            ),
            state!(
                7,
                5,
                "jungle_fence_gate",
                "facing=west,in_wall=false,open=true"
            ),
            state!(
                7,
                6,
                "jungle_fence_gate",
                "facing=north,in_wall=false,open=true"
            ),
            state!(
                7,
                7,
                "jungle_fence_gate",
                "facing=east,in_wall=false,open=true"
            ),
        ],
    },
    BlockDefinition {
        id: 186,
        name: "dark_oak_fence_gate",
        states: &[
            state!(
                7,
                0,
                "dark_oak_fence_gate",
                "facing=south,in_wall=false,open=false"
            ),
            state!(
                7,
                1,
                "dark_oak_fence_gate",
                "facing=west,in_wall=false,open=false"
            ),
            state!(
                7,
                2,
                "dark_oak_fence_gate",
                "facing=north,in_wall=false,open=false"
            ),
            state!(
                7,
                3,
                "dark_oak_fence_gate",
                "facing=east,in_wall=false,open=false"
            ),
            state!(
                7,
                4,
                "dark_oak_fence_gate",
                "facing=south,in_wall=false,open=true"
            ),
            state!(
                7,
                5,
                "dark_oak_fence_gate",
                "facing=west,in_wall=false,open=true"
            ),
            state!(
                7,
                6,
                "dark_oak_fence_gate",
                "facing=north,in_wall=false,open=true"
            ),
            state!(
                7,
                7,
                "dark_oak_fence_gate",
                "facing=east,in_wall=false,open=true"
            ),
        ],
    },
    BlockDefinition {
        id: 187,
        name: "acacia_fence_gate",
        states: &[
            state!(
                7,
                0,
                "acacia_fence_gate",
                "facing=south,in_wall=false,open=false"
            ),
            state!(
                7,
                1,
                "acacia_fence_gate",
                "facing=west,in_wall=false,open=false"
            ),
            state!(
                7,
                2,
                "acacia_fence_gate",
                "facing=north,in_wall=false,open=false"
            ),
            state!(
                7,
                3,
                "acacia_fence_gate",
                "facing=east,in_wall=false,open=false"
            ),
            state!(
                7,
                4,
                "acacia_fence_gate",
                "facing=south,in_wall=false,open=true"
            ),
            state!(
                7,
                5,
                "acacia_fence_gate",
                "facing=west,in_wall=false,open=true"
            ),
            state!(
                7,
                6,
                "acacia_fence_gate",
                "facing=north,in_wall=false,open=true"
            ),
            state!(
                7,
                7,
                "acacia_fence_gate",
                "facing=east,in_wall=false,open=true"
            ),
        ],
    },
    BlockDefinition {
        id: 188,
        name: "spruce_fence",
        states: &[state!(
            0,
            0,
            "spruce_fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 189,
        name: "birch_fence",
        states: &[state!(
            0,
            0,
            "birch_fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 190,
        name: "jungle_fence",
        states: &[state!(
            0,
            0,
            "jungle_fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 191,
        name: "dark_oak_fence",
        states: &[state!(
            0,
            0,
            "dark_oak_fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 192,
        name: "acacia_fence",
        states: &[state!(
            0,
            0,
            "acacia_fence",
            "east=false,north=false,south=false,west=false"
        )],
    },
    BlockDefinition {
        id: 193,
        name: "spruce_door",
        states: &[
            state!(
                15,
                0,
                "spruce_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "spruce_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "spruce_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "spruce_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "spruce_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "spruce_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "spruce_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "spruce_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "spruce_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "spruce_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "spruce_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "spruce_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "spruce_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "spruce_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "spruce_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "spruce_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
    BlockDefinition {
        id: 194,
        name: "birch_door",
        states: &[
            state!(
                15,
                0,
                "birch_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "birch_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "birch_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "birch_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "birch_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "birch_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "birch_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "birch_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "birch_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "birch_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "birch_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "birch_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "birch_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "birch_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "birch_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "birch_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
    BlockDefinition {
        id: 195,
        name: "jungle_door",
        states: &[
            state!(
                15,
                0,
                "jungle_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "jungle_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "jungle_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "jungle_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "jungle_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "jungle_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "jungle_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "jungle_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "jungle_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "jungle_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "jungle_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "jungle_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "jungle_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "jungle_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "jungle_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "jungle_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
    BlockDefinition {
        id: 196,
        name: "acacia_door",
        states: &[
            state!(
                15,
                0,
                "acacia_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "acacia_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "acacia_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "acacia_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "acacia_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "acacia_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "acacia_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "acacia_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "acacia_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "acacia_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "acacia_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "acacia_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "acacia_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "acacia_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "acacia_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "acacia_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
    BlockDefinition {
        id: 197,
        name: "dark_oak_door",
        states: &[
            state!(
                15,
                0,
                "dark_oak_door",
                "facing=east,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                1,
                "dark_oak_door",
                "facing=south,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                2,
                "dark_oak_door",
                "facing=west,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                3,
                "dark_oak_door",
                "facing=north,half=lower,hinge=left,open=false"
            ),
            state!(
                15,
                4,
                "dark_oak_door",
                "facing=east,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                5,
                "dark_oak_door",
                "facing=south,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                6,
                "dark_oak_door",
                "facing=west,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                7,
                "dark_oak_door",
                "facing=north,half=lower,hinge=left,open=true"
            ),
            state!(
                15,
                8,
                "dark_oak_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                9,
                "dark_oak_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                10,
                "dark_oak_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                11,
                "dark_oak_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                12,
                "dark_oak_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                13,
                "dark_oak_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
            state!(
                15,
                14,
                "dark_oak_door",
                "facing=north,half=upper,hinge=left,open=false"
            ),
            state!(
                15,
                15,
                "dark_oak_door",
                "facing=north,half=upper,hinge=right,open=false"
            ),
        ],
    },
];

/// Definition for a numeric block id, if the id is known.
#[must_use]
pub fn definition(id: u16) -> Option<&'static BlockDefinition> {
    DEFINITIONS
        .binary_search_by_key(&id, |definition| definition.id)
        .ok()
        .map(|index| &DEFINITIONS[index])
}

/// Every known 1.8 block definition, ordered by numeric id.
#[must_use]
pub fn definitions() -> &'static [BlockDefinition] {
    DEFINITIONS
}
