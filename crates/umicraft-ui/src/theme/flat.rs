// SPDX-License-Identifier: MIT OR Apache-2.0

//! The flat skin: filled rectangles with an optional border and bevel.

use crate::context::Ui;
use crate::draw::{Color, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;

/// Height of the bevel under an enabled control, in texels.
const BEVEL_TEXELS: f32 = 2.0;
/// Width of the border, in texels.
const BORDER_TEXELS: f32 = 1.0;
/// Horizontal room kept free around a label, in physical pixels.
const LABEL_PAD: f32 = 6.0;

/// What a control looks like right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState {
    /// Enabled and not under the pointer.
    Idle,
    /// Enabled and under the pointer (or being dragged).
    Hovered,
    /// Not reacting to input.
    Disabled,
}

/// Colours and shape of a flat control. A texel is one GUI unit at the scale
/// the control is drawn at (`height / 20`, as vanilla skins its 20 unit tall
/// buttons).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatSkin {
    /// Button fill at rest.
    pub fill: Color,
    /// Button fill under the pointer.
    pub hover: Color,
    /// Button fill when disabled.
    pub disabled: Color,
    /// Slider groove.
    pub track: Color,
    /// Outline around the control, if any.
    pub border: Option<Color>,
    /// Darker strip along the bottom of an enabled control, if any.
    pub bevel: Option<Color>,
    /// Colour of the slider handle.
    pub accent: Color,
    /// Colour of the groove left of the handle, if different from `track`.
    pub slider_fill: Option<Color>,
    /// Label colour at rest.
    pub text: Color,
    /// Label colour under the pointer.
    pub text_hover: Color,
    /// Label colour when disabled.
    pub text_disabled: Color,
    /// Whether labels get the vanilla one-texel drop shadow.
    pub text_shadow: bool,
    /// Width of the slider handle in texels.
    pub handle_texels: f32,
}

impl FlatSkin {
    /// Paints a button and its label. `tint` replaces the label colour of an
    /// enabled button (a conflicting key is red, vanilla style).
    pub(crate) fn draw_button(
        &self,
        ui: &mut Ui,
        rect: Rect,
        label: (&FontAtlas, &str),
        state: ControlState,
        tint: Option<Color>,
    ) {
        let unit = texel(rect);
        let fill = match state {
            ControlState::Idle => self.fill,
            ControlState::Hovered => self.hover,
            ControlState::Disabled => self.disabled,
        };
        self.draw_surface(ui, rect, fill, unit, state != ControlState::Disabled);
        let color = match (state, tint) {
            (ControlState::Disabled, _) => self.text_disabled,
            (_, Some(tint)) => tint,
            (ControlState::Hovered, None) => self.text_hover,
            (ControlState::Idle, None) => self.text,
        };
        self.draw_label(ui, rect, label, color);
    }

    /// Paints a slider with its handle at `value` (`0.0..=1.0`) and its label.
    pub(crate) fn draw_slider(
        &self,
        ui: &mut Ui,
        rect: Rect,
        label: (&FontAtlas, &str),
        value: f32,
        hovered: bool,
    ) {
        let unit = texel(rect);
        self.draw_surface(ui, rect, self.track, unit, false);
        let handle = self.handle_texels * unit;
        let x = rect.x + value * (rect.width - handle).max(0.0);
        let inner = self.inner(rect, unit);
        if let Some(fill) = self.slider_fill {
            let reached = Rect::new(inner.x, inner.y, (x - inner.x).max(0.0), inner.height);
            ui.draw().rect(reached, fill);
        }
        let color = if hovered { self.text_hover } else { self.text };
        let thumb = Rect::new(x, rect.y, handle, rect.height);
        ui.draw().rect(thumb, self.accent);
        self.draw_label(ui, rect, label, color);
    }

    /// The rectangle inside the border.
    fn inner(&self, rect: Rect, unit: f32) -> Rect {
        let border = if self.border.is_some() {
            BORDER_TEXELS * unit
        } else {
            0.0
        };
        rect.inset(border)
    }

    fn draw_surface(&self, ui: &mut Ui, rect: Rect, fill: Color, unit: f32, bevel: bool) {
        if let Some(border) = self.border {
            ui.draw().rect(rect, border);
        }
        let inner = self.inner(rect, unit);
        ui.draw().rect(inner, fill);
        if let (true, Some(shade)) = (bevel, self.bevel) {
            let height = (BEVEL_TEXELS * unit).min(inner.height);
            let strip = Rect::new(inner.x, inner.bottom() - height, inner.width, height);
            ui.draw().rect(strip, shade);
        }
    }

    /// Draws `label` centred in `rect`, shrunk to fit, with the shadow if the
    /// skin has one.
    fn draw_label(&self, ui: &mut Ui, rect: Rect, (atlas, text): (&FontAtlas, &str), color: Color) {
        if text.is_empty() || rect.is_empty() {
            return;
        }
        let available = (rect.width - LABEL_PAD).max(0.0);
        let mut scale = texel(rect);
        let mut width = atlas.measure(text, scale);
        if width > available && available > 0.0 {
            scale = (scale * available / width).max(1.0);
            width = atlas.measure(text, scale);
        }
        let position = [
            (rect.x + (rect.width - width).max(0.0) * 0.5).round(),
            (rect.y + (rect.height - 8.0 * scale).max(0.0) * 0.5).round(),
        ];
        ui.push_clip(rect);
        if self.text_shadow {
            let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]];
            let offset = [position[0] + scale, position[1] + scale];
            ui.draw().text(offset, text, TextStyle::new(scale, shadow));
        }
        ui.draw().text(position, text, TextStyle::new(scale, color));
        ui.pop_clip();
    }
}

/// The texel size of a control: one GUI unit at the scale it is drawn at.
#[must_use]
pub(crate) fn texel(rect: Rect) -> f32 {
    (rect.height / 20.0).round().max(1.0)
}
