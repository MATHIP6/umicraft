// SPDX-License-Identifier: MIT OR Apache-2.0

//! The advanced-mode (F3+H) parts of a tooltip (MCP `ItemStack.getTooltip`
//! with `advanced`, `ItemMap.addInformation`): the id after the name, the dye
//! colour in hex, the map scale, and the registry name and tag count at the
//! end.

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::items;
use crate::nbt::Nbt;

/// What an advanced tooltip knows beyond the stack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Advanced {
    /// The scale of the map a filled map shows (`MapData.scale`), `None` when
    /// the client has no data for it.
    pub map_scale: Option<i8>,
}

/// The id after the name, ` (#0276)` or ` (#0035/14)` for an item with
/// subtypes. The reference ends the name with `§r`, so the id is white and
/// upright whatever the rarity colour.
pub(super) fn id_suffix(item_id: i16, damage: i16) -> TooltipText {
    let id = if items::has_subtypes(item_id) {
        format!("#{item_id:04}/{damage}")
    } else {
        format!("#{item_id:04}")
    };
    TooltipText::literal(format!("§f ({id})"))
}

/// `Color: #RRGGBB` (`Integer.toHexString`, upper case) in place of "Dyed".
pub(super) fn colour_line(colour: i32) -> TooltipLine {
    TooltipLine::plain(
        TooltipText::literal(format!("Color: #{:X}", colour as u32)),
        TooltipColor::Gray,
    )
}

/// `ItemMap.addInformation` in advanced mode, English in the reference too.
pub(super) fn map_lines(scale: Option<i8>) -> Vec<TooltipLine> {
    let line = |text: String| TooltipLine::plain(TooltipText::literal(text), TooltipColor::Gray);
    match scale {
        None => vec![line("Unknown map".into())],
        Some(scale) => {
            let factor = 1_i32
                .checked_shl(u32::try_from(scale).unwrap_or(0))
                .unwrap_or(0);
            vec![
                line(format!("Scaling at 1:{factor}")),
                line(format!("(Level {scale}/4)")),
            ]
        }
    }
}

/// The two dark-grey lines after everything else: the registry name and, for
/// a stack with a tag, how many keys the tag has.
pub(super) fn end_lines(def: Option<&items::ItemDef>, root: Option<&Nbt>) -> Vec<TooltipLine> {
    let line =
        |text: String| TooltipLine::plain(TooltipText::literal(text), TooltipColor::DarkGray);
    let mut lines = Vec::new();
    if let Some(def) = def {
        lines.push(line(format!("minecraft:{}", def.name)));
    }
    if let Some(Nbt::Compound(entries)) = root {
        lines.push(line(format!("NBT: {} tag(s)", entries.len())));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_has_a_damage_only_with_subtypes() {
        assert_eq!(id_suffix(276, 0), TooltipText::literal("§f (#0276)"));
        assert_eq!(id_suffix(35, 14), TooltipText::literal("§f (#0035/14)"));
    }

    #[test]
    fn colours_print_like_to_hex_string() {
        assert_eq!(
            colour_line(0xA0_6540).text,
            TooltipText::literal("Color: #A06540")
        );
        assert_eq!(
            colour_line(-1).text,
            TooltipText::literal("Color: #FFFFFFFF")
        );
    }

    #[test]
    fn maps_show_their_scale_or_unknown() {
        assert_eq!(map_lines(None)[0].text, TooltipText::literal("Unknown map"));
        let lines = map_lines(Some(2));
        assert_eq!(lines[0].text, TooltipText::literal("Scaling at 1:4"));
        assert_eq!(lines[1].text, TooltipText::literal("(Level 2/4)"));
    }

    #[test]
    fn the_end_names_the_item_and_counts_the_tags() {
        let tag = Nbt::Compound(vec![("a".into(), Nbt::Byte(1)), ("b".into(), Nbt::Byte(2))]);
        let lines = end_lines(items::lookup(276), Some(&tag));
        assert_eq!(
            lines[0].text,
            TooltipText::literal("minecraft:diamond_sword")
        );
        assert_eq!(lines[1].text, TooltipText::literal("NBT: 2 tag(s)"));
        assert_eq!(lines[1].color, TooltipColor::DarkGray);
    }
}
