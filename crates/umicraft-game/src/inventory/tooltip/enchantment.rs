// SPDX-License-Identifier: MIT OR Apache-2.0

//! Enchantment lines of a tooltip (MCP `Enchantment.getTranslatedName`): the
//! `ench` list of an item and the stored enchantments of a book.

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::enchantments;
use crate::item_nbt::Enchantment;
use crate::nbt::Nbt;

/// Highest level that has an `enchantment.level.N` translation.
const MAX_NAMED_LEVEL: u16 = 10;

/// The `StoredEnchantments` list of an enchanted book
/// (`ItemEnchantedBook.getEnchantments`).
pub(crate) fn stored_enchantments(root: Option<&Nbt>) -> Vec<Enchantment> {
    let list = root
        .and_then(|r| r.get("StoredEnchantments"))
        .and_then(Nbt::as_list)
        .unwrap_or_default();
    list.iter()
        .map(|entry| {
            let short = |key| entry.get(key).and_then(Nbt::as_int).unwrap_or(0);
            Enchantment {
                id: short("id") as i16,
                level: short("lvl").clamp(0, i32::from(u16::MAX)) as u16,
            }
        })
        .collect()
}

/// One enchantment line (`Enchantment.getTranslatedName`), `None` for an id
/// the 1.8 table does not know (vanilla skips those).
pub(super) fn enchantment_line(enchantment: &Enchantment) -> Option<TooltipLine> {
    let text = enchantment_name(enchantment.id, enchantment.level)?;
    Some(TooltipLine::plain(text, TooltipColor::Gray))
}

/// `Enchantment.getTranslatedName(level)`: the name and the level in roman
/// numerals, `None` for an id the 1.8 table does not know.
pub(super) fn enchantment_name(id: i16, level: u16) -> Option<TooltipText> {
    let key = enchantments::lookup(id)?.key;
    let level = if (1..=MAX_NAMED_LEVEL).contains(&level) {
        TooltipText::key(format!("enchantment.level.{level}"))
    } else {
        TooltipText::literal(level.to_string())
    };
    Some(TooltipText::Concat(vec![
        TooltipText::key(key),
        TooltipText::literal(" "),
        level,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_use_the_family_keys_and_roman_levels() {
        assert_eq!(
            enchantment_name(17, 2),
            Some(TooltipText::Concat(vec![
                TooltipText::key("enchantment.damage.undead"),
                TooltipText::literal(" "),
                TooltipText::key("enchantment.level.2"),
            ]))
        );
        assert_eq!(enchantment_name(9, 1), None);
        assert!(matches!(
            enchantment_name(0, 11),
            Some(TooltipText::Concat(parts)) if parts[2] == TooltipText::literal("11")
        ));
    }
}
