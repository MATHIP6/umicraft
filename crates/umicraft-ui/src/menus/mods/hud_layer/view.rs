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
    /// Every mod that has a widget, on or off, for the list of the HUD editor.
    pub entries: Vec<HudEntryView>,
    /// Whether there is a change to undo, and one to redo.
    pub history: (bool, bool),
    /// Whether a layout was saved, so the "Custom" preset has something to apply.
    pub has_custom: bool,
}

/// One mod that has a HUD widget, as the list of the HUD editor shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudEntryView {
    /// The id of the mod.
    pub id: String,
    /// The translated name of the mod.
    pub name: String,
    /// Whether the mod is on, so its widget is drawn.
    pub enabled: bool,
    /// Whether the widget is locked in place.
    pub locked: bool,
}

/// A ready-made layout of the HUD editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudPresetKind {
    /// Every widget where its mod puts it.
    Default,
    /// The widgets of a fight, out of the way.
    Pvp,
    /// The FPS counter alone.
    Minimal,
    /// The layout the player last saved.
    Custom,
}

impl HudEditorView {
    /// Whether the widget of the mod `id` is locked in place.
    #[must_use]
    pub fn is_locked(&self, id: &str) -> bool {
        self.entries.iter().any(|entry| entry.id == id && entry.locked)
    }
}

impl Default for HudEditorView {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            screen: [0.0; 2],
            scale_range: (1.0, 1.0),
            entries: Vec::new(),
            history: (false, false),
            has_custom: false,
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
    /// A widget was resized from the panel of the HUD editor: like
    /// [`Placed`](Self::Placed), but the frames of one slider drag are one
    /// change to undo.
    Scaled {
        /// The id of the mod.
        id: String,
        /// The rectangle of the widget.
        rect: Rect,
        /// The new scale of the widget.
        scale: f32,
        /// The size of the screen in GUI units.
        screen: [f32; 2],
    },
    /// A widget was locked in place, or unlocked.
    Lock {
        /// The id of the mod.
        id: String,
        /// The state asked for.
        locked: bool,
    },
    /// A ready-made layout was chosen.
    Preset(HudPresetKind),
    /// The last change is taken back.
    Undo,
    /// The change last taken back is made again.
    Redo,
    /// The layout is kept, and becomes the "Custom" preset.
    Save,
    /// Everything changed since the menu opened is taken back.
    Cancel,
}
