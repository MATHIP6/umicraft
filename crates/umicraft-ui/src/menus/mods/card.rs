// SPDX-License-Identifier: MIT OR Apache-2.0

//! One mod as a card: its icon, its name, the button that opens its settings and
//! the on/off switch. A mod that is on is tinted and its icon is lit; the pointer
//! lifts the card.

use crate::context::Ui;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::modern::theme::{self, mix, with_opacity};
use crate::modern::{
    draw_icon, draw_text, fit_text, measure_text, toggle, Align, TOGGLE_HEIGHT, TOGGLE_WIDTH,
};

use super::motion::Motion;
use super::style::{hover_level, tile_fill, Paint, NAME_SIZE};
use super::view::ModCardView;

/// Space between the card edge and its content, in GUI units.
const PAD: f32 = 6.0;
/// Edge of the icon of the mod.
const ICON: f32 = 17.0;
/// Height of the settings button and the space between it and the switch.
const BUTTON_HEIGHT: f32 = 12.0;
const BUTTON_GAP: f32 = 4.0;
/// Edge of the small icon in the settings button, and its space to the label.
const BUTTON_ICON: f32 = 7.0;
const BUTTON_ICON_GAP: f32 = 3.0;
/// Size of the label of the settings button.
const BUTTON_TEXT: f32 = 5.2;

/// What the player did on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CardAction {
    /// The switch was clicked; the state asked for.
    Toggle(bool),
    /// The settings button was clicked.
    OpenSettings,
}

/// The icon of the mod called `id`; a mod without a picture gets the grid.
fn mod_icon(id: &str) -> Icon {
    match id {
        "fps" => Icon::Gauge,
        "coordinates" => Icon::Target,
        "cps" => Icon::Cursor,
        "ping" => Icon::Signal,
        "toggle_sprint" => Icon::Run,
        "motion_blur" => Icon::Blur,
        "fullbright" => Icon::Lightbulb,
        _ => Icon::Grid,
    }
}

/// The switch and the settings button of a card at `rect`, in physical pixels,
/// side by side along the bottom of the card.
pub(super) fn controls(rect: Rect, scale: f32) -> (Rect, Rect) {
    let left = rect.x + PAD * scale;
    let bottom = rect.bottom() - PAD * scale;
    let middle = bottom - BUTTON_HEIGHT * scale * 0.5;
    let switch = Rect::new(
        rect.right() - PAD * scale - TOGGLE_WIDTH * scale,
        middle - TOGGLE_HEIGHT * scale * 0.5,
        TOGGLE_WIDTH * scale,
        TOGGLE_HEIGHT * scale,
    );
    let options = Rect::new(
        left,
        bottom - BUTTON_HEIGHT * scale,
        switch.x - BUTTON_GAP * scale - left,
        BUTTON_HEIGHT * scale,
    );
    (switch, options)
}

/// Draws `card` in `rect` and reports what was clicked. `options` is the label
/// of the settings button.
pub(super) fn card(
    ui: &mut Ui,
    paint: Paint<'_>,
    rect: Rect,
    (card, motion): (&ModCardView, &mut Motion),
    options: &str,
) -> Option<CardAction> {
    let scale = paint.scale;
    let hover = motion.hover(&format!("hover/{}", card.id), ui.is_hovered(rect));
    let (on, speed) = motion.level(&format!("on/{}", card.id), card.enabled);
    let (switch, settings) = controls(rect, scale);
    let middle_x = rect.x + rect.width * 0.5;

    ui.draw()
        .rounded_rect(rect, theme::RADIUS * scale * 1.4, tile_fill(hover, on));
    let icon_colour = mix(with_opacity(theme::TEXT_MUTED, 0.7), theme::TEXT, on);
    draw_icon(
        ui,
        paint.brand,
        mod_icon(&card.id),
        (middle_x, rect.y + (PAD + ICON * 0.5) * scale),
        (ICON * scale, icon_colour),
    );
    let name = fit_text(
        paint.brand,
        &card.name,
        NAME_SIZE * scale,
        rect.width - 2.0 * PAD * scale,
    );
    let name_colour = mix(theme::TEXT_MUTED, theme::TEXT, on);
    let name_at = (middle_x, rect.y + (PAD + ICON + 7.0) * scale);
    paint.text(ui, &name, name_at, (NAME_SIZE, name_colour, Align::Center));

    ui.push_id(&card.id);
    let settings_id = ui.id("settings");
    let switch_id = ui.id("switch");
    let response = ui.response(settings_id, settings);
    if response.clicked {
        ui.note_button_press();
    }
    let lit = f32::from(u8::from(response.hovered));
    let button_fill = with_opacity(theme::TEXT, 0.07 + 0.07 * lit);
    ui.draw()
        .rounded_rect(settings, theme::RADIUS * scale, button_fill);
    // The icon and the label of the button are centred together.
    let size = BUTTON_TEXT * scale;
    let icon_size = BUTTON_ICON * scale;
    let label = fit_text(
        paint.brand,
        options,
        size,
        settings.width - (BUTTON_ICON + BUTTON_ICON_GAP + 6.0) * scale,
    );
    let text_width = measure_text(paint.brand, &label, size);
    let group = icon_size + BUTTON_ICON_GAP * scale + text_width;
    let left = settings.x + (settings.width - group) * 0.5;
    let centre_y = settings.y + settings.height * 0.5;
    let button_colour = mix(theme::TEXT_MUTED, theme::TEXT, lit);
    draw_icon(
        ui,
        paint.brand,
        Icon::Sliders,
        (left + icon_size * 0.5, centre_y),
        (icon_size, button_colour),
    );
    draw_text(
        ui,
        paint.brand,
        &label,
        (left + icon_size + BUTTON_ICON_GAP * scale, centre_y),
        (size, button_colour, Align::Left),
    );

    let look = (hover_level(ui, switch), on, speed);
    let flipped = toggle(ui, switch_id, switch, card.enabled, look);
    ui.pop_id();
    flipped
        .map(CardAction::Toggle)
        .or(response.clicked.then_some(CardAction::OpenSettings))
}
