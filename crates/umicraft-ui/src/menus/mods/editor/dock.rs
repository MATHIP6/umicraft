// SPDX-License-Identifier: MIT OR Apache-2.0

//! The dock of the HUD editor: its header, the two tabs, the list of widgets
//! and the actions at the bottom.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::modern::theme::{self, mix, with_opacity};
use crate::modern::{self, fit_text, search_field, toggle, Align, TOGGLE_HEIGHT, TOGGLE_WIDTH};
use crate::text::FontAtlas;

use super::super::button::icon_button;
use super::super::hud_layer::{HudEditorOutcome, HudEntryView, HudPresetKind};
use super::super::style::{hover_level, Paint, NAME_SIZE, TILE_ON};
use super::super::view::{ModsOutcome, ModsView};
use super::super::{ModsScreen, BLINK_TICKS};
use super::{draw_panel, DockLayout, ACTION, GAP, TAB_SETTINGS, TAB_WIDGETS};

/// The fill under the panel of the dock.
const DOCK_BASE: Color = [0.05, 0.055, 0.07, 0.86];
/// Size of the text of a chip, in GUI units.
const CHIP_TEXT: f32 = 6.0;
/// Height of the search box of the list.
const SEARCH: f32 = 13.0;
/// Height of one row of the list and the space between two.
const ROW: f32 = 15.0;
const ROW_GAP: f32 = 2.0;

/// A small button of the dock: `active` is lit with the accent colour (a
/// switch that is on), a disabled one is dimmed and never answers. Returns
/// whether it was clicked.
pub(in crate::menus::mods) fn chip(
    ui: &mut Ui,
    paint: Paint<'_>,
    rect: Rect,
    (key, label): (&str, &str),
    (enabled, active): (bool, bool),
) -> bool {
    let id = ui.id(key);
    let response = ui.response(id, rect);
    let hover = if enabled { hover_level(ui, rect) } else { 0.0 };
    let dim = if enabled {
        1.0
    } else {
        theme::DISABLED_OPACITY
    };
    let fill = if active {
        with_opacity(theme::ACCENT, 0.34 + 0.14 * hover)
    } else {
        mix(theme::SURFACE, theme::SURFACE_HOVER, hover)
    };
    ui.draw()
        .rounded_rect(rect, theme::RADIUS * paint.scale, with_opacity(fill, dim));
    let color = if active {
        theme::TEXT
    } else {
        mix(theme::TEXT_MUTED, theme::TEXT, hover)
    };
    let room = rect.width - 4.0 * paint.scale;
    let text = fit_text(paint.brand, label, CHIP_TEXT * paint.scale, room);
    let at = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
    let style = (CHIP_TEXT, with_opacity(color, dim), Align::Center);
    paint.text(ui, &text, at, style);
    let clicked = response.clicked && enabled;
    if clicked {
        ui.note_button_press();
    }
    clicked
}

/// `count` rectangles side by side filling `row`, `GAP` apart.
pub(in crate::menus::mods) fn split(row: Rect, count: usize, scale: f32) -> Vec<Rect> {
    let gap = GAP * scale;
    let width = (row.width - gap * (count.max(1) - 1) as f32) / count.max(1) as f32;
    (0..count)
        .map(|index| {
            Rect::new(
                row.x + index as f32 * (width + gap),
                row.y,
                width,
                row.height,
            )
        })
        .collect()
}

/// Row number `index` of the actions in `footer`.
pub(in crate::menus::mods) fn action_row(footer: Rect, index: usize, scale: f32) -> Rect {
    Rect::new(
        footer.x,
        footer.y + index as f32 * (ACTION + GAP) * scale,
        footer.width,
        ACTION * scale,
    )
}

/// A small padlock centred on `at` (physical pixels), `size` high: the mark of
/// a locked widget. Drawn from rectangles, the icon set has none.
pub(super) fn draw_lock(ui: &mut Ui, at: (f32, f32), size: f32, color: Color) {
    let body = Rect::new(
        at.0 - size * 0.4,
        at.1 - size * 0.05,
        size * 0.8,
        size * 0.55,
    );
    ui.draw().rounded_rect(body, size * 0.1, color);
    let bar = size * 0.12;
    let (left, right, top) = (at.0 - size * 0.25, at.0 + size * 0.25 - bar, at.1 - size * 0.5);
    for rect in [
        Rect::new(left, top, bar, size * 0.5),
        Rect::new(right, top, bar, size * 0.5),
        Rect::new(left, top, size * 0.5, bar),
    ] {
        ui.draw().rect(rect, color);
    }
}

/// The rectangle of row `index` of the widget list in `body`.
pub(in crate::menus::mods) fn list_row(body: Rect, index: usize, scale: f32) -> Rect {
    Rect::new(
        body.x,
        body.y + (SEARCH + GAP + index as f32 * (ROW + ROW_GAP)) * scale,
        body.width,
        ROW * scale,
    )
}

/// The switch at the right end of a row of the dock.
pub(in crate::menus::mods) fn row_toggle(row: Rect, scale: f32) -> Rect {
    Rect::new(
        row.right() - (TOGGLE_WIDTH + 3.0) * scale,
        row.y + (row.height - TOGGLE_HEIGHT * scale) * 0.5,
        TOGGLE_WIDTH * scale,
        TOGGLE_HEIGHT * scale,
    )
}

impl ModsScreen {
    /// Draws the dock and returns what the player asked for.
    pub(super) fn dock(
        &mut self,
        ui: &mut Ui,
        (paint, font): (Paint<'_>, &FontAtlas),
        lang: &Lang,
        view: &ModsView,
        layout: &DockLayout,
    ) -> Option<ModsOutcome> {
        let scale = paint.scale;
        // Opaque enough that a widget under the dock does not show through it.
        ui.draw()
            .rounded_rect(layout.dock, theme::RADIUS * scale * 1.5, DOCK_BASE);
        draw_panel(ui, layout.dock, scale);
        // A hairline between the tab, which scrolls, and the actions.
        let rule = Rect::new(
            layout.footer.x,
            layout.footer.y - GAP * scale * 0.5 - scale.max(1.0) * 0.5,
            layout.footer.width,
            scale.max(1.0) * 0.5,
        );
        ui.draw().rect(rule, [1.0, 1.0, 1.0, 0.08]);

        let middle = layout.header.y + layout.header.height * 0.5;
        let title = (NAME_SIZE * 1.2, theme::TEXT, Align::Left);
        let title_at = (layout.header.x, middle);
        paint.text(ui, lang.tr("umicraft.mods.editor.title"), title_at, title);
        let close_id = ui.id("close");
        let fade = hover_level(ui, layout.close);
        let close = icon_button(
            ui,
            paint.brand,
            (close_id, layout.close),
            (Icon::Close, scale, fade),
        );

        let labels = [
            lang.tr("umicraft.mods.editor.tab.widgets"),
            lang.tr("umicraft.mods.editor.tab.settings"),
        ];
        let tab = self.editor.tab.min(TAB_SETTINGS);
        let line = self.motion.spring("editor_tabs", tab as f32).0;
        if let Some(clicked) =
            modern::tabs(ui, paint.brand, layout.tabs, &labels, (tab, line), scale)
        {
            self.editor.tab = clicked;
            self.editor.scroll = 0.0;
        }

        let body = if self.editor.tab == TAB_WIDGETS {
            self.widget_list(ui, paint, lang, view, layout)
        } else {
            self.inspector(ui, (paint, font), lang, view, layout.body)
        };
        let action = self.actions(ui, paint, lang, view, layout);
        if close.clicked {
            self.close_editor();
            return None;
        }
        body.or(action)
    }

    /// The search box and one row per mod that has a widget: its name, a
    /// padlock while it is locked and the switch of the mod. A click on the
    /// row selects the widget and opens its settings.
    fn widget_list(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        view: &ModsView,
        layout: &DockLayout,
    ) -> Option<ModsOutcome> {
        let scale = paint.scale;
        let body = layout.body;
        let search = Rect::new(body.x, body.y, body.width, SEARCH * scale);
        let blink_on = (self.blink / BLINK_TICKS).is_multiple_of(2);
        let placeholder = lang.tr("umicraft.mods.editor.search");
        let field = (&mut self.editor.query, placeholder);
        search_field(ui, paint.brand, search, field, (scale, blink_on));

        let query = self.editor.query.trim().to_lowercase();
        let entries: Vec<&HudEntryView> = view
            .hud
            .entries
            .iter()
            .filter(|entry| query.is_empty() || entry.name.to_lowercase().contains(&query))
            .collect();
        if entries.is_empty() {
            let at = (body.x + body.width * 0.5, list_row(body, 0, scale).y + 8.0 * scale);
            let style = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Center);
            paint.text(ui, lang.tr("umicraft.mods.editor.no_match"), at, style);
            return None;
        }

        let mut outcome = None;
        ui.push_clip(body);
        for (index, entry) in entries.into_iter().enumerate() {
            let row = list_row(body, index, scale);
            if row.bottom() > body.bottom() {
                break;
            }
            ui.push_id(&entry.id);
            let selected = self.hud.selected() == Some(entry.id.as_str());
            let track = row_toggle(row, scale);
            // The row answers left of its switch, so one click does one thing.
            let name_area = Rect::new(row.x, row.y, track.x - row.x - 2.0 * scale, row.height);
            let hover = self.motion.hover(&format!("editor/row/{}", entry.id), ui.is_hovered(row));
            let fill = if selected {
                with_opacity(TILE_ON, 2.2)
            } else {
                [1.0, 1.0, 1.0, 0.05 * hover]
            };
            ui.draw().rounded_rect(row, theme::RADIUS * scale, fill);
            if selected {
                let bar = Rect::new(row.x, row.y + 3.0 * scale, 1.5 * scale, row.height - 6.0 * scale);
                ui.draw().rounded_rect(bar, 0.75 * scale, theme::ACCENT);
            }
            let hidden = self.editor.hidden.contains(&entry.id);
            let color = if entry.enabled && !hidden {
                theme::TEXT
            } else {
                theme::TEXT_MUTED
            };
            let middle = row.y + row.height * 0.5;
            let name_x = row.x + 6.0 * scale;
            let lock_room = if entry.locked { 9.0 * scale } else { 0.0 };
            let name = fit_text(
                paint.brand,
                &entry.name,
                theme::TEXT_SIZE_BODY * scale,
                name_area.right() - name_x - lock_room,
            );
            paint.text(ui, &name, (name_x, middle), (theme::TEXT_SIZE_BODY, color, Align::Left));
            if entry.locked {
                let at = (name_area.right() - 4.0 * scale, middle);
                draw_lock(ui, at, 6.0 * scale, theme::TEXT_MUTED);
            }
            let row_id = ui.id("row");
            if ui.response(row_id, name_area).clicked {
                ui.note_button_press();
                self.hud.select(Some(entry.id.clone()));
                self.show_settings_tab();
            }
            let toggle_id = ui.id("enabled");
            let key = format!("editor/on/{}", entry.id);
            let (progress, speed) = self.motion.level(&key, entry.enabled);
            let look = (hover_level(ui, track), progress, speed);
            if let Some(enabled) = toggle(ui, toggle_id, track, entry.enabled, look) {
                outcome = Some(ModsOutcome::Toggle {
                    id: entry.id.clone(),
                    enabled,
                });
            }
            ui.pop_id();
        }
        ui.pop_clip();
        outcome
    }

    /// The actions at the bottom of the dock: undo, redo, the grid and the dim
    /// switches, the four presets, then reset, cancel and save.
    fn actions(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        view: &ModsView,
        layout: &DockLayout,
    ) -> Option<ModsOutcome> {
        let scale = paint.scale;
        let hud = |outcome| Some(ModsOutcome::Hud(outcome));
        let mut asked = None;
        let label = |key: &'static str| (key, lang.tr(key));
        let (can_undo, can_redo) = view.hud.history;

        let tools = split(action_row(layout.footer, 0, scale), 4, scale);
        if chip(ui, paint, tools[0], label("umicraft.mods.editor.undo"), (can_undo, false)) {
            asked = hud(HudEditorOutcome::Undo);
        }
        if chip(ui, paint, tools[1], label("umicraft.mods.editor.redo"), (can_redo, false)) {
            asked = hud(HudEditorOutcome::Redo);
        }
        let grid = (true, self.editor.grid);
        if chip(ui, paint, tools[2], label("umicraft.mods.editor.grid"), grid) {
            self.editor.grid = !self.editor.grid;
        }
        let dim = (true, self.editor.dim);
        if chip(ui, paint, tools[3], label("umicraft.mods.editor.dim"), dim) {
            self.editor.dim = !self.editor.dim;
        }

        let presets = [
            ("umicraft.mods.editor.preset.default", HudPresetKind::Default, true),
            ("umicraft.mods.editor.preset.pvp", HudPresetKind::Pvp, true),
            ("umicraft.mods.editor.preset.minimal", HudPresetKind::Minimal, true),
            (
                "umicraft.mods.editor.preset.custom",
                HudPresetKind::Custom,
                view.hud.has_custom,
            ),
        ];
        let cells = split(action_row(layout.footer, 1, scale), 2, scale)
            .into_iter()
            .chain(split(action_row(layout.footer, 2, scale), 2, scale));
        for (rect, (key, kind, enabled)) in cells.zip(presets) {
            if chip(ui, paint, rect, label(key), (enabled, false)) {
                asked = hud(HudEditorOutcome::Preset(kind));
            }
        }

        let last = split(action_row(layout.footer, 3, scale), 3, scale);
        let any = !view.hud.entries.is_empty();
        if chip(ui, paint, last[0], label("umicraft.mods.editor.reset_all"), (any, false)) {
            self.editor.confirm = true;
        }
        if chip(ui, paint, last[1], label("umicraft.mods.editor.cancel"), (true, false)) {
            asked = hud(HudEditorOutcome::Cancel);
            self.close_editor();
        }
        if chip(ui, paint, last[2], label("umicraft.mods.editor.save"), (true, true)) {
            asked = hud(HudEditorOutcome::Save);
            self.close_editor();
        }
        asked
    }
}
