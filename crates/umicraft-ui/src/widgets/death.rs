// SPDX-License-Identifier: MIT OR Apache-2.0

//! The death screen (MCP `GuiGameOver`).
//!
//! A dark red gradient covers the game, "You died!" is drawn at twice the
//! text size, the score follows, and a "Respawn" button leaves the screen.
//! The buttons only react 20 ticks after the death (`enableButtonsTimer`);
//! the caller passes that state in, so the widget keeps no clock.
//!
//! Positions are the vanilla ones in GUI pixels (`width / 2 - 100`,
//! `height / 4 + 72`, ...) scaled by [`gui_scale`].

use crate::context::Ui;
use crate::draw::{Color, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::gui_scale;
use super::style;

/// The ticks the buttons stay disabled after the death
/// (MCP `GuiGameOver.updateScreen`: `enableButtonsTimer == 20`).
pub const BUTTON_DELAY_TICKS: u32 = 20;

/// The top colour of the gradient, `0x60500000` (MCP `GuiGameOver.drawScreen`).
const GRADIENT_TOP: Color = [80.0 / 255.0, 0.0, 0.0, 96.0 / 255.0];
/// The bottom colour of the gradient, `0xA0803030`.
const GRADIENT_BOTTOM: Color = [128.0 / 255.0, 48.0 / 255.0, 48.0 / 255.0, 160.0 / 255.0];
/// The yellow of the score (`EnumChatFormatting.YELLOW`).
const YELLOW: Color = [1.0, 1.0, 85.0 / 255.0, 1.0];
/// The number of bands the gradient is drawn in.
const GRADIENT_BANDS: u32 = 48;

/// Button width and height in GUI pixels (MCP `GuiButton`).
const BUTTON_SIZE: (f32, f32) = (200.0, 20.0);

/// What the player did on the death screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathAction {
    /// Nothing happened.
    None,
    /// The player pressed "Respawn".
    Respawn,
}

/// The rectangles of the death screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeathLayout {
    /// The "Respawn" button.
    pub respawn: Rect,
    /// The "Title Screen" button.
    pub title_screen: Rect,
    /// The vertical position of the title text.
    pub title_y: f32,
    /// The vertical position of the score text.
    pub score_y: f32,
    /// Pixels per GUI pixel.
    pub scale: f32,
}

/// Computes the vanilla layout of the death screen for a viewport.
#[must_use]
pub fn death_layout(viewport: Rect) -> DeathLayout {
    let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
    let width = viewport.width / scale;
    let height = viewport.height / scale;
    let button = |y: f32| {
        Rect::new(
            viewport.x + (width / 2.0 - BUTTON_SIZE.0 / 2.0) * scale,
            viewport.y + y * scale,
            BUTTON_SIZE.0 * scale,
            BUTTON_SIZE.1 * scale,
        )
    };
    DeathLayout {
        respawn: button(height / 4.0 + 72.0),
        title_screen: button(height / 4.0 + 96.0),
        // The title is drawn in a 2x scaled space at y = 30.
        title_y: viewport.y + 60.0 * scale,
        score_y: viewport.y + 100.0 * scale,
        scale,
    }
}

/// Draws the death screen and reports what the player chose.
///
/// `buttons_enabled` is false during the first [`BUTTON_DELAY_TICKS`] ticks.
/// The "Title Screen" button is drawn but never enabled: the client has no
/// title screen yet (milestone P10).
pub fn death_screen(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    score: i32,
    buttons_enabled: bool,
) -> DeathAction {
    let layout = death_layout(viewport);
    draw_gradient(ui, viewport);

    let title_scale = 2.0 * layout.scale;
    centered_text(
        ui,
        atlas,
        viewport,
        layout.title_y,
        title_scale,
        &[("You died!", style::TEXT)],
    );
    let score_text = score.to_string();
    centered_text(
        ui,
        atlas,
        viewport,
        layout.score_y,
        layout.scale,
        &[("Score: ", style::TEXT), (&score_text, YELLOW)],
    );

    let respawn_id = ui.id("death.respawn");
    let clicked = draw_button(
        ui,
        atlas,
        buttons_enabled.then_some(respawn_id),
        layout.respawn,
        "Respawn",
        layout.scale,
    );
    draw_button(
        ui,
        atlas,
        None,
        layout.title_screen,
        "Title Screen",
        layout.scale,
    );

    if clicked {
        DeathAction::Respawn
    } else {
        DeathAction::None
    }
}

/// Draws the vertical gradient as horizontal bands.
fn draw_gradient(ui: &mut Ui, viewport: Rect) {
    let band = viewport.height / GRADIENT_BANDS as f32;
    for index in 0..GRADIENT_BANDS {
        let t = (index as f32 + 0.5) / GRADIENT_BANDS as f32;
        let color = std::array::from_fn(|channel| {
            GRADIENT_TOP[channel] + (GRADIENT_BOTTOM[channel] - GRADIENT_TOP[channel]) * t
        });
        ui.draw().rect(
            Rect::new(
                viewport.x,
                viewport.y + band * index as f32,
                viewport.width,
                band + 1.0,
            ),
            color,
        );
    }
}

/// Draws coloured runs as one line centred in the viewport.
fn centered_text(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    y: f32,
    scale: f32,
    runs: &[(&str, Color)],
) {
    let total: f32 = runs
        .iter()
        .map(|(text, _)| atlas.measure(text, scale))
        .sum();
    let mut x = viewport.x + (viewport.width - total) * 0.5;
    for (text, color) in runs {
        ui.draw().text([x, y], *text, TextStyle::new(scale, *color));
        x += atlas.measure(text, scale);
    }
}

/// Draws a button; `id` is `None` for a disabled one. True when clicked.
fn draw_button(
    ui: &mut Ui,
    atlas: &FontAtlas,
    id: Option<crate::id::WidgetId>,
    rect: Rect,
    label: &str,
    scale: f32,
) -> bool {
    let response = id.map(|id| ui.response(id, rect));
    let hovered = response.is_some_and(|response| response.hovered);
    let fill = match (id.is_some(), hovered) {
        (false, _) => style::BUTTON_DISABLED,
        (true, true) => style::BUTTON_HOVER,
        (true, false) => style::BUTTON,
    };
    ui.draw().rect(rect, fill);
    let width = atlas.measure(label, scale);
    let height = atlas.line_height(scale);
    let color = if id.is_some() {
        style::TEXT
    } else {
        style::TEXT_DIM
    };
    ui.push_clip(rect);
    ui.draw().text(
        [
            rect.x + (rect.width - width).max(0.0) * 0.5,
            rect.y + (rect.height - height).max(0.0) * 0.5,
        ],
        label,
        TextStyle::new(scale, color),
    );
    ui.pop_clip();
    response.is_some_and(|response| response.clicked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCommand;
    use crate::input::{InputEvent, PointerButton, UiInput};

    const VIEWPORT: Rect = Rect::new(0.0, 0.0, 1280.0, 720.0);

    fn click(position: [f32; 2]) -> UiInput {
        UiInput::new(vec![
            InputEvent::PointerMoved { position },
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            },
            InputEvent::PointerReleased {
                position,
                button: PointerButton::Left,
            },
        ])
    }

    fn run(input: UiInput, enabled: bool) -> (DeathAction, Vec<DrawCommand>) {
        let mut ui = Ui::new(VIEWPORT);
        ui.begin_frame(VIEWPORT, input);
        let action = death_screen(&mut ui, &FontAtlas::fallback(), VIEWPORT, 42, enabled);
        let output = ui.end_frame();
        (action, output.draw_list.commands().to_vec())
    }

    fn centre(rect: Rect) -> [f32; 2] {
        [rect.x + rect.width / 2.0, rect.y + rect.height / 2.0]
    }

    #[test]
    fn the_buttons_sit_where_vanilla_puts_them() {
        // 1280x720 gives GUI scale 3: 426.7 x 240 GUI pixels.
        let layout = death_layout(VIEWPORT);
        assert_eq!(layout.scale, 3.0);
        assert!((layout.respawn.y - (240.0 / 4.0 + 72.0) * 3.0).abs() < 1e-3);
        assert!((layout.title_screen.y - layout.respawn.y - 24.0 * 3.0).abs() < 1e-3);
        assert_eq!(layout.respawn.width, 600.0);
        assert_eq!(layout.respawn.height, 60.0);
        let middle = layout.respawn.x + layout.respawn.width / 2.0;
        assert!((middle - 640.0).abs() < 1e-3);
    }

    #[test]
    fn clicking_respawn_after_the_delay_respawns() {
        let layout = death_layout(VIEWPORT);
        let (action, _) = run(click(centre(layout.respawn)), true);
        assert_eq!(action, DeathAction::Respawn);
    }

    #[test]
    fn the_buttons_ignore_clicks_during_the_first_second() {
        let layout = death_layout(VIEWPORT);
        let (action, _) = run(click(centre(layout.respawn)), false);
        assert_eq!(action, DeathAction::None);
    }

    #[test]
    fn the_title_screen_button_does_nothing_yet() {
        let layout = death_layout(VIEWPORT);
        let (action, _) = run(click(centre(layout.title_screen)), true);
        assert_eq!(action, DeathAction::None);
    }

    #[test]
    fn a_click_elsewhere_does_nothing() {
        let (action, _) = run(click([10.0, 10.0]), true);
        assert_eq!(action, DeathAction::None);
    }

    #[test]
    fn the_screen_shows_the_title_the_score_and_both_buttons() {
        let (_, commands) = run(UiInput::default(), true);
        let texts: Vec<&str> = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for expected in ["You died!", "Score: ", "42", "Respawn", "Title Screen"] {
            assert!(texts.contains(&expected), "{expected} missing in {texts:?}");
        }
    }

    #[test]
    fn the_gradient_darkens_towards_the_bottom_and_covers_the_screen() {
        let (_, commands) = run(UiInput::default(), true);
        let bands: Vec<(Rect, Color)> = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Rect { rect, color } if rect.width == VIEWPORT.width => {
                    Some((*rect, *color))
                }
                _ => None,
            })
            .take(GRADIENT_BANDS as usize)
            .collect();
        assert_eq!(bands.len(), GRADIENT_BANDS as usize);
        assert!(bands[0].1[3] < bands[bands.len() - 1].1[3]);
        assert_eq!(bands[0].0.y, 0.0);
        let last = bands[bands.len() - 1].0;
        assert!(last.y + last.height >= VIEWPORT.height);
    }
}
