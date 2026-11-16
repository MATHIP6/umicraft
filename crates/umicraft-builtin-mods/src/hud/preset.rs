// SPDX-License-Identifier: MIT OR Apache-2.0

//! The ready-made layouts of the HUD editor: which widgets are on and where.
//!
//! Only the widgets that exist are named here; one a preset does not name is
//! turned off by it.

use super::{Anchor, HudPlacement};

/// A layout the HUD editor can apply in one click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudPreset {
    /// Every widget where its mod puts it, each mod on or off as it ships.
    Default,
    /// What a fight needs, out of the way: FPS and ping at the top left,
    /// clicks per second at the top right.
    Pvp,
    /// The FPS counter alone, small.
    Minimal,
    /// The layout the player last saved.
    Custom,
}

impl HudPreset {
    /// Whether the widget of the mod `id` is on in this preset and where it
    /// is; `None` for a widget the preset does not name, and for the two
    /// presets that are not a table ([`Default`](Self::Default) and
    /// [`Custom`](Self::Custom)).
    #[must_use]
    pub fn widget(self, id: &str) -> Option<(bool, HudPlacement)> {
        let small = |placement: HudPlacement| HudPlacement {
            scale: 0.8,
            ..placement
        };
        let placement = match (self, id) {
            (Self::Pvp, "fps") => HudPlacement::at(Anchor::TopLeft, 4.0, 4.0),
            (Self::Pvp, "ping") => HudPlacement::at(Anchor::TopLeft, 4.0, 20.0),
            (Self::Pvp, "cps") => HudPlacement::at(Anchor::TopRight, -4.0, 4.0),
            (Self::Minimal, "fps") => small(HudPlacement::at(Anchor::TopLeft, 2.0, 2.0)),
            _ => return None,
        };
        Some((true, placement))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods;

    #[test]
    fn the_presets_only_name_widgets_that_exist() {
        let hud_ids: Vec<&str> = mods::all()
            .iter()
            .filter(|module| module.hud().is_some())
            .map(|module| module.meta().id.as_str())
            .collect();
        for preset in [HudPreset::Pvp, HudPreset::Minimal] {
            for id in ["fps", "ping", "cps", "coordinates"] {
                if preset.widget(id).is_some() {
                    assert!(hud_ids.contains(&id), "{id} has no widget");
                }
            }
            assert!(
                hud_ids.iter().any(|id| preset.widget(id).is_some()),
                "{preset:?} shows nothing"
            );
        }
        assert_eq!(HudPreset::Default.widget("fps"), None);
        assert_eq!(HudPreset::Custom.widget("fps"), None);
    }

    #[test]
    fn the_minimal_preset_is_the_fps_counter_alone() {
        assert!(HudPreset::Minimal.widget("fps").is_some());
        assert!(HudPreset::Minimal.widget("cps").is_none());
        assert!(HudPreset::Pvp.widget("coordinates").is_none());
    }
}
