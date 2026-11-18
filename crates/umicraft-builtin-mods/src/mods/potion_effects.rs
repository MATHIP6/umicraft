// SPDX-License-Identifier: MIT OR Apache-2.0
//! Potion Effects, in the style of the PvP clients: each active effect as its
//! status icon, with its name and level beside it and the time left under
//! the name in grey; the time blinks through the last ten seconds.

use std::fmt::Write;

use super::hud_style::hud_settings_bare;
use super::parse_color;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudIcon, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("potion_effects"),
    name_key: "umicraft.mods.potion_effects.name",
    description_key: "umicraft.mods.potion_effects.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = hud_settings_bare![
    SettingDef {
        key: "invert",
        label_key: "umicraft.mods.potion_effects.invert",
        kind: SettingKind::Toggle { default: false },
    },
    SettingDef {
        key: "icons",
        label_key: "umicraft.mods.potion_effects.icons",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "names",
        label_key: "umicraft.mods.potion_effects.names",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "colors",
        label_key: "umicraft.mods.potion_effects.colors",
        kind: SettingKind::Toggle { default: false },
    },
    SettingDef {
        key: "name_color",
        label_key: "umicraft.mods.potion_effects.name_color",
        kind: SettingKind::Color { default: "#ffffff" },
    },
    SettingDef {
        key: "duration_color",
        label_key: "umicraft.mods.potion_effects.duration_color",
        kind: SettingKind::Color { default: "#aaaaaa" },
    },
    SettingDef {
        key: "blink",
        label_key: "umicraft.mods.potion_effects.blink",
        kind: SettingKind::Toggle { default: true },
    },
];

/// The English name and the liquid colour of each 1.8 effect, by id - 1
/// (`Potion.potionTypes`, `Potion.liquidColor`).
const EFFECTS: [(&str, u32); 23] = [
    ("Speed", 0x7c_afc6),
    ("Slowness", 0x5a_6c81),
    ("Haste", 0xd9_c043),
    ("Mining Fatigue", 0x4a_4217),
    ("Strength", 0x93_2423),
    ("Instant Health", 0xf8_2423),
    ("Instant Damage", 0x43_0a09),
    ("Jump Boost", 0x22_ff4c),
    ("Nausea", 0x55_1d4a),
    ("Regeneration", 0xcd_5cab),
    ("Resistance", 0x99_453a),
    ("Fire Resistance", 0xe4_9a3a),
    ("Water Breathing", 0x2e_5299),
    ("Invisibility", 0x7f_8392),
    ("Blindness", 0x1f_1f23),
    ("Night Vision", 0x1f_1fa1),
    ("Hunger", 0x58_7653),
    ("Weakness", 0x48_4d48),
    ("Poison", 0x4e_9331),
    ("Wither", 0x35_2a27),
    ("Health Boost", 0xf8_7d23),
    ("Absorption", 0x25_52a5),
    ("Saturation", 0xf8_2423),
];

/// The ticks under which a timer blinks (ten seconds).
const BLINK_TICKS: i32 = 200;
/// The colour of a timer while it blinks off.
const DIM: u32 = 0x55_5555;
/// The `duration` the server sends for an endless effect.
const ENDLESS: i32 = 32_767;

/// The potion effects mod.
pub(super) struct PotionEffects;

/// `I` to `X`, the level numerals of the game; above, the number.
fn roman(level: i32, out: &mut String) {
    const NUMERALS: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    match usize::try_from(level - 1)
        .ok()
        .and_then(|i| NUMERALS.get(i))
    {
        Some(numeral) => out.push_str(numeral),
        None => {
            let _ = write!(out, "{level}");
        }
    }
}

impl BuiltinMod for PotionEffects {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopRight, -4.0, 4.0),
        })
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        out.invert = settings.bool("invert");
        let icons = settings.bool("icons");
        let names = settings.bool("names");
        let colors = settings.bool("colors");
        let name_color = parse_color(settings.text("name_color"));
        let duration_color = parse_color(settings.text("duration_color"));
        let blink = settings.bool("blink");
        for effect in ctx.effects.iter().flatten() {
            let Some(&(name, liquid)) = usize::from(effect.id)
                .checked_sub(1)
                .and_then(|index| EFFECTS.get(index))
            else {
                continue;
            };
            let ending = effect.ticks < BLINK_TICKS && effect.ticks != ENDLESS;
            // Blinks twice a second through its last ten seconds.
            let dim = blink && ending && (ctx.now_ms / 250) % 2 == 1;
            let color = if colors { liquid } else { name_color };
            let icon = if icons {
                HudIcon::Effect(effect.id)
            } else {
                HudIcon::None
            };
            let line = out.icon_line(icon, color);
            if names {
                line.text.push_str(name);
                let level = i32::from(effect.amplifier) + 1;
                if level > 1 {
                    line.text.push(' ');
                    roman(level, &mut line.text);
                }
            }
            line.detail_color = if dim { DIM } else { duration_color };
            // Writing into a `String` cannot fail.
            let _ = if effect.ticks == ENDLESS {
                write!(line.detail, "**:**")
            } else {
                let seconds = effect.ticks.max(0) / 20;
                write!(line.detail, "{}:{:02}", seconds / 60, seconds % 60)
            };
            if !names {
                // Only the time: it becomes the line itself.
                line.text = std::mem::take(&mut line.detail);
                line.color = line.detail_color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::EffectInfo;
    use crate::mods::assert_reads_only_schema_keys;

    fn ctx(now_ms: u64) -> ModContext {
        let mut effects = [None; crate::context::MAX_EFFECTS];
        effects[0] = Some(EffectInfo {
            id: 1,
            amplifier: 1,
            ticks: 20 * 83,
        });
        effects[1] = Some(EffectInfo {
            id: 12,
            amplifier: 0,
            ticks: ENDLESS,
        });
        effects[2] = Some(EffectInfo {
            id: 5,
            amplifier: 0,
            ticks: 60,
        });
        ModContext {
            now_ms,
            effects,
            ..ModContext::default()
        }
    }

    fn lines(now_ms: u64, settings: &ModSettings) -> Vec<(HudIcon, String, String, u32)> {
        let mut out = HudContent::default();
        PotionEffects.draw(&ctx(now_ms), settings, &mut out);
        out.lines()
            .iter()
            .map(|l| (l.icon, l.text.clone(), l.detail.clone(), l.detail_color))
            .collect()
    }

    #[test]
    fn each_effect_is_an_icon_a_name_and_the_time_under_it() {
        let settings = ModSettings::defaults(SETTINGS);
        let grey = 0xaa_aaaa;
        assert_eq!(
            lines(0, &settings),
            [
                (HudIcon::Effect(1), "Speed II".into(), "1:23".into(), grey),
                (
                    HudIcon::Effect(12),
                    "Fire Resistance".into(),
                    "**:**".into(),
                    grey
                ),
                (HudIcon::Effect(5), "Strength".into(), "0:03".into(), grey),
            ]
        );
    }

    #[test]
    fn an_ending_effect_blinks() {
        let settings = ModSettings::defaults(SETTINGS);
        assert_eq!(lines(250, &settings)[2].3, DIM);
        assert_eq!(lines(250, &settings)[0].3, 0xaa_aaaa, "not ending");
    }

    #[test]
    fn without_names_the_time_is_the_line() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set("names", &crate::settings::SettingValue::Bool(false))
            .expect("known key");
        let first = &lines(0, &settings)[0];
        assert_eq!((first.1.as_str(), first.2.as_str()), ("1:23", ""));
    }

    #[test]
    fn the_invert_option_reaches_the_content() {
        let mut settings = ModSettings::defaults(SETTINGS);
        let mut out = HudContent::default();
        PotionEffects.draw(&ctx(0), &settings, &mut out);
        assert!(!out.invert, "off by default");
        settings
            .set("invert", &crate::settings::SettingValue::Bool(true))
            .expect("known key");
        PotionEffects.draw(&ctx(0), &settings, &mut out);
        assert!(out.invert);
    }

    #[test]
    fn levels_are_roman_numerals() {
        let mut text = String::new();
        roman(4, &mut text);
        text.push(',');
        roman(12, &mut text);
        assert_eq!(text, "IV,12");
    }

    #[test]
    fn reads_only_its_schema() {
        assert_reads_only_schema_keys(
            &PotionEffects,
            &[
                "icons",
                "names",
                "colors",
                "name_color",
                "duration_color",
                "blink",
                "invert",
            ],
        );
    }
}
