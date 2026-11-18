// SPDX-License-Identifier: MIT OR Apache-2.0
//! Keystrokes: the movement keys and the mouse buttons as a grid of boxes that
//! light up while held, with the clicks per second under each button, as the
//! PvP clients show them.

use std::fmt::Write;

use super::cps::ClickWindow;
use super::hud_style::hud_settings;
use super::parse_color;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("keystrokes"),
    name_key: "umicraft.mods.keystrokes.name",
    description_key: "umicraft.mods.keystrokes.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = hud_settings![
    SettingDef {
        key: "mouse",
        label_key: "umicraft.mods.keystrokes.mouse",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "cps",
        label_key: "umicraft.mods.keystrokes.cps",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "space",
        label_key: "umicraft.mods.keystrokes.space",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "color",
        label_key: "umicraft.mods.keystrokes.color",
        kind: SettingKind::Color { default: "#ffffff" },
    },
];

/// The side of a key box, in GUI units.
const KEY: f32 = 22.0;
/// The gap between two boxes.
const GAP: f32 = 2.0;
/// The height of the space bar box.
const SPACE_HEIGHT: f32 = 10.0;

/// The keystrokes mod.
#[derive(Default)]
pub(super) struct Keystrokes {
    left: ClickWindow,
    right: ClickWindow,
}

impl BuiltinMod for Keystrokes {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 90.0),
        })
    }

    fn update(&mut self, ctx: &ModContext, _settings: &ModSettings) {
        self.left.record(ctx.now_ms, ctx.left_clicks);
        self.right.record(ctx.now_ms, ctx.right_clicks);
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        let color = parse_color(settings.text("color"));
        let keys = ctx.keys;
        let step = KEY + GAP;
        let width = 3.0 * KEY + 2.0 * GAP;
        out.cell([step, 0.0, KEY, KEY], keys.forward, color)
            .push('W');
        out.cell([0.0, step, KEY, KEY], keys.left, color).push('A');
        out.cell([step, step, KEY, KEY], keys.back, color).push('S');
        out.cell([2.0 * step, step, KEY, KEY], keys.right, color)
            .push('D');
        let mut y = 2.0 * step;
        if settings.bool("mouse") {
            let half = (width - GAP) / 2.0;
            let cps = settings.bool("cps");
            let buttons = [
                (0.0, "LMB", keys.attack, self.left.total),
                (half + GAP, "RMB", keys.use_item, self.right.total),
            ];
            for (x, name, held, clicks) in buttons {
                let text = out.cell([x, y, half, KEY], held, color);
                // Writing into a `String` cannot fail.
                let _ = if cps {
                    write!(text, "{clicks} CPS")
                } else {
                    write!(text, "{name}")
                };
            }
            y += step;
        }
        if settings.bool("space") {
            out.cell([0.0, y, width, SPACE_HEIGHT], keys.jump, color)
                .push_str("---");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::KeyState;
    use crate::mods::assert_reads_only_schema_keys;
    use crate::settings::SettingValue;

    fn draw(keystrokes: &Keystrokes, ctx: &ModContext, settings: &ModSettings) -> HudContent {
        let mut out = HudContent::default();
        keystrokes.draw(ctx, settings, &mut out);
        out
    }

    #[test]
    fn the_held_keys_light_up() {
        let ctx = ModContext {
            keys: KeyState {
                forward: true,
                attack: true,
                ..KeyState::default()
            },
            ..ModContext::default()
        };
        let out = draw(
            &Keystrokes::default(),
            &ctx,
            &ModSettings::defaults(SETTINGS),
        );
        let lit: Vec<_> = out
            .cells()
            .iter()
            .filter(|cell| cell.pressed)
            .map(|cell| cell.text.as_str())
            .collect();
        assert_eq!(lit, ["W", "0 CPS"]);
        assert_eq!(out.cells().len(), 7, "WASD, two buttons, the space bar");
    }

    #[test]
    fn the_buttons_count_the_clicks_of_the_last_second() {
        let mut keystrokes = Keystrokes::default();
        let settings = ModSettings::defaults(SETTINGS);
        for (now_ms, left_clicks) in [(0, 2), (400, 3), (900, 1)] {
            keystrokes.update(
                &ModContext {
                    now_ms,
                    left_clicks,
                    ..ModContext::default()
                },
                &settings,
            );
        }
        let out = draw(&keystrokes, &ModContext::default(), &settings);
        assert_eq!(out.cells()[4].text, "6 CPS");
        assert_eq!(out.cells()[5].text, "0 CPS");
    }

    #[test]
    fn the_rows_can_be_hidden() {
        let mut settings = ModSettings::defaults(SETTINGS);
        for key in ["mouse", "space"] {
            settings
                .set(key, &SettingValue::Bool(false))
                .expect("known key");
        }
        let out = draw(&Keystrokes::default(), &ModContext::default(), &settings);
        assert_eq!(out.cells().len(), 4);
        assert!(out.lines().is_empty());
    }

    #[test]
    fn reads_only_its_schema() {
        assert_reads_only_schema_keys(&Keystrokes::default(), &["mouse", "cps", "space", "color"]);
    }
}
