// SPDX-License-Identifier: MIT OR Apache-2.0

//! Text of the item tooltip under the pointer; the box is drawn by
//! [`umicraft_ui::widgets::tooltip`] (MCP `GuiScreen.drawHoveringText`).
//!
//! The lines come from [`umicraft_game::inventory::tooltip`]. [`resolve`]
//! turns their language keys into text with the current [`Lang`]
//! (ADR-0039); a key the language does not know (the item registry's names
//! are not all vanilla keys yet) becomes a humanised last key segment
//! (`item.diamond_sword.name` gives "Diamond Sword").

use umicraft_core::lang::Lang;
use umicraft_game::inventory::tooltip::{TooltipLine, TooltipText};
use umicraft_ui::widgets::tooltip::{self, TooltipRow};
use umicraft_ui::{FontAtlas, Rect, Ui};

pub(crate) use umicraft_ui::widgets::tooltip::text_color;

/// The text of a tooltip text in the current language.
pub fn resolve(text: &TooltipText) -> String {
    resolve_in(&crate::language::current(), text)
}

/// The text of a tooltip text in `lang`.
pub fn resolve_in(lang: &Lang, text: &TooltipText) -> String {
    match text {
        TooltipText::Literal(text) => text.clone(),
        TooltipText::Concat(parts) => parts.iter().map(|part| resolve_in(lang, part)).collect(),
        TooltipText::Translation { key, args } => {
            if lang.get(key).is_none() {
                return humanise(key);
            }
            let args: Vec<String> = args.iter().map(|arg| resolve_in(lang, arg)).collect();
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            lang.tr_args(key, &args)
        }
    }
}

/// The key's most telling segment in title case: `item.diamond_sword.name`
/// gives "Diamond Sword", `attribute.name.generic.attackDamage` "Attack Damage".
fn humanise(key: &str) -> String {
    let segment = key
        .rsplit('.')
        .find(|s| !matches!(*s, "name" | "desc"))
        .unwrap_or(key);
    let mut words = String::with_capacity(segment.len() + 4);
    let mut start_of_word = true;
    for c in segment.chars() {
        if c == '_' {
            words.push(' ');
            start_of_word = true;
        } else if c.is_uppercase() && !start_of_word {
            words.push(' ');
            words.extend(c.to_uppercase());
            start_of_word = false;
        } else if start_of_word {
            words.extend(c.to_uppercase());
            start_of_word = false;
        } else {
            words.push(c);
        }
    }
    words
}

/// Draws `lines` in a vanilla tooltip box next to `pointer`.
///
/// `scale` is the GUI scale factor; `viewport` bounds the box.
pub fn draw_tooltip(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    scale: f32,
    pointer: [f32; 2],
    lines: &[TooltipLine],
) {
    let rows: Vec<TooltipRow> = lines
        .iter()
        .map(|line| TooltipRow {
            text: resolve(&line.text),
            rgb: line.color.rgb(),
        })
        .collect();
    tooltip::tooltip(ui, atlas, viewport, scale, pointer, &rows);
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::inventory::tooltip::{tooltip, tooltip_advanced};
    use umicraft_protocol::packets::play::ItemStack;

    fn key(key: &str) -> TooltipText {
        TooltipText::Translation {
            key: key.into(),
            args: Vec::new(),
        }
    }

    #[test]
    fn item_keys_become_title_case_names() {
        assert_eq!(
            resolve_in(&lang(), &key("item.diamond_sword.name")),
            "Diamond Sword"
        );
        assert_eq!(resolve_in(&lang(), &key("tile.stone.name")), "Stone");
        assert_eq!(
            resolve_in(&lang(), &key("attribute.name.generic.attackDamage")),
            "Attack Damage"
        );
    }

    /// A language that has the keys the tests use, so no vanilla text is
    /// repeated in the code under test.
    fn lang() -> Lang {
        Lang::from_texts(
            "xx_XX",
            None,
            Some(
                "enchantment.damage.all=Edge\nenchantment.level.3=III\n\
                 attribute.modifier.plus.0=+%d %s\nitem.durability=Wear: %s / %s\n\
                 itemGroup.misc=Stuff\ninventory.binSlot=Bin\n",
            ),
        )
    }

    #[test]
    fn enchantments_and_levels_come_from_the_language() {
        let text = TooltipText::Concat(vec![
            key("enchantment.damage.all"),
            TooltipText::Literal(" ".into()),
            key("enchantment.level.3"),
        ]);
        assert_eq!(resolve_in(&lang(), &text), "Edge III");
    }

    #[test]
    fn the_attack_damage_line_fills_the_format_arguments() {
        let text = TooltipText::Translation {
            key: "attribute.modifier.plus.0".into(),
            args: vec![
                TooltipText::Literal("7".into()),
                key("attribute.name.generic.attackDamage"),
            ],
        };
        assert_eq!(resolve_in(&lang(), &text), "+7 Attack Damage");
    }

    #[test]
    fn the_durability_line_fills_the_format_arguments() {
        let stack = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 61,
            nbt: None,
        };
        let lines = tooltip_advanced(&stack, true);
        let last = lines.last().expect("lines");
        assert_eq!(resolve_in(&lang(), &last.text), "Wear: 1500 / 1561");
    }

    #[test]
    fn a_sword_tooltip_resolves_to_readable_lines() {
        let stack = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let lines: Vec<String> = tooltip(&stack)
            .iter()
            .map(|l| resolve_in(&lang(), &l.text))
            .collect();
        assert_eq!(lines[0], "Diamond Sword");
    }

    #[test]
    fn creative_tab_names_and_the_bin_slot_come_from_the_language() {
        let text = |key: &str| {
            resolve_in(
                &lang(),
                &TooltipText::Translation {
                    key: key.to_owned(),
                    args: Vec::new(),
                },
            )
        };
        assert_eq!(text("itemGroup.misc"), "Stuff");
        assert_eq!(text("inventory.binSlot"), "Bin");
    }
}
