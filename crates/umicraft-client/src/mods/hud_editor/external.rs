// SPDX-License-Identifier: MIT OR Apache-2.0

//! The widgets of the external mods in the HUD editor (F3.4).
//!
//! They are listed and previewed next to the built-in ones, through the same
//! conversion ([`mod_widget_view`]), under the ids `ext:<mod>:<widget>`, so a drag,
//! a resize or a reset reaches the host through the one routing function of
//! [`external_menu`](crate::mods::external_menu).

use umicraft_modhost::ModHost;
use umicraft_ui::menus::{HudEditorView, HudEntryView, HudWidgetView};
use umicraft_ui::FontAtlas;

use super::placeholder;
use crate::mods::external_menu::{card_id, widget_id};
use crate::mods::hud::mod_widget_view;
use crate::mods::icons::ModIcons;

/// Adds the external widgets to `view`: a preview for each widget of an enabled mod
/// (its label while it has nothing to draw, like a built-in one) and an entry for
/// each widget of every mod, on or off, failed ones off.
///
/// Locking is not offered for an external widget, so its entry is never locked.
pub(crate) fn extend(
    view: &mut HudEditorView,
    host: &ModHost,
    font: &FontAtlas,
    display: (u32, u32),
    icons: &ModIcons<'_>,
) {
    host.visit_widgets(|drawn| {
        // A widget on the screen (API 1.1) is placed by its mod, not the player.
        if drawn.screen {
            return;
        }
        let empty;
        let content = if drawn.content.is_empty() {
            empty = placeholder(drawn.label);
            &empty
        } else {
            drawn.content
        };
        let preview = mod_widget_view((drawn.placement, content), font, display, icons);
        view.widgets.extend(preview.map(|widget| HudWidgetView {
            id: widget_id(drawn.mod_id, drawn.key),
            name: drawn.label.to_owned(),
            widget,
        }));
    });
    for mod_view in host.mods() {
        let enabled = mod_view.enabled && !mod_view.failed;
        view.entries.extend(
            mod_view
                .widgets()
                .filter(|widget| !widget.screen)
                .map(|widget| HudEntryView {
                    id: widget_id(mod_view.id, widget.key),
                    card: card_id(mod_view.id),
                    name: widget.label.to_owned(),
                    enabled,
                    locked: false,
                }),
        );
    }
}
