// SPDX-License-Identifier: MIT OR Apache-2.0
//! The combo: hits in a row on others without being hit back, as the PvP
//! clients count it.
//!
//! A hit counts when the server confirmed it (the target was hurt right after
//! the attack). The combo ends when the player is hurt or after two seconds
//! without a hit.

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
    id: ModId::new("combo_counter"),
    name_key: "umicraft.mods.combo_counter.name",
    description_key: "umicraft.mods.combo_counter.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = hud_settings![SettingDef {
    key: "color",
    label_key: "umicraft.mods.combo_counter.color",
    kind: SettingKind::Color { default: "#ffffff" },
},];

/// Time without a hit after which the combo ends.
const COMBO_TIMEOUT_MS: u64 = 2000;

/// The combo counter mod.
#[derive(Default)]
pub(super) struct ComboCounter {
    seen_hits: u32,
    seen_hurts: u32,
    combo: u32,
    last_hit_ms: u64,
    started: bool,
}

impl BuiltinMod for ComboCounter {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 112.0),
        })
    }

    fn update(&mut self, ctx: &ModContext, _settings: &ModSettings) {
        let combat = ctx.combat;
        if self.started {
            if combat.hurts != self.seen_hurts {
                self.combo = 0;
            }
            let new_hits = combat.hits.wrapping_sub(self.seen_hits);
            if new_hits > 0 {
                self.combo = self.combo.saturating_add(new_hits);
                self.last_hit_ms = ctx.now_ms;
            }
            if ctx.now_ms.saturating_sub(self.last_hit_ms) >= COMBO_TIMEOUT_MS {
                self.combo = 0;
            }
        }
        self.seen_hits = combat.hits;
        self.seen_hurts = combat.hurts;
        self.started = true;
    }

    fn draw(&self, _ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        let line = out.line(parse_color(settings.text("color")));
        // Writing into a `String` cannot fail.
        let _ = if self.combo == 0 {
            write!(line, "No Combo")
        } else {
            write!(line, "{} Combo", self.combo)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CombatInfo;
    use crate::mods::drawn_texts;

    fn frame(m: &mut ComboCounter, now_ms: u64, hits: u32, hurts: u32) -> String {
        let ctx = ModContext {
            now_ms,
            combat: CombatInfo { hits, hurts },
            ..ModContext::default()
        };
        let settings = ModSettings::defaults(SETTINGS);
        m.update(&ctx, &settings);
        drawn_texts(m, &ctx, &settings).remove(0)
    }

    #[test]
    fn hits_in_a_row_build_the_combo() {
        let mut m = ComboCounter::default();
        assert_eq!(frame(&mut m, 0, 0, 0), "No Combo");
        assert_eq!(frame(&mut m, 500, 1, 0), "1 Combo");
        assert_eq!(frame(&mut m, 1000, 2, 0), "2 Combo");
        assert_eq!(frame(&mut m, 1500, 3, 0), "3 Combo");
    }

    #[test]
    fn being_hurt_ends_the_combo() {
        let mut m = ComboCounter::default();
        frame(&mut m, 0, 0, 0);
        frame(&mut m, 100, 2, 0);
        assert_eq!(frame(&mut m, 200, 2, 1), "No Combo");
        assert_eq!(frame(&mut m, 300, 3, 1), "1 Combo");
    }

    #[test]
    fn two_seconds_without_a_hit_end_the_combo() {
        let mut m = ComboCounter::default();
        frame(&mut m, 0, 0, 0);
        frame(&mut m, 100, 1, 0);
        assert_eq!(frame(&mut m, 2099, 1, 0), "1 Combo");
        assert_eq!(frame(&mut m, 2100, 1, 0), "No Combo");
    }

    #[test]
    fn the_counts_of_a_running_session_are_not_a_combo() {
        let mut m = ComboCounter::default();
        assert_eq!(frame(&mut m, 0, 40, 7), "No Combo");
    }
}
