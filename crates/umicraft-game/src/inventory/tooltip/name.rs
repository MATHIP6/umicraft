// SPDX-License-Identifier: MIT OR Apache-2.0

//! Line 0 of a tooltip: the display name (MCP `ItemStack.getDisplayName`).
//!
//! A custom name wins, in italic. Otherwise the name is the language key of
//! [`items::unlocalized_name`], except for the items whose class builds the
//! name itself: the potion (effect or brewing prefix), the banner (base
//! colour from the tag), the player head (owner), the spawn egg (entity) and
//! the filled map (` #<damage>`).

use umicraft_protocol::packets::play::ItemStack;

use super::{banner, potion, Rarity, TooltipLine, TooltipText, POTION};
use crate::item_nbt::ItemDisplay;
use crate::items;
use crate::nbt::Nbt;

/// Registry name of the mob head item.
const SKULL: &str = "skull";
/// Damage value of the player head.
const PLAYER_HEAD: i16 = 3;
/// Registry name of the spawn egg, whose name ends with the spawned entity.
const SPAWN_EGG: &str = "spawn_egg";
/// Registry name of the filled map, whose name gets a ` #<damage>` suffix.
const FILLED_MAP: &str = "filled_map";

/// Line 0: the custom name in italic, else the translated item name
/// (`ItemStack.getDisplayName` and the filled-map ` #damage` suffix).
pub(super) fn name_line(
    stack: &ItemStack,
    def: Option<&items::ItemDef>,
    display: &ItemDisplay,
    root: Option<&Nbt>,
    rarity: Rarity,
) -> TooltipLine {
    let colour = rarity.color();
    if let Some(custom) = &display.custom_name {
        return TooltipLine::italic(TooltipText::literal(custom.as_str()), colour);
    }
    let Some(def) = def else {
        let fallback = format!("Unknown item {}", stack.item_id);
        return TooltipLine::plain(TooltipText::Literal(fallback), colour);
    };
    let name = TooltipText::key(format!(
        "{}.name",
        items::unlocalized_name(def, stack.damage)
    ));
    let text = if let Some(owner) = (def.name == SKULL)
        .then(|| skull_owner(stack.damage, root))
        .flatten()
    {
        TooltipText::Translation {
            key: "item.skull.player.name".to_owned(),
            args: vec![TooltipText::literal(owner)],
        }
    } else if def.name == banner::BANNER {
        let key = banner::unlocalized_name(stack.damage, root);
        TooltipText::key(format!("{key}.name"))
    } else if let Some(potion) = (def.name == POTION)
        .then(|| potion::display_name(stack.damage, root))
        .flatten()
    {
        potion
    } else if def.name == FILLED_MAP {
        let suffix = TooltipText::literal(format!(" #{}", stack.damage));
        TooltipText::Concat(vec![name, suffix])
    } else if def.name == SPAWN_EGG {
        spawn_egg_name(name, stack.damage)
    } else {
        name
    };
    TooltipLine::plain(text, colour)
}

/// The owner of a player head (`ItemSkull.getItemStackDisplayName`): the
/// `SkullOwner` string, or the `Name` of a `SkullOwner` compound. Only the
/// player head (damage 3) has one.
fn skull_owner(damage: i16, root: Option<&Nbt>) -> Option<String> {
    if damage != PLAYER_HEAD {
        return None;
    }
    let owner = root?.get("SkullOwner")?;
    owner
        .as_str()
        .or_else(|| owner.get("Name").and_then(Nbt::as_str))
        .map(str::to_owned)
}

/// `ItemMonsterPlacer.getItemStackDisplayName`: the egg name, a space and the
/// name of the entity it spawns (nothing more for an unknown entity id).
fn spawn_egg_name(egg: TooltipText, damage: i16) -> TooltipText {
    let Some(entity) = items::spawn_egg_entity(damage) else {
        return egg;
    };
    let entity = TooltipText::key(format!("entity.{entity}.name"));
    TooltipText::Concat(vec![egg, TooltipText::literal(" "), entity])
}
