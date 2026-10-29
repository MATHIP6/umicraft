// SPDX-License-Identifier: MIT OR Apache-2.0

//! Text of the item tooltip under the pointer; the box is drawn by
//! [`umicraft_ui::widgets::tooltip`] (MCP `GuiScreen.drawHoveringText`).
//!
//! The lines come from [`umicraft_game::inventory::tooltip`]. There is no
//! language table in the client yet, so [`resolve`] turns the language keys
//! into English text itself: a small table for the keys the tooltip emits and
//! a humanised last key segment for everything else (`item.diamond_sword.name`
//! becomes "Diamond Sword").

use umicraft_game::inventory::tooltip::{TooltipLine, TooltipText};
use umicraft_ui::widgets::tooltip::{self, TooltipRow};
use umicraft_ui::{FontAtlas, Rect, Ui};

pub(crate) use umicraft_ui::widgets::tooltip::text_color;

const ENCHANTMENTS: &[(&str, &str)] = &[
    ("protect.all", "Protection"),
    ("protect.fire", "Fire Protection"),
    ("protect.fall", "Feather Falling"),
    ("protect.explosion", "Blast Protection"),
    ("protect.projectile", "Projectile Protection"),
    ("oxygen", "Respiration"),
    ("waterWorker", "Aqua Affinity"),
    ("waterWalker", "Depth Strider"),
    ("damage.all", "Sharpness"),
    ("damage.undead", "Smite"),
    ("damage.arthropods", "Bane of Arthropods"),
    ("fire", "Fire Aspect"),
    ("digging", "Efficiency"),
    ("untouching", "Silk Touch"),
    ("durability", "Unbreaking"),
    ("lootBonus", "Looting"),
    ("lootBonusDigger", "Fortune"),
    ("lootBonusFishing", "Luck of the Sea"),
    ("fishingSpeed", "Lure"),
    ("arrowDamage", "Power"),
    ("arrowKnockback", "Punch"),
    ("arrowFire", "Flame"),
    ("arrowInfinite", "Infinity"),
];

const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];

/// The English text of a tooltip text.
pub fn resolve(text: &TooltipText) -> String {
    match text {
        TooltipText::Literal(text) => text.clone(),
        TooltipText::Concat(parts) => parts.iter().map(resolve).collect(),
        TooltipText::Translation { key, args } => {
            let args: Vec<String> = args.iter().map(resolve).collect();
            translate(key, &args)
        }
    }
}

fn translate(key: &str, args: &[String]) -> String {
    match key {
        "item.dyed" => return "Dyed".to_owned(),
        "item.unbreakable" => return "Unbreakable".to_owned(),
        "attribute.modifier.plus.0" => return format!("+{}", args.join(" ")),
        "item.durability" => return format!("Durability: {}", args.join(" / ")),
        _ => {}
    }
    if let Some(level) = key.strip_prefix("enchantment.level.") {
        let roman = level
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| ROMAN.get(n));
        return roman.map_or_else(|| level.to_owned(), |r| (*r).to_owned());
    }
    if let Some(name) = key.strip_prefix("enchantment.") {
        if let Some((_, english)) = ENCHANTMENTS.iter().find(|(k, _)| *k == name) {
            return (*english).to_owned();
        }
    }
    if let Some(name) = item_group_name(key) {
        return name.to_owned();
    }
    if key == "inventory.binSlot" {
        return "Delete Items".to_owned();
    }
    if let Some(record) = key
        .strip_prefix("item.record.")
        .and_then(|k| k.strip_suffix(".desc"))
    {
        return format!("C418 - {record}");
    }
    humanise(key)
}

/// The English name of a creative tab (`itemGroup.<label>` in `en_US.lang`).
fn item_group_name(key: &str) -> Option<&'static str> {
    let name = match key.strip_prefix("itemGroup.")? {
        "buildingBlocks" => "Building Blocks",
        "decorations" => "Decoration Blocks",
        "redstone" => "Redstone",
        "transportation" => "Transportation",
        "misc" => "Miscellaneous",
        "search" => "Search Items",
        "food" => "Foodstuffs",
        "tools" => "Tools",
        "combat" => "Combat",
        "brewing" => "Brewing",
        "materials" => "Materials",
        "inventory" => "Survival Inventory",
        _ => return None,
    };
    Some(name)
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
        assert_eq!(resolve(&key("item.diamond_sword.name")), "Diamond Sword");
        assert_eq!(resolve(&key("tile.stone.name")), "Stone");
        assert_eq!(
            resolve(&key("attribute.name.generic.attackDamage")),
            "Attack Damage"
        );
    }

    #[test]
    fn enchantments_and_levels_use_their_english_names() {
        let text = TooltipText::Concat(vec![
            key("enchantment.damage.all"),
            TooltipText::Literal(" ".into()),
            key("enchantment.level.3"),
        ]);
        assert_eq!(resolve(&text), "Sharpness III");
    }

    #[test]
    fn the_attack_damage_line_reads_like_vanilla() {
        let text = TooltipText::Translation {
            key: "attribute.modifier.plus.0".into(),
            args: vec![
                TooltipText::Literal("7".into()),
                key("attribute.name.generic.attackDamage"),
            ],
        };
        assert_eq!(resolve(&text), "+7 Attack Damage");
    }

    #[test]
    fn the_durability_line_reads_like_vanilla() {
        let stack = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 61,
            nbt: None,
        };
        let lines = tooltip_advanced(&stack, true);
        let last = lines.last().expect("lines");
        assert_eq!(resolve(&last.text), "Durability: 1500 / 1561");
    }

    #[test]
    fn a_sword_tooltip_resolves_to_readable_lines() {
        let stack = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let lines: Vec<String> = tooltip(&stack).iter().map(|l| resolve(&l.text)).collect();
        assert_eq!(lines[0], "Diamond Sword");
    }

    #[test]
    fn creative_tab_names_and_the_bin_slot_have_english_text() {
        let text = |key: &str| {
            resolve(&TooltipText::Translation {
                key: key.to_owned(),
                args: Vec::new(),
            })
        };
        assert_eq!(text("itemGroup.buildingBlocks"), "Building Blocks");
        assert_eq!(text("itemGroup.misc"), "Miscellaneous");
        assert_eq!(text("itemGroup.inventory"), "Survival Inventory");
        assert_eq!(text("inventory.binSlot"), "Delete Items");
    }
}
