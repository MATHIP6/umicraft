// SPDX-License-Identifier: MIT OR Apache-2.0

//! The settings page of one mod: a row per setting, generated from its
//! [`SettingView`]s. Every control reports the new value; the page changes
//! nothing itself.

use crate::context::Ui;
use crate::geom::Rect;
use crate::modern::theme;
use crate::modern::{modern_slider, toggle, Align, ModernButton, SliderDrag, SliderRange};
use crate::modern::{TOGGLE_HEIGHT, TOGGLE_WIDTH};

use super::color::{parse_hex, same_color, PRESETS};
use super::style::{card_fill, hover_level, Paint};
use super::view::{ModValue, SettingControl, SettingView};

/// Height of one row, in GUI units.
pub(super) const ROW_HEIGHT: f32 = 20.0;
/// Space between two rows, in GUI units.
const ROW_GAP: f32 = 2.0;
/// Space between the row edge and its content.
const PAD: f32 = 5.0;
/// Width of the slider track.
const SLIDER_WIDTH: f32 = 90.0;
/// Width reserved for the value shown next to a slider.
const VALUE_WIDTH: f32 = 22.0;
/// Width of the button that cycles through the options of a choice.
const CHOICE_WIDTH: f32 = 84.0;
/// Edge of a color swatch and the space between two.
const SWATCH: f32 = 9.0;
const SWATCH_GAP: f32 = 2.0;

/// The digits after the point needed to show multiples of `step`, at most 3.
fn decimals(step: f32) -> usize {
    (0..3)
        .find(|&digits| {
            let scaled = step * 10f32.powi(digits);
            (scaled - scaled.round()).abs() < 1e-3
        })
        .map_or(3, |digits| digits as usize)
}

/// A slider value as text, with the digits its step needs.
pub(super) fn format_value(value: f32, step: f32) -> String {
    format!("{value:.*}", decimals(step))
}

/// Height of `count` rows with the gaps between them, in GUI units.
pub(super) fn content_height(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        count as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP
    }
}

/// The rectangle of row `index` in `area`.
pub(super) fn row_rect(area: Rect, scale: f32, index: usize) -> Rect {
    let height = ROW_HEIGHT * scale;
    Rect::new(
        area.x,
        area.y + index as f32 * (height + ROW_GAP * scale),
        area.width,
        height,
    )
}

/// A rectangle `width` x `height` GUI units, ending at the right padding of
/// `row` and centred on it vertically, at `shift` GUI units left of that end.
fn at_right(row: Rect, scale: f32, (width, height): (f32, f32), shift: f32) -> Rect {
    Rect::new(
        row.right() - (PAD + shift + width) * scale,
        row.y + (row.height - height * scale) * 0.5,
        width * scale,
        height * scale,
    )
}

/// The switch of a toggle row.
pub(super) fn toggle_track(row: Rect, scale: f32) -> Rect {
    at_right(row, scale, (TOGGLE_WIDTH, TOGGLE_HEIGHT), 0.0)
}

/// The track of a slider row, left of its value.
pub(super) fn slider_track(row: Rect, scale: f32) -> Rect {
    at_right(row, scale, (SLIDER_WIDTH, 10.0), VALUE_WIDTH)
}

/// The button of a choice row.
pub(super) fn choice_button(row: Rect, scale: f32) -> Rect {
    at_right(row, scale, (CHOICE_WIDTH, 12.0), 0.0)
}

/// Swatch number `index` of a color row.
pub(super) fn swatch_rect(row: Rect, scale: f32, index: usize) -> Rect {
    let from_end = (PRESETS.len() - 1 - index) as f32 * (SWATCH + SWATCH_GAP);
    at_right(row, scale, (SWATCH, SWATCH), from_end)
}

/// Draws the rows of `settings` in `area`, scrolled by `offset` GUI units, and
/// reports the first control the player changed, as `(setting key, new value)`.
pub(super) fn settings_panel(
    ui: &mut Ui,
    paint: Paint<'_>,
    area: Rect,
    settings: &[SettingView],
    (drag, offset): (&mut SliderDrag, f32),
) -> Option<(String, ModValue)> {
    let mut changed = None;
    let top = Rect {
        y: area.y - offset * paint.scale,
        ..area
    };
    ui.push_clip(area);
    for (index, setting) in settings.iter().enumerate() {
        let row = row_rect(top, paint.scale, index);
        if row.bottom() < area.y || row.y > area.bottom() {
            continue;
        }
        ui.draw()
            .rounded_rect(row, theme::RADIUS * paint.scale, card_fill(0.0));
        ui.push_id(&setting.key);
        let value = control(ui, paint, row, setting, drag);
        ui.pop_id();
        if let Some(value) = value {
            changed = Some((setting.key.clone(), value));
        }
    }
    ui.pop_clip();
    changed
}

/// Draws the label and the control of one row.
fn control(
    ui: &mut Ui,
    paint: Paint<'_>,
    row: Rect,
    setting: &SettingView,
    drag: &mut SliderDrag,
) -> Option<ModValue> {
    let scale = paint.scale;
    let left = row.x + PAD * scale;
    let middle = row.y + row.height * 0.5;
    let label = (theme::TEXT_SIZE_BODY, theme::TEXT, Align::Left);
    let id = ui.id("control");
    match &setting.control {
        SettingControl::Toggle(on) => {
            paint.text(ui, &setting.label, (left, middle), label);
            let track = toggle_track(row, scale);
            let hover = hover_level(ui, track);
            toggle(ui, id, track, *on, hover).map(ModValue::Bool)
        }
        SettingControl::Slider {
            min,
            max,
            step,
            value,
        } => {
            paint.text(ui, &setting.label, (left, middle), label);
            let shown = format_value(*value, *step);
            let end = (row.right() - PAD * scale, middle);
            let small = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Right);
            paint.text(ui, &shown, end, small);
            let range = SliderRange {
                min: *min,
                max: *max,
                step: *step,
            };
            let track = slider_track(row, scale);
            modern_slider(ui, id, track, range, *value, drag).map(ModValue::Number)
        }
        SettingControl::Choice { options, selected } => {
            paint.text(ui, &setting.label, (left, middle), label);
            let button = choice_button(row, scale);
            let current = options.get(*selected).map_or("", |(_, text)| text.as_str());
            let fade = hover_level(ui, button);
            let clicked = ModernButton::new(id, button, current)
                .show(ui, paint.brand, scale, fade)
                .clicked;
            let next = options.get((*selected + 1) % options.len().max(1))?;
            clicked.then(|| ModValue::Text(next.0.clone()))
        }
        SettingControl::Color(hex) => {
            paint.text(ui, &setting.label, (left, middle - 3.5 * scale), label);
            let small = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Left);
            paint.text(ui, hex, (left, middle + 5.5 * scale), small);
            swatches(ui, paint, row, hex)
        }
    }
}

/// Draws the palette of a color row and returns the preset clicked.
fn swatches(ui: &mut Ui, paint: Paint<'_>, row: Rect, current: &str) -> Option<ModValue> {
    let scale = paint.scale;
    let mut picked = None;
    for (index, preset) in PRESETS.iter().enumerate() {
        let rect = swatch_rect(row, scale, index);
        let id = ui.id(preset);
        let response = ui.response(id, rect);
        if same_color(preset, current) {
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
        if response.clicked {
            ui.note_button_press();
            picked = Some(ModValue::Text((*preset).to_owned()));
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_shows_the_digits_of_its_step() {
        assert_eq!(format_value(2.0, 1.0), "2");
        assert_eq!(format_value(1.5, 0.5), "1.5");
        assert_eq!(format_value(0.25, 0.25), "0.25");
        assert_eq!(format_value(0.1234, 0.01), "0.12");
    }
}
