// SPDX-License-Identifier: MIT OR Apache-2.0

//! One mod as a card: name, description, category, the on/off switch and the
//! button that opens its settings.

use crate::context::Ui;
use crate::geom::Rect;
use crate::modern::theme::{self, with_opacity};
use crate::modern::{fit_text, toggle, Align, TOGGLE_HEIGHT, TOGGLE_WIDTH};

use super::style::{card_fill, hover_level, Paint};
use super::view::ModCardView;

/// Space between the card edge and its content, in GUI units.
const PAD: f32 = 6.0;
/// Edge of the settings button.
const GEAR_SIZE: f32 = 11.0;

/// What the player did on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CardAction {
    /// The switch was clicked; the state asked for.
    Toggle(bool),
    /// The settings button was clicked.
    OpenSettings,
}

/// The switch and the settings button of a card at `rect`, in physical pixels.
pub(super) fn controls(rect: Rect, scale: f32) -> (Rect, Rect) {
    let right = rect.right() - PAD * scale;
    let switch = Rect::new(
        right - TOGGLE_WIDTH * scale,
        rect.y + 5.0 * scale,
        TOGGLE_WIDTH * scale,
        TOGGLE_HEIGHT * scale,
    );
    let gear = Rect::new(
        right - GEAR_SIZE * scale,
        rect.bottom() - (PAD - 1.0 + GEAR_SIZE - 3.0) * scale,
        GEAR_SIZE * scale,
        GEAR_SIZE * scale,
    );
    (switch, gear)
}

/// Draws `card` in `rect` and reports what was clicked. `category` is the
/// translated name of its category.
pub(super) fn card(
    ui: &mut Ui,
    paint: Paint<'_>,
    rect: Rect,
    (card, category): (&ModCardView, &str),
) -> Option<CardAction> {
    let scale = paint.scale;
    let lift = hover_level(ui, rect) * 0.5;
    ui.draw()
        .rounded_rect(rect, theme::RADIUS * scale, card_fill(lift));
    let (switch, gear) = controls(rect, scale);
    let left = rect.x + PAD * scale;
    let name_width = switch.x - left - PAD * scale;
    let name = fit_text(
        paint.brand,
        &card.name,
        theme::TEXT_SIZE_BODY * scale,
        name_width,
    );
    paint.text(
        ui,
        &name,
        (left, rect.y + 9.0 * scale),
        (theme::TEXT_SIZE_BODY, theme::TEXT, Align::Left),
    );
    let wide = gear.x - left - PAD * scale * 0.5;
    let desc = fit_text(
        paint.brand,
        &card.description,
        theme::TEXT_SIZE_SMALL * scale,
        wide,
    );
    let muted = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Left);
    paint.text(ui, &desc, (left, rect.y + 19.0 * scale), muted);
    let tag = (
        theme::TEXT_SIZE_SMALL,
        with_opacity(theme::ACCENT, 0.8),
        Align::Left,
    );
    paint.text(ui, category, (left, rect.y + 27.5 * scale), tag);

    ui.push_id(&card.id);
    let switch_id = ui.id("switch");
    let gear_id = ui.id("settings");
    let switch_hover = hover_level(ui, switch);
    let flipped = toggle(ui, switch_id, switch, card.enabled, switch_hover);
    let opened = gear_button(ui, gear_id, gear, scale);
    ui.pop_id();
    flipped
        .map(CardAction::Toggle)
        .or(opened.then_some(CardAction::OpenSettings))
}

/// Draws the settings button, a square with three sliders' lines, and reports
/// a click. Drawn from shapes so it needs no glyph of the font.
fn gear_button(ui: &mut Ui, id: crate::id::WidgetId, rect: Rect, scale: f32) -> bool {
    let response = ui.response(id, rect);
    ui.draw().rounded_rect(
        rect,
        theme::RADIUS * scale,
        card_fill(f32::from(u8::from(response.hovered))),
    );
    let line = Rect::new(
        rect.x + rect.width * 0.2,
        0.0,
        rect.width * 0.6,
        scale.max(1.0),
    );
    for (row, knob) in [(0.28, 0.65), (0.5, 0.3), (0.72, 0.55)] {
        let y = rect.y + rect.height * row;
        ui.draw()
            .rect(Rect { y, ..line }, with_opacity(theme::TEXT, 0.8));
        let dot = scale.max(1.0) * 2.0;
        let dot_rect = Rect::new(
            line.x + line.width * knob - dot * 0.5,
            y - dot * 0.5 + line.height * 0.5,
            dot,
            dot,
        );
        ui.draw().rounded_rect(dot_rect, dot * 0.5, theme::TEXT);
    }
    if response.clicked {
        ui.note_button_press();
    }
    response.clicked
}
