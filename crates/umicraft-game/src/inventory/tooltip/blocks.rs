// SPDX-License-Identifier: MIT OR Apache-2.0

//! The adventure-mode block lists of a tooltip (MCP `ItemStack.getTooltip`,
//! `CanDestroy` and `CanPlaceOn`): a blank line, a grey heading, then each
//! block's name in dark grey.

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::items;
use crate::nbt::Nbt;

/// `HideFlags` bits of the two lists.
const HIDE_CAN_DESTROY: i32 = 8;
const HIDE_CAN_PLACE_ON: i32 = 16;
/// The highest block id: block items share the block's id below it.
const LAST_BLOCK_ID: u16 = 255;

/// Both lists, unless `hide` hides them.
pub(super) fn lines(root: Option<&Nbt>, hide: i32) -> Vec<TooltipLine> {
    let mut lines = Vec::new();
    if hide & HIDE_CAN_DESTROY == 0 {
        lines.extend(list(root, "CanDestroy", "item.canBreak"));
    }
    if hide & HIDE_CAN_PLACE_ON == 0 {
        lines.extend(list(root, "CanPlaceOn", "item.canPlace"));
    }
    lines
}

/// One list, nothing when it is missing or empty.
fn list(root: Option<&Nbt>, key: &str, heading: &str) -> Vec<TooltipLine> {
    let names = root
        .and_then(|tag| tag.get(key))
        .and_then(Nbt::as_list)
        .unwrap_or_default();
    if names.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![
        TooltipLine::plain(TooltipText::literal(""), TooltipColor::Gray),
        TooltipLine::plain(TooltipText::key(heading), TooltipColor::Gray),
    ];
    lines.extend(names.iter().map(|name| {
        let text = name
            .as_str()
            .and_then(block_name)
            .unwrap_or_else(|| TooltipText::literal("missingno"));
        TooltipLine::plain(text, TooltipColor::DarkGray)
    }));
    lines
}

/// `Block.getBlockFromName(...).getLocalizedName()`: the name of a block by
/// registry name (with or without `minecraft:`) or number.
///
/// The name comes from the block's item, which carries the block's
/// unlocalised name; a block without an item (water, fire, portal) is not
/// found and shows `missingno`, where the reference names it.
fn block_name(name: &str) -> Option<TooltipText> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let def = match name.parse::<i32>() {
        Ok(id) => items::lookup(id),
        Err(_) => items::lookup_by_name(name),
    }
    .filter(|def| def.id <= LAST_BLOCK_ID)?;
    Some(TooltipText::key(format!(
        "{}.name",
        items::block_unlocalized_name(def)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(key: &str, names: &[&str]) -> Nbt {
        Nbt::Compound(vec![(
            key.into(),
            Nbt::List(names.iter().map(|n| Nbt::String((*n).into())).collect()),
        )])
    }

    #[test]
    fn can_destroy_lists_each_block() {
        let root = tag("CanDestroy", &["minecraft:stone", "dirt", "unknown"]);
        let lines = lines(Some(&root), 0);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[1].text, TooltipText::key("item.canBreak"));
        assert_eq!(lines[2].text, TooltipText::key("tile.stone.name"));
        assert_eq!(lines[2].color, TooltipColor::DarkGray);
        assert_eq!(lines[4].text, TooltipText::literal("missingno"));
    }

    #[test]
    fn hide_flags_hide_the_lists() {
        let root = tag("CanPlaceOn", &["stone"]);
        assert_eq!(lines(Some(&root), 0).len(), 3);
        assert!(lines(Some(&root), HIDE_CAN_PLACE_ON).is_empty());
    }
}
