// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the HUD layer shows and what it reports.
//!
//! Like the rest of the mods menu, the layer knows nothing of the built-in mods crate: the
//! client converts its registry into these plain types and applies the
//! [`HudEditorOutcome`] the layer answers with. The placement maths (anchors,
//! snapping) stays in that crate; the layer only gets the snapping as a
//! function ([`Snap`]).

use crate::geom::Rect;
use crate::mod_widget::ModWidgetView;

/// One HUD widget as the layer previews it.
#[derive(Debug, Clone, PartialEq)]
pub struct HudWidgetView {
    /// The id of the mod that owns the widget.
    pub id: String,
    /// The translated name of the mod, shown while the widget is selected.
    pub name: String,
    /// The widget as the HUD draws it, its position already resolved in GUI units.
    pub widget: ModWidgetView,
}

/// Everything the HUD layer draws.
#[derive(Debug, Clone, PartialEq)]
pub struct HudEditorView {
    /// The widgets of the enabled HUD mods, the last one drawn on top.
    pub widgets: Vec<HudWidgetView>,
    /// The size of the screen in GUI units, the space the widgets are placed in.
    pub screen: [f32; 2],
    /// The smallest and the largest widget scale.
    pub scale_range: (f32, f32),
}

impl Default for HudEditorView {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            screen: [0.0; 2],
            scale_range: (1.0, 1.0),
        }
    }
}

/// Snaps a rectangle (GUI units) to the screen edges and centre; the second
/// argument is the screen size in GUI units.
pub type Snap<'a> = &'a dyn Fn(Rect, [f32; 2]) -> Rect;

/// What the player did with the widgets.
#[derive(Debug, Clone, PartialEq)]
pub enum HudEditorOutcome {
    /// A widget was dropped or resized: the rectangle it now covers, its scale
    /// and the screen size (GUI units) both are relative to.
    Placed {
        /// The id of the mod.
        id: String,
        /// The rectangle of the widget.
        rect: Rect,
        /// The new scale of the widget.
        scale: f32,
        /// The size of the screen in GUI units.
        screen: [f32; 2],
    },
    /// The gear of a widget was clicked: the menu opens the settings of its mod.
    /// The menu handles it itself; it never reaches the client.
    OpenSettings {
        /// The id of the mod.
        id: String,
    },
    /// The selected widget goes back to its default place and size.
    Reset {
        /// The id of the mod.
        id: String,
    },
    /// Every widget goes back to its default place and size.
    ResetAll,
}
