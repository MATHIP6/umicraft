// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ping display: "42 ms", nothing when there is no measurement.

use std::fmt::Write;

use super::WHITE;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::ModSettings;
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("ping"),
    name_key: "umicraft.mods.ping.name",
    description_key: "umicraft.mods.ping.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

/// The ping mod.
pub(super) struct Ping;

impl BuiltinMod for Ping {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 28.0),
        })
    }

    fn draw(&self, ctx: &ModContext, _settings: &ModSettings, out: &mut HudContent) {
        if let Some(ping) = ctx.ping_ms {
            // Writing into a `String` cannot fail.
            let _ = write!(out.line(WHITE), "{ping} ms");
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
        drawn_texts(&Ping, &ctx, &ModSettings::defaults(&[]))
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
    fn has_no_settings() {
        assert!(Ping.settings().is_empty());
    }
}
