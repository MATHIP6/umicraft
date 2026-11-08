// SPDX-License-Identifier: MIT OR Apache-2.0
//! Clicks per second, counted over a sliding window of one second.

use std::fmt::Write;

use super::WHITE;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("cps"),
    name_key: "umicraft.mods.cps.name",
    description_key: "umicraft.mods.cps.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = &[SettingDef {
    key: "mode",
    label_key: "umicraft.mods.cps.mode",
    kind: SettingKind::Choice {
        options: &["left", "right", "both"],
        default: 2,
    },
}];

/// Length of the sliding window.
const WINDOW_MS: u64 = 1000;

/// Frames with clicks kept per button. Only frames with a click are stored and a
/// human clicks well under 128 times a second, so the oldest is dropped only if a
/// macro floods the buffer.
const CAPACITY: usize = 128;

/// The clicks of the last second, in a ring buffer of fixed size: pushing and
/// counting never allocate.
#[derive(Debug, Clone)]
struct ClickWindow {
    /// `(time of the frame, clicks that frame)`, oldest at `head`.
    frames: [(u64, u32); CAPACITY],
    head: usize,
    len: usize,
    /// Sum of the clicks in `frames`, kept so counting is O(1).
    total: u32,
}

impl Default for ClickWindow {
    fn default() -> Self {
        Self {
            frames: [(0, 0); CAPACITY],
            head: 0,
            len: 0,
            total: 0,
        }
    }
}

impl ClickWindow {
    /// Records `clicks` at `now_ms` and forgets what left the window.
    fn record(&mut self, now_ms: u64, clicks: u32) {
        self.expire(now_ms);
        if clicks == 0 {
            return;
        }
        if self.len == CAPACITY {
            self.pop_oldest();
        }
        let tail = (self.head + self.len) % CAPACITY;
        self.frames[tail] = (now_ms, clicks);
        self.len += 1;
        self.total += clicks;
    }

    fn expire(&mut self, now_ms: u64) {
        while self.len > 0 && now_ms.saturating_sub(self.frames[self.head].0) >= WINDOW_MS {
            self.pop_oldest();
        }
    }

    fn pop_oldest(&mut self) {
        self.total -= self.frames[self.head].1;
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
    }
}

/// The clicks per second mod.
#[derive(Default)]
pub(super) struct Cps {
    left: ClickWindow,
    right: ClickWindow,
}

impl BuiltinMod for Cps {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 76.0),
        })
    }

    fn update(&mut self, ctx: &ModContext, _settings: &ModSettings) {
        self.left.record(ctx.now_ms, ctx.left_clicks);
        self.right.record(ctx.now_ms, ctx.right_clicks);
    }

    fn draw(&self, _ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        let line = out.line(WHITE);
        // Writing into a `String` cannot fail.
        let _ = match settings.text("mode") {
            "left" => write!(line, "{} CPS", self.left.total),
            "right" => write!(line, "{} CPS", self.right.total),
            _ => write!(line, "{} | {} CPS", self.left.total, self.right.total),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{assert_reads_only_schema_keys, drawn_texts};
    use crate::settings::SettingValue;

    fn frame(m: &mut Cps, now_ms: u64, left: u32, right: u32) {
        let ctx = ModContext {
            now_ms,
            left_clicks: left,
            right_clicks: right,
            ..ModContext::default()
        };
        m.update(&ctx, &ModSettings::defaults(SETTINGS));
    }

    fn shown(m: &Cps, settings: &ModSettings) -> String {
        drawn_texts(m, &ModContext::default(), settings).remove(0)
    }

    #[test]
    fn counts_each_button_in_the_last_second() {
        let mut m = Cps::default();
        frame(&mut m, 0, 1, 0);
        frame(&mut m, 100, 2, 1);
        frame(&mut m, 500, 0, 0);
        assert_eq!(shown(&m, &ModSettings::defaults(SETTINGS)), "3 | 1 CPS");
    }

    #[test]
    fn clicks_leave_the_window_after_a_second() {
        let mut m = Cps::default();
        frame(&mut m, 0, 4, 4);
        frame(&mut m, 999, 1, 0);
        assert_eq!(shown(&m, &ModSettings::defaults(SETTINGS)), "5 | 4 CPS");
        frame(&mut m, 1000, 0, 0);
        assert_eq!(shown(&m, &ModSettings::defaults(SETTINGS)), "1 | 0 CPS");
        frame(&mut m, 5000, 0, 0);
        assert_eq!(shown(&m, &ModSettings::defaults(SETTINGS)), "0 | 0 CPS");
    }

    #[test]
    fn a_single_button_can_be_shown() {
        let mut m = Cps::default();
        frame(&mut m, 0, 3, 2);
        let mut settings = ModSettings::defaults(SETTINGS);
        for (mode, expected) in [("left", "3 CPS"), ("right", "2 CPS"), ("both", "3 | 2 CPS")] {
            settings
                .set("mode", &SettingValue::Text(mode.into()))
                .expect("known key");
            assert_eq!(shown(&m, &settings), expected, "{mode}");
        }
    }

    #[test]
    fn a_full_buffer_drops_the_oldest_frame() {
        let mut m = Cps::default();
        for t in 0..(CAPACITY as u64 + 10) {
            frame(&mut m, t, 1, 0);
        }
        assert_eq!(m.left.total, CAPACITY as u32);
        assert_eq!(m.left.len, CAPACITY);
    }

    #[test]
    fn the_ring_wraps_around_correctly() {
        let mut m = Cps::default();
        for round in 0..5_u64 {
            for i in 0..100_u64 {
                frame(&mut m, round * 2000 + i, 1, 0);
            }
        }
        assert_eq!(m.left.total, 100);
    }

    #[test]
    fn reads_only_schema_keys() {
        assert_reads_only_schema_keys(&Cps::default(), &["mode"]);
    }
}
