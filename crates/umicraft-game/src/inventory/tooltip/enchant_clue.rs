// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tooltip of an enchanting table option row (MCP
//! `GuiEnchantment.drawScreen`): the clue the server sent, then what the
//! enchantment costs.

use super::{enchantment_name, TooltipColor, TooltipLine, TooltipText};

/// First window property of the clues (`ContainerEnchantment.updateProgressBar`:
/// ids 4 to 6 fill `field_178151_h`, the enchantment id in the low byte and
/// the level in the second).
const CLUE_PROPERTY: usize = 4;
/// What a clue property holds before the server sends one.
const NO_CLUE: i32 = -1;

/// What the tooltip of one option row depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantRow<'a> {
    /// The window properties of the enchanting table.
    pub properties: &'a [i16],
    /// The row, 0 to 2.
    pub row: u8,
    /// The lapis in the lapis slot.
    pub lapis: u8,
    /// The experience level of the player.
    pub xp_level: i32,
    /// Whether the player is in creative mode (no cost lines).
    pub creative: bool,
}

/// The lines of the row's tooltip, or `None` when vanilla shows none (no cost,
/// or no clue yet).
///
/// The clue is white and italic (`container.enchant.clue`). Outside creative a
/// blank line follows, then the red "Level Requirement" (English in the
/// reference too) when the level is missing, or the lapis line (red when the
/// slot holds too little) and the level line in grey.
#[must_use]
pub fn enchant_row_lines(state: EnchantRow<'_>) -> Option<Vec<TooltipLine>> {
    let property = |id: usize| state.properties.get(id).map(|&v| i32::from(v));
    let row = usize::from(state.row);
    let cost = property(row).unwrap_or(0);
    let clue = property(CLUE_PROPERTY + row).unwrap_or(NO_CLUE);
    if cost <= 0 || clue < 0 {
        return None;
    }
    let mut lines = Vec::new();
    let level = u16::try_from((clue & 0xFF00) >> 8).unwrap_or(0);
    let name = i16::try_from(clue & 0xFF)
        .ok()
        .and_then(|id| enchantment_name(id, level));
    if let Some(name) = name {
        lines.push(TooltipLine {
            text: TooltipText::Translation {
                key: "container.enchant.clue".into(),
                args: vec![name],
            },
            color: TooltipColor::White,
            italic: true,
        });
    }
    if state.creative {
        return Some(lines);
    }
    lines.push(TooltipLine::plain(
        TooltipText::literal(""),
        TooltipColor::Gray,
    ));
    if state.xp_level < cost {
        lines.push(TooltipLine::plain(
            TooltipText::literal(format!("Level Requirement: {cost}")),
            TooltipColor::Red,
        ));
        return Some(lines);
    }
    let needed = state.row + 1;
    let lapis_color = if state.lapis >= needed {
        TooltipColor::Gray
    } else {
        TooltipColor::Red
    };
    lines.push(TooltipLine::plain(
        count_text("container.enchant.lapis", needed),
        lapis_color,
    ));
    lines.push(TooltipLine::plain(
        count_text("container.enchant.level", needed),
        TooltipColor::Gray,
    ));
    Some(lines)
}

/// `<key>.one`, or `<key>.many` with the count.
fn count_text(key: &str, count: u8) -> TooltipText {
    if count == 1 {
        TooltipText::key(format!("{key}.one"))
    } else {
        TooltipText::Translation {
            key: format!("{key}.many"),
            args: vec![TooltipText::literal(count.to_string())],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Costs 3, 9, 30 and a sharpness II clue (16 | 2 << 8) on every row.
    const PROPERTIES: [i16; 7] = [3, 9, 30, 0, 528, 528, 528];

    fn row(row: u8, lapis: u8, xp_level: i32, creative: bool) -> Option<Vec<TooltipLine>> {
        enchant_row_lines(EnchantRow {
            properties: &PROPERTIES,
            row,
            lapis,
            xp_level,
            creative,
        })
    }

    fn texts(lines: &[TooltipLine]) -> Vec<(&TooltipText, TooltipColor)> {
        lines.iter().map(|line| (&line.text, line.color)).collect()
    }

    #[test]
    fn the_clue_comes_first_white_and_italic() {
        let lines = row(0, 3, 30, true).expect("a tooltip");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].italic);
        assert_eq!(lines[0].color, TooltipColor::White);
        assert_eq!(
            lines[0].text,
            TooltipText::Translation {
                key: "container.enchant.clue".into(),
                args: vec![enchantment_name(16, 2).expect("sharpness")],
            }
        );
    }

    #[test]
    fn a_missing_level_shows_the_requirement() {
        let lines = row(2, 3, 10, false).expect("a tooltip");
        assert_eq!(
            texts(&lines[1..]),
            [
                (&TooltipText::literal(""), TooltipColor::Gray),
                (
                    &TooltipText::literal("Level Requirement: 30"),
                    TooltipColor::Red
                ),
            ]
        );
    }

    #[test]
    fn the_cost_lines_count_lapis_and_levels() {
        let lines = row(1, 1, 30, false).expect("a tooltip");
        assert_eq!(
            texts(&lines[2..]),
            [
                (&count_text("container.enchant.lapis", 2), TooltipColor::Red),
                (
                    &count_text("container.enchant.level", 2),
                    TooltipColor::Gray
                ),
            ]
        );
        let one = row(0, 1, 30, false).expect("a tooltip");
        assert_eq!(one[2].text, TooltipText::key("container.enchant.lapis.one"));
        assert_eq!(one[2].color, TooltipColor::Gray);
    }

    #[test]
    fn no_cost_or_no_clue_has_no_tooltip() {
        let empty = |properties: &[i16]| {
            enchant_row_lines(EnchantRow {
                properties,
                row: 0,
                lapis: 3,
                xp_level: 30,
                creative: false,
            })
        };
        assert_eq!(empty(&[0, 0, 0, 0, 528]), None);
        assert_eq!(empty(&[5, 0, 0, 0, -1]), None);
        assert_eq!(empty(&[5]), None);
    }
}
