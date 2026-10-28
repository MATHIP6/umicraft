// SPDX-License-Identifier: MIT OR Apache-2.0

//! How fast the local player digs a block.
//!
//! [`relative_hardness`] is the progress one tick of digging adds to a block
//! (MCP `Block.getPlayerRelativeBlockHardness`): the block is destroyed once
//! the sum reaches 1. It combines the hardness table of [`crate::block_data`],
//! the held tool (`Item.getStrVsBlock`, `canHarvestBlock`), the Efficiency
//! enchantment, the Haste and Mining Fatigue effects, water and flight.
//!
//! The 1.8 item registry does not exist yet (milestone P3), so the tools are
//! the 21 vanilla tool ids below.

use umicraft_protocol::packets::play::ItemStack;

use crate::block_data::{Material, BLOCKS};

/// The potion id of Haste (MCP `Potion.digSpeed`).
pub const HASTE: u8 = 3;

/// The potion id of Mining Fatigue (MCP `Potion.digSlowdown`).
pub const MINING_FATIGUE: u8 = 4;

/// The enchantment id of Efficiency (MCP `Enchantment.efficiency`).
pub const EFFICIENCY: i16 = 32;

/// The enchantment id of Aqua Affinity (MCP `Enchantment.aquaAffinity`).
pub const AQUA_AFFINITY: i16 = 6;

/// The enchantment id of Depth Strider (MCP `Enchantment.depthStrider`).
pub const DEPTH_STRIDER: i16 = 8;

const WEB: u16 = 30;
const WOOL: u16 = 35;
const REDSTONE_WIRE: u16 = 55;
const TRIPWIRE: u16 = 132;
const SNOW_LAYER: u16 = 78;
const SNOW: u16 = 80;
const OBSIDIAN: u16 = 49;

/// The blocks a shovel digs at tool speed (MCP `ItemSpade.EFFECTIVE_ON`):
/// clay, dirt, farmland, grass, gravel, mycelium, sand, snow, snow layer and
/// soul sand.
const SHOVEL_BLOCKS: [u16; 10] = [82, 3, 60, 2, 13, 110, 12, 80, 78, 88];

/// The blocks an axe digs at tool speed besides the wood, plant and vine
/// materials (MCP `ItemAxe.EFFECTIVE_ON`): planks, bookshelf, logs, chest,
/// pumpkins, melon and ladder.
const AXE_BLOCKS: [u16; 9] = [5, 47, 17, 162, 54, 86, 91, 103, 65];

/// The blocks a pickaxe digs at tool speed besides the rock, iron and anvil
/// materials (MCP `ItemPickaxe.EFFECTIVE_ON`).
const PICKAXE_BLOCKS: [u16; 25] = [
    157, 16, 4, 28, 57, 56, 43, 27, 41, 14, 79, 42, 15, 22, 21, 74, 48, 87, 174, 66, 73, 24, 179,
    1, 44,
];

/// What a tool is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Shovel,
    Pickaxe,
    Axe,
    Sword,
    Shears,
}

/// A tool material (MCP `Item.ToolMaterial`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tier {
    /// `efficiencyOnProperMaterial`.
    efficiency: f32,
    /// `harvestLevel`.
    harvest_level: u8,
}

const WOOD: Tier = Tier {
    efficiency: 2.0,
    harvest_level: 0,
};
const STONE: Tier = Tier {
    efficiency: 4.0,
    harvest_level: 1,
};
const IRON: Tier = Tier {
    efficiency: 6.0,
    harvest_level: 2,
};
const DIAMOND: Tier = Tier {
    efficiency: 8.0,
    harvest_level: 3,
};
const GOLD: Tier = Tier {
    efficiency: 12.0,
    harvest_level: 0,
};

/// A tool item.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tool {
    kind: ToolKind,
    tier: Tier,
}

/// The tool of an item id, if it is one (MCP `Item.registerItems`).
fn tool(item_id: i16) -> Option<Tool> {
    use ToolKind::{Axe, Pickaxe, Shears, Shovel, Sword};
    let (kind, tier) = match item_id {
        256 => (Shovel, IRON),
        257 => (Pickaxe, IRON),
        258 => (Axe, IRON),
        267 => (Sword, IRON),
        268 => (Sword, WOOD),
        269 => (Shovel, WOOD),
        270 => (Pickaxe, WOOD),
        271 => (Axe, WOOD),
        272 => (Sword, STONE),
        273 => (Shovel, STONE),
        274 => (Pickaxe, STONE),
        275 => (Axe, STONE),
        276 => (Sword, DIAMOND),
        277 => (Shovel, DIAMOND),
        278 => (Pickaxe, DIAMOND),
        279 => (Axe, DIAMOND),
        283 => (Sword, GOLD),
        284 => (Shovel, GOLD),
        285 => (Pickaxe, GOLD),
        286 => (Axe, GOLD),
        359 => (Shears, WOOD),
        _ => return None,
    };
    Some(Tool { kind, tier })
}

/// Whether a material needs the right tool to drop its block
/// (MCP `Material.setRequiresTool`).
fn requires_tool(material: Material) -> bool {
    matches!(
        material,
        Material::Rock
            | Material::Iron
            | Material::Anvil
            | Material::Snow
            | Material::CraftedSnow
            | Material::Web
            | Material::Barrier
    )
}

impl Tool {
    /// MCP `Item.getStrVsBlock` of the tool.
    fn strength(self, block: u16, material: Material) -> f32 {
        let tool_speed = self.tier.efficiency;
        match self.kind {
            ToolKind::Pickaxe => {
                if matches!(material, Material::Iron | Material::Anvil | Material::Rock)
                    || PICKAXE_BLOCKS.contains(&block)
                {
                    tool_speed
                } else {
                    1.0
                }
            }
            ToolKind::Axe => {
                if matches!(material, Material::Wood | Material::Plants | Material::Vine)
                    || AXE_BLOCKS.contains(&block)
                {
                    tool_speed
                } else {
                    1.0
                }
            }
            ToolKind::Shovel => {
                if SHOVEL_BLOCKS.contains(&block) {
                    tool_speed
                } else {
                    1.0
                }
            }
            ToolKind::Sword => {
                if block == WEB {
                    15.0
                } else if matches!(
                    material,
                    Material::Plants
                        | Material::Vine
                        | Material::Coral
                        | Material::Leaves
                        | Material::Gourd
                ) {
                    1.5
                } else {
                    1.0
                }
            }
            ToolKind::Shears => {
                if block == WEB || material == Material::Leaves {
                    15.0
                } else if block == WOOL {
                    5.0
                } else {
                    1.0
                }
            }
        }
    }

    /// MCP `Item.canHarvestBlock` of the tool.
    fn can_harvest(self, block: u16, material: Material) -> bool {
        let level = self.tier.harvest_level;
        match self.kind {
            ToolKind::Pickaxe => match block {
                OBSIDIAN => level == 3,
                // Diamond, emerald, gold and redstone need iron.
                57 | 56 | 129 | 133 | 41 | 14 | 73 | 74 => level >= 2,
                // Iron and lapis need stone.
                42 | 15 | 22 | 21 => level >= 1,
                _ => matches!(material, Material::Rock | Material::Iron | Material::Anvil),
            },
            ToolKind::Shovel => block == SNOW_LAYER || block == SNOW,
            ToolKind::Sword => block == WEB,
            ToolKind::Shears => matches!(block, WEB | REDSTONE_WIRE | TRIPWIRE),
            ToolKind::Axe => false,
        }
    }
}

/// The state of the player that changes how fast it digs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Digger<'a> {
    /// The held item.
    pub held: Option<&'a ItemStack>,
    /// The amplifier of the Haste effect, if active.
    pub haste: Option<i8>,
    /// The amplifier of the Mining Fatigue effect, if active.
    pub fatigue: Option<i8>,
    /// Whether the eyes are inside water.
    pub in_water: bool,
    /// Whether the helmet has Aqua Affinity.
    pub aqua_affinity: bool,
    /// Whether the player stands on the ground.
    pub on_ground: bool,
}

impl Digger<'_> {
    /// MCP `EntityPlayer.getToolDigEfficiency`.
    fn efficiency(&self, block: u16, material: Material) -> f32 {
        let held = self.held.filter(|stack| stack.item_id >= 0);
        let mut speed = held
            .and_then(|stack| tool(stack.item_id))
            .map_or(1.0, |tool| tool.strength(block, material));
        if speed > 1.0 {
            let level = held
                .and_then(|stack| stack.nbt.as_deref())
                .map_or(0, |nbt| enchantment_level(nbt, EFFICIENCY));
            if level > 0 {
                speed += (level * level + 1) as f32;
            }
        }
        if let Some(amplifier) = self.haste {
            speed *= 1.0 + (f32::from(amplifier) + 1.0) * 0.2;
        }
        if let Some(amplifier) = self.fatigue {
            speed *= match amplifier {
                0 => 0.3,
                1 => 0.09,
                2 => 0.0027,
                _ => 8.1e-4,
            };
        }
        if self.in_water && !self.aqua_affinity {
            speed /= 5.0;
        }
        if !self.on_ground {
            speed /= 5.0;
        }
        speed
    }

    /// MCP `EntityPlayer.canHarvestBlock`.
    fn can_harvest(&self, block: u16, material: Material) -> bool {
        if !requires_tool(material) {
            return true;
        }
        self.held
            .and_then(|stack| tool(stack.item_id))
            .is_some_and(|tool| tool.can_harvest(block, material))
    }
}

/// The progress one tick of digging adds to a block, `0.0` for an
/// unbreakable one (MCP `Block.getPlayerRelativeBlockHardness`).
///
/// A block is destroyed when the progress reaches `1.0`: with the right tool
/// the efficiency is divided by the hardness and by 30, without it by 100.
#[must_use]
pub fn relative_hardness(block: u16, digger: &Digger<'_>) -> f32 {
    let data = BLOCKS.get(usize::from(block)).copied().unwrap_or_default();
    if data.hardness < 0.0 {
        return 0.0;
    }
    let divisor = if digger.can_harvest(block, data.material) {
        30.0
    } else {
        100.0
    };
    digger.efficiency(block, data.material) / data.hardness / divisor
}

/// Whether an item is a tool whose damage changes while it is used, so that
/// a changed damage value is still "the same item" (MCP
/// `ItemStack.isItemStackDamageable`, reduced to the tools).
#[must_use]
pub fn is_damageable_tool(item_id: i16) -> bool {
    tool(item_id).is_some()
}

/// Whether an item is a sword (MCP `ItemSword`): a creative player holding
/// one cannot break blocks.
#[must_use]
pub fn is_sword(item_id: i16) -> bool {
    tool(item_id).is_some_and(|tool| tool.kind == ToolKind::Sword)
}

/// Whether a block is air for the digging rules (MCP `Material.air`).
#[must_use]
pub fn is_air(block: u16) -> bool {
    block == 0
}

/// The level of an enchantment in the NBT of a stack, `0` when absent or
/// when the NBT is malformed.
///
/// The NBT comes from the server, so every read is bounds-checked and the
/// nesting depth is limited. Only the `ench` list of compounds with a `short`
/// `id` and `lvl` is looked at (MCP `EnchantmentHelper.getEnchantmentLevel`).
#[must_use]
pub fn enchantment_level(nbt: &[u8], id: i16) -> u32 {
    let mut reader = NbtReader { data: nbt, pos: 0 };
    reader.find_enchantment(id).unwrap_or(0)
}

/// The deepest NBT nesting that is skipped.
const MAX_NBT_DEPTH: u32 = 32;

/// A bounds-checked cursor over NBT bytes.
struct NbtReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl NbtReader<'_> {
    fn take(&mut self, count: usize) -> Option<&[u8]> {
        let end = self.pos.checked_add(count)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn short(&mut self) -> Option<i16> {
        let bytes = self.take(2)?;
        Some(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn length(&mut self) -> Option<usize> {
        let bytes = self.take(4)?;
        usize::try_from(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])).ok()
    }

    fn name(&mut self) -> Option<&[u8]> {
        let length = usize::from(self.short()? as u16);
        self.take(length)
    }

    /// Skips the payload of a tag of type `tag`.
    fn skip(&mut self, tag: u8, depth: u32) -> Option<()> {
        if depth > MAX_NBT_DEPTH {
            return None;
        }
        match tag {
            1 => self.take(1).map(drop),
            2 => self.take(2).map(drop),
            3 | 5 => self.take(4).map(drop),
            4 | 6 => self.take(8).map(drop),
            7 => {
                let length = self.length()?;
                self.take(length).map(drop)
            }
            8 => self.name().map(drop),
            9 => {
                let item = self.byte()?;
                for _ in 0..self.length()? {
                    self.skip(item, depth + 1)?;
                }
                Some(())
            }
            10 => loop {
                let tag = self.byte()?;
                if tag == 0 {
                    return Some(());
                }
                self.name()?;
                self.skip(tag, depth + 1)?;
            },
            11 => {
                let length = self.length()?;
                self.take(length.checked_mul(4)?).map(drop)
            }
            _ => None,
        }
    }

    /// Finds the level of enchantment `id` in the root compound.
    fn find_enchantment(&mut self, id: i16) -> Option<u32> {
        if self.byte()? != 10 {
            return None;
        }
        self.name()?;
        loop {
            let tag = self.byte()?;
            if tag == 0 {
                return None;
            }
            let is_list = tag == 9 && self.name()? == b"ench";
            if tag != 9 {
                self.name()?;
            }
            if is_list {
                return self.enchantment_in_list(id);
            }
            if tag != 9 {
                self.skip(tag, 1)?;
            } else {
                self.skip(9, 1)?;
            }
        }
    }

    /// Reads an `ench` list; returns the level of `id`.
    fn enchantment_in_list(&mut self, id: i16) -> Option<u32> {
        if self.byte()? != 10 {
            return None;
        }
        for _ in 0..self.length()? {
            let (mut found, mut level) = (None, 0);
            loop {
                let tag = self.byte()?;
                if tag == 0 {
                    break;
                }
                let name = self.name()?.to_vec();
                if tag == 2 {
                    let value = self.short()?;
                    match name.as_slice() {
                        b"id" => found = Some(value),
                        b"lvl" => level = u32::from(value.max(0) as u16),
                        _ => {}
                    }
                } else {
                    self.skip(tag, 2)?;
                }
            }
            if found == Some(id) {
                return Some(level);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE_ID: u16 = 1;
    const DIRT: u16 = 3;
    const LOG: u16 = 17;
    const BEDROCK: u16 = 7;

    fn stack(item_id: i16, nbt: Option<Vec<u8>>) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage: 0,
            nbt,
        }
    }

    fn digger(held: Option<&ItemStack>) -> Digger<'_> {
        Digger {
            held,
            on_ground: true,
            ..Digger::default()
        }
    }

    /// Ticks needed to destroy a block, as the client counts them.
    fn ticks(block: u16, digger: &Digger<'_>) -> u32 {
        let per_tick = relative_hardness(block, digger);
        assert!(per_tick > 0.0);
        (1.0 / per_tick).ceil() as u32
    }

    /// An `ench` list with one enchantment, as a server would send it.
    fn enchanted(id: i16, level: i16) -> Vec<u8> {
        let mut nbt = vec![10, 0, 0, 9, 0, 4];
        nbt.extend(b"ench");
        nbt.extend([10, 0, 0, 0, 1]);
        nbt.extend([2, 0, 2]);
        nbt.extend(b"id");
        nbt.extend(id.to_be_bytes());
        nbt.extend([2, 0, 3]);
        nbt.extend(b"lvl");
        nbt.extend(level.to_be_bytes());
        nbt.extend([0, 0]);
        nbt
    }

    #[test]
    fn bare_hands_take_the_vanilla_time_on_stone() {
        // Hardness 1.5 without a pickaxe: 1 / 1.5 / 100 per tick, 150 ticks.
        assert_eq!(ticks(STONE_ID, &digger(None)), 150);
    }

    #[test]
    fn a_pickaxe_digs_stone_in_the_vanilla_time() {
        let wood = stack(270, None);
        let diamond = stack(278, None);
        // 2 / 1.5 / 30 = 0.0444 per tick -> 23 ticks; diamond 8 -> 6 ticks.
        assert_eq!(ticks(STONE_ID, &digger(Some(&wood))), 23);
        assert_eq!(ticks(STONE_ID, &digger(Some(&diamond))), 6);
    }

    #[test]
    fn dirt_is_dug_quicker_with_a_shovel() {
        let shovel = stack(269, None);
        assert!(ticks(DIRT, &digger(Some(&shovel))) < ticks(DIRT, &digger(None)));
        // 1 / 0.5 / 30 by hand, 2 / 0.5 / 30 with a wooden shovel.
        assert_eq!(ticks(DIRT, &digger(None)), 15);
        assert_eq!(ticks(DIRT, &digger(Some(&shovel))), 8);
    }

    #[test]
    fn a_wooden_pickaxe_does_not_harvest_diamond_ore_but_an_iron_one_does() {
        let wood = stack(270, None);
        let iron = stack(257, None);
        let slow = relative_hardness(56, &digger(Some(&wood)));
        let fast = relative_hardness(56, &digger(Some(&iron)));
        // Same hardness 3: the divisor is 100 without the harvest level.
        assert!((slow - 2.0 / 3.0 / 100.0).abs() < 1e-6);
        assert!((fast - 6.0 / 3.0 / 30.0).abs() < 1e-6);
    }

    #[test]
    fn an_axe_is_the_tool_for_logs() {
        let axe = stack(271, None);
        assert!(
            relative_hardness(LOG, &digger(Some(&axe))) > relative_hardness(LOG, &digger(None))
        );
    }

    #[test]
    fn bedrock_is_unbreakable() {
        let diamond = stack(278, None);
        assert_eq!(relative_hardness(BEDROCK, &digger(Some(&diamond))), 0.0);
    }

    #[test]
    fn efficiency_adds_level_squared_plus_one() {
        let plain = stack(278, None);
        let enchanted = stack(278, Some(enchanted(EFFICIENCY, 3)));
        let base = relative_hardness(STONE_ID, &digger(Some(&plain)));
        let boosted = relative_hardness(STONE_ID, &digger(Some(&enchanted)));
        assert!((boosted / base - (8.0 + 10.0) / 8.0).abs() < 1e-5);
    }

    #[test]
    fn efficiency_does_not_help_a_tool_that_is_not_effective() {
        let sword = stack(276, Some(enchanted(EFFICIENCY, 5)));
        let plain = stack(276, None);
        assert_eq!(
            relative_hardness(STONE_ID, &digger(Some(&sword))),
            relative_hardness(STONE_ID, &digger(Some(&plain)))
        );
    }

    #[test]
    fn haste_and_mining_fatigue_scale_the_speed() {
        let base = relative_hardness(STONE_ID, &digger(None));
        let haste = Digger {
            haste: Some(1),
            ..digger(None)
        };
        let fatigue = Digger {
            fatigue: Some(0),
            ..digger(None)
        };
        assert!((relative_hardness(STONE_ID, &haste) / base - 1.4).abs() < 1e-5);
        assert!((relative_hardness(STONE_ID, &fatigue) / base - 0.3).abs() < 1e-5);
    }

    #[test]
    fn water_and_the_air_each_divide_the_speed_by_five() {
        let base = relative_hardness(DIRT, &digger(None));
        let swimming = Digger {
            in_water: true,
            ..digger(None)
        };
        let airborne = Digger {
            on_ground: false,
            ..digger(None)
        };
        let both = Digger {
            in_water: true,
            on_ground: false,
            ..digger(None)
        };
        assert!((relative_hardness(DIRT, &swimming) * 5.0 - base).abs() < 1e-6);
        assert!((relative_hardness(DIRT, &airborne) * 5.0 - base).abs() < 1e-6);
        assert!((relative_hardness(DIRT, &both) * 25.0 - base).abs() < 1e-6);
        let affinity = Digger {
            aqua_affinity: true,
            ..swimming
        };
        assert_eq!(relative_hardness(DIRT, &affinity), base);
    }

    #[test]
    fn a_malformed_nbt_has_no_enchantment() {
        let mut nbt = enchanted(EFFICIENCY, 3);
        assert_eq!(enchantment_level(&nbt, EFFICIENCY), 3);
        assert_eq!(enchantment_level(&nbt, AQUA_AFFINITY), 0);
        nbt.truncate(nbt.len() - 7);
        assert_eq!(enchantment_level(&nbt, EFFICIENCY), 0);
        assert_eq!(enchantment_level(&[], EFFICIENCY), 0);
        assert_eq!(enchantment_level(&[10, 0, 0, 3], EFFICIENCY), 0);
    }

    #[test]
    fn deeply_nested_nbt_is_rejected_without_overflow() {
        let mut nbt = vec![10, 0, 0];
        for _ in 0..200 {
            nbt.extend([10, 0, 1, b'a']);
        }
        assert_eq!(enchantment_level(&nbt, EFFICIENCY), 0);
    }

    #[test]
    fn air_is_id_zero() {
        assert!(is_air(0));
        assert!(!is_air(1));
    }
}
