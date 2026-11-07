// SPDX-License-Identifier: MIT OR Apache-2.0

//! The modern button and its hover animation.

use crate::context::{Ui, UiResponse};
use crate::draw::{Color, SpriteSheet};
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::text::FontAtlas;

use super::text::{draw_text, measure_text, Align};
use super::theme::{
    mix, with_opacity, DISABLED_OPACITY, HOVER_FADE_STEP, RADIUS, SPACING, SURFACE,
    SURFACE_DISABLED, SURFACE_HOVER, TEXT, TEXT_SIZE_BODY,
};

/// Share of the button height the icon of a labelled button takes.
const LABEL_ICON_SHARE: f32 = 0.5;
/// Share of the button edge the icon of an icon-only button takes.
const ICON_ONLY_SHARE: f32 = 0.6;

/// The hover animation of one button.
///
/// The screen keeps one per button, calls [`HoverFade::tick`] once per 20 TPS
/// tick with the `hovered` flag the button returned, and draws with
/// [`HoverFade::level`] so the fade is smooth between ticks.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HoverFade {
    previous: f32,
    current: f32,
}

impl HoverFade {
    /// Advances one tick towards hovered (`1.0`) or resting (`0.0`).
    pub fn tick(&mut self, hovered: bool) {
        self.previous = self.current;
        let step = if hovered {
            HOVER_FADE_STEP
        } else {
            -HOVER_FADE_STEP
        };
        self.current = (self.current + step).clamp(0.0, 1.0);
    }

    /// The fade in `0.0..=1.0`, `partial_tick` of the way to the next tick.
    #[must_use]
    pub fn level(self, partial_tick: f32) -> f32 {
        self.previous + (self.current - self.previous) * partial_tick.clamp(0.0, 1.0)
    }
}

/// A translucent rounded button with a centred brand label and an optional
/// icon to its left.
///
/// ```
/// use umicraft_ui::modern::{theme, HoverFade, ModernButton};
/// use umicraft_ui::{FontAtlas, Rect, Ui, UiInput};
///
/// let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
/// let mut ui = Ui::new(viewport);
/// ui.begin_frame(viewport, UiInput::default());
/// let id = ui.id("play");
/// let rect = Rect::new(100.0, 100.0, 300.0, theme::BUTTON_HEIGHT * 3.0);
/// let fade = HoverFade::default();
/// let response = ModernButton::new(id, rect, "Play").show(
///     &mut ui,
///     &FontAtlas::fallback(),
///     3.0,
///     fade.level(0.5),
/// );
/// assert!(!response.clicked);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ModernButton<'a> {
    id: WidgetId,
    rect: Rect,
    label: &'a str,
    icon: Option<SpriteSheet>,
    enabled: bool,
}

impl<'a> ModernButton<'a> {
    /// An enabled button without icon; `rect` is in physical pixels.
    #[must_use]
    pub fn new(id: WidgetId, rect: Rect, label: &'a str) -> Self {
        Self {
            id,
            rect,
            label,
            icon: None,
            enabled: true,
        }
    }

    /// Shows `icon` before the label; `None` leaves it out.
    #[must_use]
    pub fn with_icon(mut self, icon: Option<SpriteSheet>) -> Self {
        self.icon = icon;
        self
    }

    /// Sets whether the button reacts; a disabled one never reports hover or
    /// click and is drawn dimmed.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the button and reports its interaction state.
    ///
    /// `scale` is pixels per GUI unit, `brand` the brand atlas and `fade` the
    /// [`HoverFade::level`] of this button.
    pub fn show(self, ui: &mut Ui, brand: &FontAtlas, scale: f32, fade: f32) -> UiResponse {
        let response = respond(ui, self.id, self.rect, self.enabled);
        draw_surface(ui, self.rect, RADIUS * scale, fade, self.enabled);
        let size = TEXT_SIZE_BODY * scale;
        let text_width = measure_text(brand, self.label, size);
        let icon_size = self.rect.height * LABEL_ICON_SHARE;
        let icon_span = self.icon.map_or(0.0, |_| icon_size + SPACING * scale);
        let left = self.rect.x + (self.rect.width - icon_span - text_width) * 0.5;
        let middle = self.rect.y + self.rect.height * 0.5;
        if let Some(sheet) = self.icon {
            let dst = Rect::new(left, middle - icon_size * 0.5, icon_size, icon_size);
            draw_icon(ui, sheet, dst, self.enabled);
        }
        let color = with_opacity(TEXT, opacity(self.enabled));
        let at = (left + icon_span, middle);
        draw_text(ui, brand, self.label, at, (size, color, Align::Left));
        response
    }
}

/// A small icon-only button, square with rounded corners or fully round.
#[derive(Debug, Clone, Copy)]
pub struct IconButton {
    id: WidgetId,
    rect: Rect,
    icon: Option<SpriteSheet>,
    round: bool,
}

impl IconButton {
    /// An enabled square button; `rect` is in physical pixels.
    #[must_use]
    pub fn new(id: WidgetId, rect: Rect, icon: Option<SpriteSheet>) -> Self {
        Self {
            id,
            rect,
            icon,
            round: false,
        }
    }

    /// Makes the corners a half circle.
    #[must_use]
    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }

    /// Draws the button and reports its interaction state; `scale` and `fade`
    /// are as in [`ModernButton::show`].
    pub fn show(self, ui: &mut Ui, scale: f32, fade: f32) -> UiResponse {
        let response = respond(ui, self.id, self.rect, true);
        let edge = self.rect.width.min(self.rect.height);
        let radius = if self.round {
            edge * 0.5
        } else {
            RADIUS * scale
        };
        draw_surface(ui, self.rect, radius, fade, true);
        if let Some(sheet) = self.icon {
            let size = edge * ICON_ONLY_SHARE;
            let dst = Rect::new(
                self.rect.x + (self.rect.width - size) * 0.5,
                self.rect.y + (self.rect.height - size) * 0.5,
                size,
                size,
            );
            draw_icon(ui, sheet, dst, true);
        }
        response
    }
}

/// Registers the widget; a disabled one reports no hover and no click.
fn respond(ui: &mut Ui, id: WidgetId, rect: Rect, enabled: bool) -> UiResponse {
    let mut response = ui.response(id, rect);
    if !enabled {
        response.hovered = false;
        response.clicked = false;
    } else if response.clicked {
        ui.note_button_press();
    }
    response
}

/// Draws the rounded background, blended towards the hover colour by `fade`.
fn draw_surface(ui: &mut Ui, rect: Rect, radius: f32, fade: f32, enabled: bool) {
    let color = if enabled {
        mix(SURFACE, SURFACE_HOVER, fade)
    } else {
        SURFACE_DISABLED
    };
    ui.draw().rounded_rect(rect, radius, color);
}

/// Draws the whole of `sheet` into `dst`, dimmed when disabled.
fn draw_icon(ui: &mut Ui, sheet: SpriteSheet, dst: Rect, enabled: bool) {
    let src = sheet.sub(0, 0, sheet.width, sheet.height);
    let tint: Color = [1.0, 1.0, 1.0, opacity(enabled)];
    ui.draw().image(src, dst, tint);
}

fn opacity(enabled: bool) -> f32 {
    if enabled {
        1.0
    } else {
        DISABLED_OPACITY
    }
}
