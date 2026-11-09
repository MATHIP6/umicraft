// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scene of the `mods` and `mods_list` views of the UI demo (ADR-0065): the
//! built-in mods with FPS, coordinates and CPS on and spread over the screen, so
//! the capture shows the HUD widgets around the open panel. `mods_list` is the
//! same scene under its former name.

use umicraft_builtin_mods::{mods, Anchor, HudPlacement, ModContext, ModRegistry};
use umicraft_core::lang::Lang;
use umicraft_ui::menus::ModsView;
use umicraft_ui::FontAtlas;

use crate::{hud_editor_view, mods_view};

/// The mods menu of the demo: every built-in mod listed, three HUD mods on at
/// their own corner of the screen.
#[must_use]
pub fn view(lang: &Lang, font: &FontAtlas, display: (u32, u32)) -> ModsView {
    let mut registry = match ModRegistry::new(mods::all()) {
        Ok(registry) => registry,
        Err(error) => {
            tracing::error!(%error, "no mods to show in the demo");
            return ModsView::default();
        }
    };
    let places = [
        ("fps", Anchor::TopLeft, 1.0),
        ("coordinates", Anchor::BottomLeft, 1.0),
        ("cps", Anchor::MiddleRight, 1.5),
    ];
    for (id, anchor, scale) in places {
        let placement = HudPlacement {
            scale,
            ..HudPlacement::at(anchor, 0.0, 0.0)
        };
        // The ids are the built-in ones, so these cannot fail.
        let _ = registry.set_enabled(id, true);
        let _ = registry.set_placement(id, placement);
    }
    registry.update(&ModContext {
        fps: 144,
        position: [123.4, 64.0, -56.7],
        ..ModContext::default()
    });
    ModsView {
        hud: hud_editor_view::build(&registry, lang, font, display),
        ..mods_view::build(&registry, lang)
    }
}
