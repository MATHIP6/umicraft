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
    HudContent, HudPlacement, HudRect, HudWidget, ModRegistry, RegistryError, MAX_SCALE, MIN_SCALE,
};
use umicraft_core::lang::Lang;
use umicraft_render::gui_scale;
use umicraft_ui::menus::{HudEditorOutcome, HudEditorView, HudWidgetView};
use umicraft_ui::{FontAtlas, Rect};

use crate::mods_hud_view::mod_widget_views;

/// The colour of the label a widget shows while its mod has nothing to say yet.
const PLACEHOLDER_COLOR: u32 = 0xFF_FF_FF;

/// The widgets of the enabled HUD mods on a window of `display` pixels.
///
/// A mod that is on but has no line yet (the first frame, or a counter that
/// shows nothing) gets its name as a placeholder so it can still be grabbed.
#[must_use]
pub fn build(
    registry: &ModRegistry,
    lang: &Lang,
    font: &FontAtlas,
    display: (u32, u32),
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
            widget.content.lines().is_empty().then(|| {
                let mut content = HudContent::default();
                content
                    .line(PLACEHOLDER_COLOR)
                    .push_str(&name_of(widget.id.as_str()));
                content
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
    let views = mod_widget_views(widgets, font, display);
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
    }
}

/// The snapping of a dragged widget: the one of the mods crate, on the UI's
/// rectangle type.
#[must_use]
pub fn snap(rect: Rect, screen: [f32; 2]) -> Rect {
    let snapped = snap_rect(to_hud_rect(rect), (screen[0], screen[1]));
    Rect::new(snapped.x, snapped.y, snapped.w, snapped.h)
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
        } => {
            let placement =
                HudPlacement::from_rect(to_hud_rect(*rect), (screen[0], screen[1]), *scale);
            registry.set_placement(id, placement)?;
        }
        HudEditorOutcome::Reset { id } => registry.reset_placement(id)?,
        HudEditorOutcome::ResetAll => {
            let ids: Vec<String> = registry
                .mods()
                .filter(|view| view.placement.is_some())
                .map(|view| view.meta.id.as_str().to_owned())
                .collect();
            for id in ids {
                registry.reset_placement(&id)?;
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
