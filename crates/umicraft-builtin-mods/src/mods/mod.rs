// SPDX-License-Identifier: MIT OR Apache-2.0
//! The built-in mods themselves, one file each.
//!
//! To add a mod: write its file, declare it here and add it to [`all`]. Nothing else
//! in the client changes, except the language keys of the mod (name, description and
//! one per setting) in the client pack's `en_US.lang` and `fr_FR.lang`; the test at
//! the bottom of this file fails if one is missing.

pub(crate) mod hud_style;

mod coordinates;
mod cps;
mod fps;
mod fullbright;
mod motion_blur;
mod ping;
mod toggle_sprint;

use crate::traits::BuiltinMod;

/// White, the default color of every text widget.
const WHITE: u32 = 0xff_ffff;

/// Every built-in mod, in the order the menu lists them within a category.
#[must_use]
pub fn all() -> Vec<Box<dyn BuiltinMod>> {
    vec![
        Box::new(fps::Fps),
        Box::new(coordinates::Coordinates),
        Box::new(cps::Cps::default()),
        Box::new(ping::Ping),
        Box::new(motion_blur::MotionBlur),
        Box::new(fullbright::Fullbright),
        Box::new(toggle_sprint::ToggleSprint::default()),
    ]
}

/// Reads a `#rrggbb` color setting as `0xRRGGBB`. The settings are sanitized, so the
/// fallback to white only guards a mod reading a key that is not a color.
pub(crate) fn parse_color(hex: &str) -> u32 {
    hex.strip_prefix('#')
        .and_then(|digits| u32::from_str_radix(digits, 16).ok())
        .unwrap_or(WHITE)
}

/// Draws a mod with the given settings and returns the text of its lines.
#[cfg(test)]
fn drawn_texts(
    module: &dyn BuiltinMod,
    ctx: &crate::context::ModContext,
    settings: &crate::settings::ModSettings,
) -> Vec<String> {
    let mut out = crate::hud::HudContent::default();
    module.draw(ctx, settings, &mut out);
    out.lines().iter().map(|l| l.text.clone()).collect()
}

/// Asserts that every key a mod reads is in its schema.
#[cfg(test)]
fn assert_reads_only_schema_keys(module: &dyn BuiltinMod, keys: &[&str]) {
    let settings = crate::settings::ModSettings::defaults(module.settings());
    for key in keys {
        assert!(settings.get(key).is_some(), "`{key}` is not in the schema");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::id::ModCategory;
    use crate::registry::ModRegistry;
    use crate::settings::SettingKind;

    const EN: &str = include_str!("../../../umicraft-assets/client-pack/umicraft/lang/en_US.lang");
    const FR: &str = include_str!("../../../umicraft-assets/client-pack/umicraft/lang/fr_FR.lang");

    fn keys_of(lang: &str) -> HashSet<&str> {
        lang.lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('=').map(|(k, _)| k))
            .collect()
    }

    /// Every language key the menu will look up for the built-in mods.
    fn required_keys() -> Vec<String> {
        let mut keys: Vec<String> = ModCategory::ALL
            .iter()
            .map(|c| c.lang_key().to_owned())
            .collect();
        for module in all() {
            let meta = module.meta();
            keys.push(meta.name_key.to_owned());
            keys.push(meta.description_key.to_owned());
            for def in module.settings() {
                keys.push(def.label_key.to_owned());
                if let SettingKind::Choice { options, .. } = def.kind {
                    keys.extend(options.iter().map(|o| format!("{}.{o}", def.label_key)));
                }
            }
        }
        keys
    }

    #[test]
    fn ids_are_unique_and_the_registry_accepts_them() {
        let mods = all();
        let ids: HashSet<_> = mods.iter().map(|m| m.meta().id).collect();
        assert_eq!(ids.len(), mods.len());
        assert!(ModRegistry::new(all()).is_ok());
    }

    #[test]
    fn keys_follow_the_naming_scheme() {
        for module in all() {
            let id = module.meta().id;
            assert_eq!(module.meta().name_key, format!("umicraft.mods.{id}.name"));
            assert_eq!(
                module.meta().description_key,
                format!("umicraft.mods.{id}.desc")
            );
            for def in module.settings() {
                let own = format!("umicraft.mods.{id}.{}", def.key);
                let shared = format!("umicraft.mods.hud.{}", def.key);
                assert!(
                    def.label_key == own || def.label_key == shared,
                    "{} is neither {own} nor {shared}",
                    def.label_key
                );
            }
        }
    }

    #[test]
    fn every_language_key_exists_in_both_languages() {
        let (en, fr) = (keys_of(EN), keys_of(FR));
        for key in required_keys() {
            assert!(en.contains(key.as_str()), "{key} missing in en_US.lang");
            assert!(fr.contains(key.as_str()), "{key} missing in fr_FR.lang");
        }
    }

    #[test]
    fn schema_defaults_are_valid() {
        for module in all() {
            for def in module.settings() {
                let default = def.default_value();
                assert_eq!(def.sanitize(&default), default, "{}", def.key);
            }
        }
    }

    #[test]
    fn colors_parse_with_a_white_fallback() {
        assert_eq!(parse_color("#ff8000"), 0xff_8000);
        assert_eq!(parse_color("nope"), WHITE);
    }
}
