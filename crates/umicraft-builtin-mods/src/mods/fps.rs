// SPDX-License-Identifier: MIT OR Apache-2.0
//! The FPS counter: "144 FPS", optionally followed by the frame time.

use std::fmt::Write;

use super::parse_color;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("fps"),
    name_key: "umicraft.mods.fps.name",
    description_key: "umicraft.mods.fps.desc",
    category: ModCategory::Hud,
    default_enabled: true,
};

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "frame_time",
        label_key: "umicraft.mods.fps.frame_time",
        kind: SettingKind::Toggle { default: false },
    },
    SettingDef {
        key: "color",
        label_key: "umicraft.mods.fps.color",
        kind: SettingKind::Color { default: "#ffffff" },
    },
];

/// The FPS counter mod.
pub(super) struct Fps;

impl BuiltinMod for Fps {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 4.0),
        })
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        let line = out.line(parse_color(settings.text("color")));
        // Writing into a `String` cannot fail.
        let _ = write!(line, "{} FPS", ctx.fps);
        if settings.bool("frame_time") {
            let _ = write!(line, " ({:.1} ms)", ctx.frame_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{assert_reads_only_schema_keys, drawn_texts};
    use crate::settings::SettingValue;

    const CTX: ModContext = ModContext {
        now_ms: 0,
        fps: 144,
        frame_ms: 6.94,
        position: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        sprinting: false,
        left_clicks: 0,
        right_clicks: 0,
        ping_ms: None,
    };

    #[test]
    fn shows_the_fps() {
        let settings = ModSettings::defaults(SETTINGS);
        assert_eq!(drawn_texts(&Fps, &CTX, &settings), ["144 FPS"]);
    }

    #[test]
    fn shows_the_frame_time_when_asked() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set("frame_time", &SettingValue::Bool(true))
            .expect("known key");
        assert_eq!(drawn_texts(&Fps, &CTX, &settings), ["144 FPS (6.9 ms)"]);
    }

    #[test]
    fn uses_the_chosen_color() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set("color", &SettingValue::Text("#00ff00".into()))
            .expect("known key");
        let mut out = HudContent::default();
        Fps.draw(&CTX, &settings, &mut out);
        assert_eq!(out.lines()[0].color, 0x00_ff00);
    }

    #[test]
    fn is_on_by_default_at_the_top_left() {
        assert!(META.default_enabled);
        let placement = Fps.hud().expect("a widget").default_placement;
        assert_eq!(placement.anchor, Anchor::TopLeft);
    }

    #[test]
    fn reads_only_schema_keys() {
        assert_reads_only_schema_keys(&Fps, &["frame_time", "color"]);
    }
}
