// SPDX-License-Identifier: MIT OR Apache-2.0

//! The settings tab of the dock: the place, the size, the look and the
//! behaviour of the selected widget.
//!
//! The look is whatever its mod declares as options, nothing more: a widget
//! shows no control it has no use for. The rows scroll with the wheel.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::input::Key;
use crate::menus::common::key_pressed;
use crate::modern::theme;
use crate::modern::{fit_text, modern_slider, toggle, Align, SliderRange};
use crate::text::FontAtlas;

use super::super::color::{parse_hex, same_color, PRESETS};
use super::super::hud_layer::{clamp_into, rescaled, widget_rect, HudEditorOutcome};
use super::super::list::wheel_scrolled;
use super::super::settings_panel::format_value;
use super::super::style::{hover_level, Paint};
use super::super::view::{ModValue, ModsOutcome, ModsView, SettingControl, SettingView};
use super::super::ModsScreen;
use super::dock::{chip, row_toggle, split};
use super::GAP;

/// Height of a row with one line, and of one with a label over its control.
const LINE: f32 = 13.0;
const TALL: f32 = 22.0;
/// Space between two rows, and above the title of a section.
const ROW_GAP: f32 = 2.0;
const SECTION_GAP: f32 = 5.0;
/// Width of the `-` and `+` of a stepper and of the button of a choice.
const STEP_BUTTON: f32 = 13.0;
const CHOICE_WIDTH: f32 = 56.0;
/// Edge of a colour swatch and the space between two.
const SWATCH: f32 = 9.0;
const SWATCH_GAP: f32 = 3.0;
/// What one click of a stepper or one arrow key moves a widget by, in GUI
/// units, and with Shift held.
const NUDGE: f32 = 1.0;
const NUDGE_FAST: f32 = 8.0;
/// The step of the scale slider.
const SCALE_STEP: f32 = 0.05;

/// The rows of the tab, laid out from the top of `area` down.
struct Rows<'a> {
    paint: Paint<'a>,
    area: Rect,
    /// The top of the next row, in physical pixels.
    y: f32,
}

impl Rows<'_> {
    /// The next row, `height` GUI units high, and whether it is whole inside
    /// the area: a row cut by an edge is drawn but does not answer, so a click
    /// on the actions under the tab never reaches a control scrolled below.
    fn next(&mut self, height: f32) -> (Rect, bool) {
        let scale = self.paint.scale;
        let row = Rect::new(self.area.x, self.y, self.area.width, height * scale);
        self.y += (height + ROW_GAP) * scale;
        let live = row.y >= self.area.y - 0.5 && row.bottom() <= self.area.bottom() + 0.5;
        (row, live)
    }

    /// The title of a section, small and muted.
    fn section(&mut self, ui: &mut Ui, title: &str) {
        self.y += SECTION_GAP * self.paint.scale;
        let (row, _) = self.next(8.0);
        let style = (theme::TEXT_SIZE_SMALL, theme::ACCENT, Align::Left);
        let at = (row.x, row.y + row.height * 0.5);
        self.paint.text(ui, &title.to_uppercase(), at, style);
    }

    /// The label of a row, at its left, on the line whose middle is `middle`.
    fn label(&self, ui: &mut Ui, text: &str, row: Rect, (middle, room): (f32, f32)) {
        let size = theme::TEXT_SIZE_SMALL + 0.5;
        let text = fit_text(self.paint.brand, text, size * self.paint.scale, room);
        let style = (size, theme::TEXT, Align::Left);
        self.paint.text(ui, &text, (row.x + 2.0 * self.paint.scale, middle), style);
    }

    /// A row with a switch; returns the state asked for.
    fn switch(
        &mut self,
        ui: &mut Ui,
        (key, label): (&str, &str),
        (on, look): (bool, (f32, f32)),
    ) -> Option<bool> {
        let (row, live) = self.next(LINE);
        let track = row_toggle(row, self.paint.scale);
        let middle = row.y + row.height * 0.5;
        self.label(ui, label, row, (middle, track.x - row.x - 4.0 * self.paint.scale));
        let id = ui.id(key);
        let hover = hover_level(ui, track);
        toggle(ui, id, track, on, (hover, look.0, look.1)).filter(|_| live)
    }

    /// A row with a value between a `-` and a `+`; returns `-1.0` or `1.0`.
    fn stepper(&mut self, ui: &mut Ui, (key, label): (&str, &str), value: &str, on: bool) -> Option<f32> {
        let scale = self.paint.scale;
        let (row, live) = self.next(LINE);
        let middle = row.y + row.height * 0.5;
        let button = |index: f32| {
            Rect::new(
                row.right() - (STEP_BUTTON * (2.0 - index) + 30.0 * (1.0 - index)) * scale,
                row.y + scale,
                STEP_BUTTON * scale,
                row.height - 2.0 * scale,
            )
        };
        let (minus, plus) = (button(0.0), button(1.0));
        self.label(ui, label, row, (middle, minus.x - row.x - 4.0 * scale));
        let style = (theme::TEXT_SIZE_SMALL + 0.5, theme::TEXT_MUTED, Align::Center);
        let at = ((minus.right() + plus.x) * 0.5, middle);
        self.paint.text(ui, value, at, style);
        ui.push_id(key);
        let down = chip(ui, self.paint, minus, ("minus", "-"), (on, false));
        let up = chip(ui, self.paint, plus, ("plus", "+"), (on, false));
        ui.pop_id();
        match (down, up) {
            _ if !live => None,
            (true, _) => Some(-1.0),
            (_, true) => Some(1.0),
            _ => None,
        }
    }

    /// A label over a row of choices, the `current` one lit; returns the one
    /// clicked.
    fn choices(
        &mut self,
        ui: &mut Ui,
        (key, label): (&str, &str),
        options: &[&str],
        (current, on): (Option<usize>, bool),
    ) -> Option<usize> {
        let scale = self.paint.scale;
        let (row, live) = self.next(TALL);
        self.label(ui, label, row, (row.y + 4.5 * scale, row.width));
        let cells = Rect::new(row.x, row.y + 9.5 * scale, row.width, 11.0 * scale);
        let mut clicked = None;
        ui.push_id(key);
        for (index, (rect, option)) in split(cells, options.len(), scale)
            .into_iter()
            .zip(options)
            .enumerate()
        {
            let state = (on, current == Some(index));
            let key = index.to_string();
            if chip(ui, self.paint, rect, (key.as_str(), *option), state) {
                clicked = Some(index);
            }
        }
        ui.pop_id();
        clicked.filter(|_| live)
    }
}

/// Which of the three places of an axis a widget is at: its start on `0`, its
/// middle on the middle or its end on `total`.
fn aligned(pos: f32, size: f32, total: f32) -> Option<usize> {
    [0.0, (total - size) * 0.5, total - size]
        .iter()
        .position(|place| (place - pos).abs() < 0.5)
}

impl ModsScreen {
    /// Draws the settings of the selected widget in `area` and returns the
    /// change the player made.
    pub(super) fn inspector(
        &mut self,
        ui: &mut Ui,
        (paint, font): (Paint<'_>, &FontAtlas),
        lang: &Lang,
        view: &ModsView,
        area: Rect,
    ) -> Option<ModsOutcome> {
        let scale = paint.scale;
        let entry = self
            .hud
            .selected()
            .and_then(|id| view.hud.entries.iter().find(|entry| entry.id == id));
        let Some(entry) = entry else {
            let at = (area.x + area.width * 0.5, area.y + 12.0 * scale);
            let style = (theme::TEXT_SIZE_SMALL + 0.5, theme::TEXT_MUTED, Align::Center);
            paint.text(ui, lang.tr("umicraft.mods.editor.none"), at, style);
            return None;
        };
        let id = entry.id.clone();
        let widget = view.hud.widgets.iter().find(|w| w.id == id);
        let card = view.cards.iter().find(|card| card.id == id);
        let screen = view.hud.screen;
        let movable = !entry.locked;
        let hud = |outcome| Some(ModsOutcome::Hud(outcome));
        let mut changed = None;

        ui.push_clip(area);
        ui.push_id("inspect");
        let start = area.y - self.editor.scroll * scale;
        let mut rows = Rows {
            paint,
            area,
            y: start,
        };

        let (title, _) = rows.next(10.0);
        let name_style = (theme::TEXT_SIZE_BODY, theme::TEXT, Align::Left);
        paint.text(ui, &entry.name, (title.x, title.y + title.height * 0.5), name_style);

        if let Some(widget) = widget {
            let rect = widget_rect(font, &widget.widget);
            let placed = |rect: Rect| HudEditorOutcome::Placed {
                id: id.clone(),
                rect: clamp_into(rect, screen),
                scale: widget.widget.scale,
                screen,
            };
            let moved = |dx: f32, dy: f32| {
                placed(Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height))
            };

            rows.section(ui, lang.tr("umicraft.mods.editor.position"));
            let step = if ui.input().modifiers.shift {
                NUDGE_FAST
            } else {
                NUDGE
            };
            let x = format!("{:.0}", rect.x);
            if let Some(way) = rows.stepper(ui, ("x", "X"), &x, movable) {
                changed = hud(moved(way * step, 0.0));
            }
            let y = format!("{:.0}", rect.y);
            if let Some(way) = rows.stepper(ui, ("y", "Y"), &y, movable) {
                changed = hud(moved(0.0, way * step));
            }
            let across = ["left", "center", "right"].map(|k| lang.tr(&align_key(k)).to_owned());
            let across: Vec<&str> = across.iter().map(String::as_str).collect();
            let now = aligned(rect.x, rect.width, screen[0]);
            let label = lang.tr("umicraft.mods.editor.horizontal");
            if let Some(index) = rows.choices(ui, ("h", label), &across, (now, movable)) {
                let x = [0.0, (screen[0] - rect.width) * 0.5, screen[0] - rect.width][index];
                changed = hud(moved(x - rect.x, 0.0));
            }
            let down = ["top", "middle", "bottom"].map(|k| lang.tr(&align_key(k)).to_owned());
            let down: Vec<&str> = down.iter().map(String::as_str).collect();
            let now = aligned(rect.y, rect.height, screen[1]);
            let label = lang.tr("umicraft.mods.editor.vertical");
            if let Some(index) = rows.choices(ui, ("v", label), &down, (now, movable)) {
                let y = [0.0, (screen[1] - rect.height) * 0.5, screen[1] - rect.height][index];
                changed = hud(moved(0.0, y - rect.y));
            }
            // The arrow keys move the widget too, for the last pixel.
            if movable {
                let arrows = [
                    (Key::ArrowLeft, (-step, 0.0)),
                    (Key::ArrowRight, (step, 0.0)),
                    (Key::ArrowUp, (0.0, -step)),
                    (Key::ArrowDown, (0.0, step)),
                ];
                for (key, (dx, dy)) in arrows {
                    if key_pressed(ui, key) {
                        changed = hud(moved(dx, dy));
                    }
                }
            }

            rows.section(ui, lang.tr("umicraft.mods.editor.size"));
            let (row, live) = rows.next(TALL);
            let (min, max) = view.hud.scale_range;
            let now = widget.widget.scale;
            rows.label(ui, lang.tr("umicraft.mods.editor.scale"), row, (row.y + 4.5 * scale, row.width * 0.6));
            let value_style = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Right);
            let shown = format!("x{now:.2}");
            paint.text(ui, &shown, (row.right() - 2.0 * scale, row.y + 4.5 * scale), value_style);
            let track = Rect::new(row.x + 2.0 * scale, row.y + 11.0 * scale, row.width - 4.0 * scale, 9.0 * scale);
            let range = SliderRange {
                min,
                max,
                step: SCALE_STEP,
            };
            let slider_id = ui.id("scale");
            let picked = modern_slider(ui, slider_id, track, range, now, &mut self.editor.drag);
            if let Some(next) = picked.filter(|_| live && movable) {
                changed = hud(HudEditorOutcome::Scaled {
                    id: id.clone(),
                    rect: rescaled(rect, (now, next), screen),
                    scale: next,
                    screen,
                });
            }
            let (row, _) = rows.next(LINE);
            let middle = row.y + row.height * 0.5;
            rows.label(ui, lang.tr("umicraft.mods.editor.dimensions"), row, (middle, row.width * 0.6));
            let size = format!("{:.0} x {:.0}", rect.width, rect.height);
            paint.text(ui, &size, (row.right() - 2.0 * scale, middle), value_style);
        }

        if let Some(card) = card.filter(|card| !card.settings.is_empty()) {
            rows.section(ui, lang.tr("umicraft.mods.editor.appearance"));
            for setting in &card.settings {
                ui.push_id(&setting.key);
                let value = self.setting_row(ui, &mut rows, (&id, setting));
                ui.pop_id();
                if let Some(value) = value {
                    changed = Some(ModsOutcome::Set {
                        id: id.clone(),
                        key: setting.key.clone(),
                        value,
                    });
                }
            }
        }

        rows.section(ui, lang.tr("umicraft.mods.editor.behavior"));
        let look = self.motion.level(&format!("editor/enabled/{id}"), entry.enabled);
        let label = lang.tr("umicraft.mods.editor.enabled");
        if let Some(enabled) = rows.switch(ui, ("enabled", label), (entry.enabled, look)) {
            changed = Some(ModsOutcome::Toggle {
                id: id.clone(),
                enabled,
            });
        }
        let look = self.motion.level(&format!("editor/locked/{id}"), entry.locked);
        let label = lang.tr("umicraft.mods.editor.locked");
        if let Some(locked) = rows.switch(ui, ("locked", label), (entry.locked, look)) {
            changed = hud(HudEditorOutcome::Lock {
                id: id.clone(),
                locked,
            });
        }
        let hidden = self.editor.hidden.contains(&id);
        let look = self.motion.level(&format!("editor/hidden/{id}"), hidden);
        let label = lang.tr("umicraft.mods.editor.hidden");
        if let Some(hide) = rows.switch(ui, ("hidden", label), (hidden, look)) {
            self.editor.hidden.retain(|other| *other != id);
            if hide {
                self.editor.hidden.push(id.clone());
            }
        }
        let (row, live) = rows.next(LINE);
        let reset = ("reset", lang.tr("umicraft.mods.editor.reset"));
        if chip(ui, paint, row, reset, (movable, false)) && live {
            changed = hud(HudEditorOutcome::Reset { id: id.clone() });
        }

        let height = (rows.y - start) / scale + GAP;
        ui.pop_id();
        ui.pop_clip();
        self.editor.scroll = wheel_scrolled(ui, area, height, self.editor.scroll);
        changed
    }

    /// One option of the mod of the widget, in the width of the dock: a switch
    /// on its line, or a label over a slider, a choice or a palette.
    fn setting_row(
        &mut self,
        ui: &mut Ui,
        rows: &mut Rows<'_>,
        (mod_id, setting): (&str, &SettingView),
    ) -> Option<ModValue> {
        let paint = rows.paint;
        let scale = paint.scale;
        match &setting.control {
            SettingControl::Toggle(on) => {
                let key = format!("editor/set/{mod_id}/{}", setting.key);
                let look = self.motion.level(&key, *on);
                rows.switch(ui, ("control", setting.label.as_str()), (*on, look))
                    .map(ModValue::Bool)
            }
            SettingControl::Slider {
                min,
                max,
                step,
                value,
            } => {
                let (row, live) = rows.next(TALL);
                let top = row.y + 4.5 * scale;
                rows.label(ui, &setting.label, row, (top, row.width * 0.75));
                let style = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Right);
                let shown = format_value(*value, *step);
                paint.text(ui, &shown, (row.right() - 2.0 * scale, top), style);
                let track = Rect::new(
                    row.x + 2.0 * scale,
                    row.y + 11.0 * scale,
                    row.width - 4.0 * scale,
                    9.0 * scale,
                );
                let range = SliderRange {
                    min: *min,
                    max: *max,
                    step: *step,
                };
                let id = ui.id("control");
                modern_slider(ui, id, track, range, *value, &mut self.editor.drag)
                    .filter(|_| live)
                    .map(ModValue::Number)
            }
            SettingControl::Choice { options, selected } => {
                let (row, live) = rows.next(LINE);
                let button = Rect::new(
                    row.right() - CHOICE_WIDTH * scale,
                    row.y + scale,
                    CHOICE_WIDTH * scale,
                    row.height - 2.0 * scale,
                );
                let middle = row.y + row.height * 0.5;
                rows.label(ui, &setting.label, row, (middle, button.x - row.x - 4.0 * scale));
                let current = options.get(*selected).map_or("", |(_, text)| text.as_str());
                let clicked = chip(ui, paint, button, ("control", current), (true, false));
                let next = options.get((*selected + 1) % options.len().max(1))?;
                (clicked && live).then(|| ModValue::Text(next.0.clone()))
            }
            SettingControl::Color(hex) => {
                let (row, live) = rows.next(TALL);
                rows.label(ui, &setting.label, row, (row.y + 4.5 * scale, row.width));
                let mut picked = None;
                for (index, preset) in PRESETS.iter().enumerate() {
                    let rect = Rect::new(
                        row.x + (2.0 + index as f32 * (SWATCH + SWATCH_GAP)) * scale,
                        row.y + 11.0 * scale,
                        SWATCH * scale,
                        SWATCH * scale,
                    );
                    let id = ui.id(preset);
                    let response = ui.response(id, rect);
                    if same_color(preset, hex) {
                        let ring = Rect::new(
                            rect.x - scale,
                            rect.y - scale,
                            rect.width + 2.0 * scale,
                            rect.height + 2.0 * scale,
                        );
                        ui.draw().rounded_rect(ring, 2.0 * scale, theme::TEXT);
                    }
                    let fill = parse_hex(preset).unwrap_or(theme::TEXT);
                    ui.draw().rounded_rect(rect, 1.5 * scale, fill);
                    if response.clicked && live {
                        ui.note_button_press();
                        picked = Some(ModValue::Text((*preset).to_owned()));
                    }
                }
                picked
            }
        }
    }
}

/// The language key of an alignment.
fn align_key(place: &str) -> String {
    format!("umicraft.mods.editor.align.{place}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_widget_knows_which_place_of_an_axis_it_is_at() {
        assert_eq!(aligned(0.0, 40.0, 400.0), Some(0));
        assert_eq!(aligned(180.0, 40.0, 400.0), Some(1));
        assert_eq!(aligned(360.0, 40.0, 400.0), Some(2));
        assert_eq!(aligned(57.0, 40.0, 400.0), None);
    }
}
