// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ping display: "42 ms", nothing when there is no measurement.

use std::fmt::Write;

use super::hud_style::hud_settings;
use super::parse_color;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("ping"),
    name_key: "umicraft.mods.ping.name",
    description_key: "umicraft.mods.ping.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = hud_settings![SettingDef {
    key: "color",
    label_key: "umicraft.mods.ping.color",
    kind: SettingKind::Color { default: "#ffffff" },
}];

/// The ping mod.
pub(super) struct Ping;

impl BuiltinMod for Ping {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 28.0),
        })
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        if let Some(ping) = ctx.ping_ms {
            // Writing into a `String` cannot fail.
            let _ = write!(out.line(parse_color(settings.text("color"))), "{ping} ms");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::drawn_texts;

    fn texts(ping_ms: Option<u32>) -> Vec<String> {
        let ctx = ModContext {
            ping_ms,
            ..ModContext::default()
        };
        drawn_texts(&Ping, &ctx, &ModSettings::defaults(SETTINGS))
    }

    #[test]
    fn shows_the_ping() {
        assert_eq!(texts(Some(42)), ["42 ms"]);
    }

    #[test]
    fn shows_nothing_without_a_measurement() {
        assert!(texts(None).is_empty());
    }

    #[test]
    fn has_a_color_and_the_shared_hud_options() {
        let keys: Vec<_> = Ping.settings().iter().map(|def| def.key).collect();
        assert_eq!(keys.len(), 1 + crate::mods::hud_style::KEYS.len());
        assert!(keys.contains(&"color"));
    }

    #[test]
    fn uses_the_chosen_color() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set(
                "color",
                &crate::settings::SettingValue::Text("#00ff00".into()),
            )
            .expect("known key");
        let ctx = ModContext {
            ping_ms: Some(7),
            ..ModContext::default()
        };
        let mut out = HudContent::default();
        Ping.draw(&ctx, &settings, &mut out);
        assert_eq!(out.lines()[0].color, 0x00_ff00);
    }
}
