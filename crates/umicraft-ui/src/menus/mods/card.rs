// SPDX-License-Identifier: MIT OR Apache-2.0

//! One mod as a card: an icon tile and the on/off switch on top, its name and
//! a two-line description, then its category tag and the gear that opens its
//! settings. A mod that is on is tinted with the accent and its tile is lit;
//! the pointer lifts the card.

use crate::context::Ui;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::menus::common::wrap_lines;
use crate::modern::theme::{self, mix, with_opacity, BRAND_PIXEL_SIZE};
use crate::modern::{
    draw_icon, draw_text, fit_text, measure_text, toggle, Align, TOGGLE_HEIGHT, TOGGLE_WIDTH,
};

use super::button::icon_button;
use super::motion::Motion;
use super::style::{
    bordered, card_colors, hover_level, Paint, DESC_SIZE, LABEL_SIZE, MOD_NAME_SIZE, SURFACE_2,
};
use super::view::ModCardView;

/// Space between the card edge and its content, in GUI units.
const PAD: f32 = 8.0;
/// Edge of the icon tile of a card.
const TILE: f32 = 20.0;
/// Edge of the gear button.
const GEAR: f32 = 16.0;
/// Height of the category tag and its space to the text.
const TAG_HEIGHT: f32 = 11.0;
const TAG_PAD: f32 = 4.0;
/// Space between the tile and the name, and between the name and the text.
const TILE_GAP: f32 = 6.0;
const NAME_GAP: f32 = 2.0;
/// How far the description lines are apart, as a share of their size.
const LINE_HEIGHT: f32 = 1.45;
/// The lines of a description.
const DESC_LINES: usize = 2;

/// What the player did on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CardAction {
    /// The switch was clicked; the state asked for.
    Toggle(bool),
    /// The settings button was clicked.
    OpenSettings,
}

/// The icon of the mod called `id`; a mod without a picture gets the grid.
pub(super) fn mod_icon(id: &str) -> Icon {
    match id {
        "fps" => Icon::Gauge,
        "coordinates" => Icon::Target,
        "cps" => Icon::Cursor,
        "ping" => Icon::Signal,
        "toggle_sprint" => Icon::Run,
        "motion_blur" => Icon::Blur,
        "fullbright" => Icon::Lightbulb,
        "zoom" => Icon::Binoculars,
        "no_hurt_cam" => Icon::Camera,
        "keystrokes" => Icon::Keyboard,
        "combo_counter" => Icon::Lightning,
        "armor_status" => Icon::Shield,
        "potion_effects" => Icon::Flask,
        "legacy_animations" => Icon::Sword,
        "direction_hud" => Icon::Compass,
        "capes" => Icon::Cape,
        _ => Icon::Grid,
    }
}

/// The switch and the settings button of a card at `rect`, in physical pixels:
/// the switch at the top right, level with the tile, the gear at the bottom
/// right.
pub(super) fn controls(rect: Rect, scale: f32) -> (Rect, Rect) {
    let switch = Rect::new(
        rect.right() - (PAD + TOGGLE_WIDTH) * scale,
        rect.y + (PAD + (TILE - TOGGLE_HEIGHT) * 0.5) * scale,
        TOGGLE_WIDTH * scale,
        TOGGLE_HEIGHT * scale,
    );
    let gear = Rect::new(
        rect.right() - (PAD + GEAR) * scale,
        rect.bottom() - (PAD + GEAR) * scale,
        GEAR * scale,
        GEAR * scale,
    );
    (switch, gear)
}

/// Draws the square tile of a mod in `rect`: filled with the accent and the
/// icon white while the mod is on (`on` is the level, `0.0` to `1.0`), a faint
/// square and a muted icon while it is off.
pub(super) fn icon_tile(ui: &mut Ui, paint: Paint<'_>, rect: Rect, (id, on): (&str, f32)) {
    let radius = rect.width * 0.27;
    let fill = mix(SURFACE_2, theme::ACCENT, on);
    ui.draw().rounded_rect(rect, radius, fill);
    let icon_colour = mix(with_opacity(theme::TEXT, 0.85), [1.0, 1.0, 1.0, 1.0], on);
    let centre = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
    draw_icon(
        ui,
        paint.brand,
        mod_icon(id),
        centre,
        (rect.width * 0.5, icon_colour),
    );
}

/// Draws `card` in `rect` and reports what was clicked. `tag` is the name of
/// the category of the mod.
pub(super) fn card(
    ui: &mut Ui,
    paint: Paint<'_>,
    rect: Rect,
    (card, motion): (&ModCardView, &mut Motion),
    tag: &str,
) -> Option<CardAction> {
    let scale = paint.scale;
    let hover = motion.hover(&format!("hover/{}", card.id), ui.is_hovered(rect));
    let (on, speed) = motion.level(&format!("on/{}", card.id), card.enabled);
    let (switch, gear) = controls(rect, scale);
    let left = rect.x + PAD * scale;
    let inner_width = rect.width - 2.0 * PAD * scale;

    bordered(ui, rect, 7.0 * scale, card_colors(hover, on));
    let tile = Rect::new(left, rect.y + PAD * scale, TILE * scale, TILE * scale);
    icon_tile(ui, paint, tile, (&card.id, on));

    let name_y = tile.bottom() + (TILE_GAP + MOD_NAME_SIZE * 0.5) * scale;
    let name = fit_text(paint.brand, &card.name, MOD_NAME_SIZE * scale, inner_width);
    paint.text(
        ui,
        &name,
        (left, name_y),
        (MOD_NAME_SIZE, theme::TEXT, Align::Left),
    );
    let desc_top = name_y + (MOD_NAME_SIZE * 0.5 + NAME_GAP) * scale;
    draw_description(ui, paint, &card.description, (left, desc_top, inner_width));

    let tag_y = rect.bottom() - (PAD + GEAR * 0.5) * scale;
    draw_tag(ui, paint, tag, (left, tag_y, gear.x - left - 4.0 * scale));

    ui.push_id(&card.id);
    let settings_id = ui.id("settings");
    let switch_id = ui.id("switch");
    let gear_fade = hover_level(ui, gear);
    ui.draw().rounded_rect(gear, 4.5 * scale, SURFACE_2);
    let settings = icon_button(
        ui,
        paint.brand,
        (settings_id, gear),
        (Icon::Gear, scale, gear_fade),
    );
    let look = (hover_level(ui, switch), on, speed);
    let flipped = toggle(ui, switch_id, switch, card.enabled, look);
    ui.pop_id();
    flipped
        .map(CardAction::Toggle)
        .or(settings.clicked.then_some(CardAction::OpenSettings))
}

/// Draws up to [`DESC_LINES`] lines of `text` from `(x, top)`, `width` wide,
/// the last one cut with an ellipsis when the text is longer.
fn draw_description(ui: &mut Ui, paint: Paint<'_>, text: &str, (x, top, width): (f32, f32, f32)) {
    let size = DESC_SIZE * paint.scale;
    let mut lines = wrap_lines(paint.brand, text, width, size / BRAND_PIXEL_SIZE);
    if lines.len() > DESC_LINES {
        let rest = lines.split_off(DESC_LINES - 1).join(" ");
        lines.push(fit_text(paint.brand, &rest, size, width));
    }
    let step = size * LINE_HEIGHT;
    for (index, line) in lines.iter().enumerate() {
        let y = top + (index as f32 + 0.5) * step;
        let style = (DESC_SIZE, theme::TEXT_MUTED, Align::Left);
        paint.text(ui, line, (x, y), style);
    }
}

/// Draws the category tag, a small pill, with its left edge at `x` and its
/// middle at `y`, in at most `room` pixels.
fn draw_tag(ui: &mut Ui, paint: Paint<'_>, text: &str, (x, y, room): (f32, f32, f32)) {
    let scale = paint.scale;
    let size = LABEL_SIZE * scale;
    let label = fit_text(paint.brand, text, size, room - 2.0 * TAG_PAD * scale);
    let width = measure_text(paint.brand, &label, size) + 2.0 * TAG_PAD * scale;
    let height = TAG_HEIGHT * scale;
    let pill = Rect::new(x, y - height * 0.5, width, height);
    ui.draw().rounded_rect(pill, 3.0 * scale, SURFACE_2);
    let colour = with_opacity(theme::TEXT, 0.7);
    draw_text(
        ui,
        paint.brand,
        &label,
        (x + TAG_PAD * scale, y),
        (size, colour, Align::Left),
    );
}
