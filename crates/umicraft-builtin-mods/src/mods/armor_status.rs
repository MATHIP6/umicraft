// SPDX-License-Identifier: MIT OR Apache-2.0
//! Armor Status, in the style of the PvP clients: one row per worn piece, the
//! item's icon with the durability left beside it in white, and the held item
//! under them. On the right half of the screen the client mirrors the rows
//! (text, then icon), so the icons stay against the screen edge.

use std::fmt::Write;

use super::hud_style::hud_settings_bare;
use super::parse_color;
use crate::context::{ArmorPiece, ModContext};
use crate::hud::{Anchor, HudContent, HudIcon, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("armor_status"),
    name_key: "umicraft.mods.armor_status.name",
    description_key: "umicraft.mods.armor_status.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = hud_settings_bare![
    SettingDef {
        key: "invert",
        label_key: "umicraft.mods.armor_status.invert",
        kind: SettingKind::Toggle { default: false },
    },
    SettingDef {
        key: "durability",
        label_key: "umicraft.mods.armor_status.durability",
        kind: SettingKind::Choice {
            options: &["value", "percent", "off"],
            default: 0,
        },
    },
    SettingDef {
        key: "damage_color",
        label_key: "umicraft.mods.armor_status.damage_color",
        kind: SettingKind::Choice {
            options: &["text", "dynamic"],
            default: 0,
        },
    },
    SettingDef {
        key: "color",
        label_key: "umicraft.mods.armor_status.color",
        kind: SettingKind::Color { default: "#ffffff" },
    },
    SettingDef {
        key: "held",
        label_key: "umicraft.mods.armor_status.held",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "names",
        label_key: "umicraft.mods.armor_status.names",
        kind: SettingKind::Toggle { default: false },
    },
];

/// More than this share left is green with the dynamic colour.
const GOOD: f32 = 0.6;
/// More than this share left is yellow; less is red.
const WORN: f32 = 0.3;

/// The armor status mod.
pub(super) struct ArmorStatus;

/// `diamond_helmet` as `Diamond Helmet`.
fn pretty_name(name: &str, out: &mut String) {
    for (index, word) in name.split('_').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let mut letters = word.chars();
        if let Some(first) = letters.next() {
            out.extend(first.to_uppercase());
            out.push_str(letters.as_str());
        }
    }
}

/// The share of the piece that is left, `None` for one that does not wear.
fn left(piece: &ArmorPiece) -> Option<f32> {
    (piece.max_damage > 0).then(|| {
        let max = f32::from(piece.max_damage);
        ((max - f32::from(piece.damage)) / max).clamp(0.0, 1.0)
    })
}

/// Green, yellow or red by the share left (the "dynamic" damage colour).
fn wear_color(share: f32) -> u32 {
    if share <= WORN {
        0xff_5555
    } else if share <= GOOD {
        0xff_ff55
    } else {
        0x55_ff55
    }
}

impl BuiltinMod for ArmorStatus {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::BottomRight, -4.0, -4.0),
        })
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        out.invert = settings.bool("invert");
        let text_color = parse_color(settings.text("color"));
        let dynamic = settings.text("damage_color") == "dynamic";
        let names = settings.bool("names");
        let durability = settings.text("durability");
        let held = settings
            .bool("held")
            .then_some(ctx.held)
            .flatten()
            .filter(|piece| piece.max_damage > 0)
            .map(|piece| (HudIcon::Held, piece));
        let worn = ctx
            .armor
            .iter()
            .enumerate()
            .filter_map(|(slot, piece)| piece.map(|piece| (HudIcon::Armor(slot as u8), piece)));
        for (icon, piece) in worn.chain(held) {
            let share = left(&piece);
            let color = match share {
                Some(share) if dynamic => wear_color(share),
                _ => text_color,
            };
            let line = out.icon_line(icon, color);
            if names {
                pretty_name(piece.name, &mut line.text);
            }
            let Some(share) = share else { continue };
            let remaining = i32::from(piece.max_damage) - i32::from(piece.damage);
            let separator = if names { " " } else { "" };
            // Writing into a `String` cannot fail.
            let _ = match durability {
                "percent" => write!(line.text, "{separator}{:.0}%", share * 100.0),
                "off" => Ok(()),
                _ => write!(line.text, "{separator}{remaining}"),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{assert_reads_only_schema_keys, drawn_texts};
    use crate::settings::SettingValue;

    fn piece(name: &'static str, damage: i16, max_damage: u16) -> Option<ArmorPiece> {
        Some(ArmorPiece {
            name,
            damage,
            max_damage,
        })
    }

    fn ctx() -> ModContext {
        ModContext {
            armor: [
                piece("diamond_helmet", 13, 363),
                None,
                piece("iron_leggings", 200, 225),
                piece("pumpkin", 0, 0),
            ],
            held: piece("diamond_sword", 61, 1561),
            ..ModContext::default()
        }
    }

    #[test]
    fn each_piece_is_an_icon_with_its_durability_in_white() {
        let mut out = HudContent::default();
        ArmorStatus.draw(&ctx(), &ModSettings::defaults(SETTINGS), &mut out);
        let lines: Vec<_> = out
            .lines()
            .iter()
            .map(|l| (l.icon, l.text.as_str(), l.color))
            .collect();
        assert_eq!(
            lines,
            [
                (HudIcon::Armor(0), "350", 0xff_ffff),
                (HudIcon::Armor(2), "25", 0xff_ffff),
                (HudIcon::Armor(3), "", 0xff_ffff),
                (HudIcon::Held, "1500", 0xff_ffff),
            ]
        );
    }

    #[test]
    fn the_dynamic_colour_percentages_and_names_are_options() {
        let mut settings = ModSettings::defaults(SETTINGS);
        for (key, value) in [
            ("durability", SettingValue::Text("percent".into())),
            ("damage_color", SettingValue::Text("dynamic".into())),
            ("names", SettingValue::Bool(true)),
            ("held", SettingValue::Bool(false)),
        ] {
            settings.set(key, &value).expect("known key");
        }
        let mut out = HudContent::default();
        ArmorStatus.draw(&ctx(), &settings, &mut out);
        let lines: Vec<_> = out
            .lines()
            .iter()
            .map(|l| (l.text.as_str(), l.color))
            .collect();
        assert_eq!(
            lines,
            [
                ("Diamond Helmet 96%", 0x55_ff55),
                ("Iron Leggings 11%", 0xff_5555),
                ("Pumpkin", 0xff_ffff),
            ]
        );
    }

    #[test]
    fn the_invert_option_reaches_the_content() {
        let mut settings = ModSettings::defaults(SETTINGS);
        let mut out = HudContent::default();
        ArmorStatus.draw(&ctx(), &settings, &mut out);
        assert!(!out.invert, "off by default");
        settings
            .set("invert", &SettingValue::Bool(true))
            .expect("known key");
        ArmorStatus.draw(&ctx(), &settings, &mut out);
        assert!(out.invert);
    }

    #[test]
    fn no_armour_draws_nothing() {
        let out = drawn_texts(
            &ArmorStatus,
            &ModContext::default(),
            &ModSettings::defaults(SETTINGS),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn reads_only_its_schema() {
        assert_reads_only_schema_keys(
            &ArmorStatus,
            &[
                "durability",
                "damage_color",
                "color",
                "held",
                "names",
                "invert",
            ],
        );
    }
}
