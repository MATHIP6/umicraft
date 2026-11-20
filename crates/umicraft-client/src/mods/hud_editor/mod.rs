// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bridge between the built-in mods registry and the movable HUD widgets of the mods menu (ADR-0065).
//!
//! `umicraft-ui` previews and moves widgets with plain view types and answers
//! with a [`HudEditorOutcome`]; this module builds the views from the
//! [`ModRegistry`] (the previews are the very widgets of the HUD, see
//! [`mod_widget_views`]) and applies the outcomes back to it. The placement
//! maths stays in `umicraft-builtin-mods`: this module only converts between
//! the rectangle types.

use umicraft_builtin_mods::hud::snap::snap_rect;
use umicraft_builtin_mods::{
    EffectInfo, HudContent, HudPlacement, HudPreset, HudRect, HudWidget, ModContext, ModRegistry,
    RegistryError, MAX_EFFECTS, MAX_SCALE, MIN_SCALE,
};
use umicraft_core::lang::Lang;
use umicraft_render::gui_scale;
use umicraft_ui::menus::{
    HudEditorOutcome, HudEditorView, HudEntryView, HudPresetKind, HudWidgetView,
};
use umicraft_ui::{FontAtlas, Rect};

use crate::mods::hud::{item_of, mod_widget_views};

pub(crate) mod external;
use crate::mods::icons::ModIcons;

/// The colour of the label a widget shows while its mod has nothing to say yet.
const PLACEHOLDER_COLOR: u32 = 0xFF_FF_FF;

/// The id of the potion effects mod, whose widget is empty without an effect.
const POTION_EFFECTS_ID: &str = "potion_effects";

/// The Speed II effect with 1:23 left that the potion effects widget shows in the
/// menu while the player has no effect, so there is something to place.
fn test_effect_context() -> ModContext {
    let mut effects = [None; MAX_EFFECTS];
    effects[0] = Some(EffectInfo {
        id: 1,
        amplifier: 1,
        ticks: 20 * 83,
    });
    ModContext {
        effects,
        ..ModContext::default()
    }
}

/// What an empty widget of the mod `id` shows in the menu instead of its name,
/// `None` for a mod that has no test content.
fn preview(registry: &ModRegistry, id: &str) -> Option<HudContent> {
    (id == POTION_EFFECTS_ID)
        .then(|| registry.preview(id, &test_effect_context()))
        .flatten()
        .filter(|content| !content.is_empty())
}

/// What an empty widget called `name` shows in the menu: its name, so it can still
/// be grabbed.
pub(crate) fn placeholder(name: &str) -> HudContent {
    let mut content = HudContent::default();
    content.line(PLACEHOLDER_COLOR).push_str(name);
    content
}

/// The widgets of the enabled HUD mods on a window of `display` pixels.
///
/// A mod that is on but has no line yet (the first frame, or a counter that
/// shows nothing) gets its name as a placeholder so it can still be grabbed; the
/// potion effects mod shows a test effect instead.
#[must_use]
pub fn build(
    registry: &ModRegistry,
    lang: &Lang,
    font: &FontAtlas,
    display: (u32, u32),
    icons: &ModIcons<'_>,
) -> HudEditorView {
    let names: Vec<(&str, String)> = registry
        .mods()
        .map(|view| {
            (
                view.meta.id.as_str(),
                lang.tr(view.meta.name_key).to_owned(),
            )
        })
        .collect();
    let name_of = |id: &str| {
        names
            .iter()
            .find(|(known, _)| *known == id)
            .map_or_else(String::new, |(_, name)| name.clone())
    };
    let placeholders: Vec<Option<HudContent>> = registry
        .hud_widgets()
        .map(|widget| {
            widget.content.is_empty().then(|| {
                preview(registry, widget.id.as_str())
                    .unwrap_or_else(|| placeholder(&name_of(widget.id.as_str())))
            })
        })
        .collect();
    let widgets = registry
        .hud_widgets()
        .zip(&placeholders)
        .map(|(widget, placeholder)| HudWidget {
            content: placeholder.as_ref().unwrap_or(widget.content),
            ..widget
        });
    let views = mod_widget_views(widgets.map(item_of), font, display, icons);
    let scale = gui_scale(display.0, display.1) as f32;
    HudEditorView {
        widgets: registry
            .hud_widgets()
            .zip(views)
            .map(|(widget, view)| HudWidgetView {
                id: widget.id.as_str().to_owned(),
                name: name_of(widget.id.as_str()),
                widget: view,
            })
            .collect(),
        screen: [display.0 as f32 / scale, display.1 as f32 / scale],
        scale_range: (MIN_SCALE, MAX_SCALE),
        // Every mod that has a widget, on or off: the list of the HUD editor.
        entries: registry
            .mods()
            .filter(|view| view.placement.is_some())
            .map(|view| HudEntryView {
                id: view.meta.id.as_str().to_owned(),
                card: view.meta.id.as_str().to_owned(),
                name: name_of(view.meta.id.as_str()),
                enabled: view.enabled,
                locked: view.locked,
            })
            .collect(),
        has_custom: registry.has_custom(),
        ..HudEditorView::default()
    }
}

/// The snapping of a dragged widget: the one of the mods crate, on the UI's
/// rectangle type.
#[must_use]
pub fn snap(rect: Rect, screen: [f32; 2]) -> Rect {
    let snapped = snap_rect(to_hud_rect(rect), (screen[0], screen[1]));
    Rect::new(snapped.x, snapped.y, snapped.w, snapped.h)
}

/// The placement of a widget dropped on `rect` at `scale` on a screen of
/// `screen` GUI units.
pub(crate) fn placement_of(rect: Rect, scale: f32, screen: [f32; 2]) -> HudPlacement {
    HudPlacement::from_rect(to_hud_rect(rect), (screen[0], screen[1]), scale)
}

fn to_hud_rect(rect: Rect) -> HudRect {
    HudRect {
        x: rect.x,
        y: rect.y,
        w: rect.width,
        h: rect.height,
    }
}

/// Applies what the player did on the editor to the registry. Returns whether
/// the saved state may have changed, so the caller writes it to disk.
///
/// # Errors
///
/// [`RegistryError`] if the outcome names a mod that does not exist.
pub fn apply(
    registry: &mut ModRegistry,
    outcome: &HudEditorOutcome,
) -> Result<bool, RegistryError> {
    match outcome {
        HudEditorOutcome::Placed {
            id,
            rect,
            scale,
            screen,
        }
        | HudEditorOutcome::Scaled {
            id,
            rect,
            scale,
            screen,
        } => {
            registry.set_placement(id, placement_of(*rect, *scale, *screen))?;
        }
        // The menu opens the settings page itself; there is nothing to apply.
        HudEditorOutcome::OpenSettings { .. } => return Ok(false),
        HudEditorOutcome::Reset { id } => registry.reset_placement(id)?,
        HudEditorOutcome::ResetAll => {
            let ids: Vec<String> = registry
                .mods()
                .filter(|view| view.placement.is_some() && !view.locked)
                .map(|view| view.meta.id.as_str().to_owned())
                .collect();
            for id in ids {
                registry.reset_placement(&id)?;
            }
        }
        HudEditorOutcome::Lock { id, locked } => registry.set_locked(id, *locked)?,
        HudEditorOutcome::Preset(kind) => registry.apply_preset(match kind {
            HudPresetKind::Default => HudPreset::Default,
            HudPresetKind::Pvp => HudPreset::Pvp,
            HudPresetKind::Minimal => HudPreset::Minimal,
            HudPresetKind::Custom => HudPreset::Custom,
        }),
        // The layout is saved after every change; saving it by hand also keeps
        // it as the "Custom" preset.
        HudEditorOutcome::Save => registry.save_custom(),
        // The history belongs to the runtime (`ModsRuntime::apply_change`).
        HudEditorOutcome::Undo | HudEditorOutcome::Redo | HudEditorOutcome::Cancel => {
            return Ok(false)
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
