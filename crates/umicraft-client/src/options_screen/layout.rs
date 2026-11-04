// SPDX-License-Identifier: MIT OR Apache-2.0

//! The geometry and drawing helpers shared by the options pages: GUI-unit
//! constants, the [`Gui`] scale and the list chrome of `GuiSlot`.

use umicraft_ui::widgets::gui_scale;
use umicraft_ui::FontAtlas;
use umicraft_ui::{Color, InputEvent, Rect, TextStyle, Ui};

use super::OptionsScreen;

/// Height of a vanilla button, in GUI units.
pub(super) const BUTTON_H: f32 = 20.0;
/// Width of a half-width button, in GUI units.
pub(super) const HALF_W: f32 = 150.0;
/// Height of one row of the video list, in GUI units.
pub(super) const VIDEO_ROW: f32 = 25.0;
/// Height of one row of the key list, in GUI units.
pub(super) const KEY_ROW: f32 = 20.0;
/// `EnumChatFormatting.RED`.
pub(super) const RED: Color = [1.0, 0.333, 0.333, 1.0];
/// `EnumChatFormatting.YELLOW`.
pub(super) const YELLOW: Color = [1.0, 1.0, 0.333, 1.0];
/// The shadow of white text (`0xFFFFFF >> 2 & 0xFCFCFC`).
pub(super) const SHADOW: Color = [0.247, 0.247, 0.247, 1.0];
/// The tint of the dirt background behind a list (`0x202020`).
pub(super) const LIST_BACKGROUND: Color = [0.125, 0.125, 0.125, 1.0];
/// The tint of the bars above and below a list (`0x404040`).
pub(super) const LIST_BAR: Color = [0.25, 0.25, 0.25, 1.0];
/// The grey of secondary text (`0x808080`).
pub(super) const GREY: Color = [0.5, 0.5, 0.5, 1.0];

/// The geometry of a frame: GUI units and their scale.
#[derive(Debug, Clone, Copy)]
pub(super) struct Gui {
    pub(super) scale: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl Gui {
    pub(super) fn new(viewport: Rect) -> Self {
        let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
        Self {
            scale,
            width: (viewport.width / scale).ceil(),
            height: (viewport.height / scale).ceil(),
        }
    }

    /// A rectangle given in GUI units, in pixels.
    pub(super) fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            x * self.scale,
            y * self.scale,
            w * self.scale,
            h * self.scale,
        )
    }

    /// Half the width, as vanilla's integer division gives it.
    pub(super) fn cx(self) -> f32 {
        (self.width / 2.0).floor()
    }
}

impl OptionsScreen {
    /// `drawCenteredString` in white with the vanilla shadow.
    pub(super) fn title(ui: &mut Ui, atlas: &FontAtlas, gui: Gui, text: &str, y: f32) {
        let width = atlas.measure(text, gui.scale);
        let x = (gui.cx() * gui.scale - width * 0.5).round();
        let y = (y * gui.scale).round();
        ui.draw().text(
            [x + gui.scale, y + gui.scale],
            text,
            TextStyle::new(gui.scale, SHADOW),
        );
        ui.draw()
            .text([x, y], text, TextStyle::new(gui.scale, [1.0; 4]));
    }

    /// Draws `text` with its left edge at `x` and its top at `y` (GUI
    /// units), shadowed like `drawString`.
    pub(super) fn text_at(ui: &mut Ui, gui: Gui, (x, y): (f32, f32), text: &str, color: Color) {
        let at = [(x * gui.scale).round(), (y * gui.scale).round()];
        let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]];
        ui.draw().text(
            [at[0] + gui.scale, at[1] + gui.scale],
            text,
            TextStyle::new(gui.scale, shadow),
        );
        ui.draw().text(at, text, TextStyle::new(gui.scale, color));
    }

    /// Draws `text` centred on the GUI x coordinate `x`.
    pub(super) fn centered_text(
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        (x, y): (f32, f32),
        text: &str,
        color: Color,
    ) {
        let left = x - atlas.measure(text, 1.0) * 0.5;
        Self::text_at(ui, gui, (left, y), text, color);
    }

    /// Draws the dirt-less list background: the middle tint, the bars above
    /// and below and the 4-unit shadows of `GuiSlot.overlayBackground`.
    pub(super) fn list_background(ui: &mut Ui, gui: Gui, top: f32, bottom: f32) {
        let width = gui.width * gui.scale;
        ui.draw()
            .rect(gui.rect(0.0, top, gui.width, bottom - top), LIST_BACKGROUND);
        ui.draw()
            .rect(Rect::new(0.0, 0.0, width, top * gui.scale), LIST_BAR);
        ui.draw().rect(
            Rect::new(
                0.0,
                bottom * gui.scale,
                width,
                (gui.height - bottom) * gui.scale,
            ),
            LIST_BAR,
        );
        for step in 0..4 {
            let alpha = 1.0 - step as f32 / 4.0;
            let color = [0.0, 0.0, 0.0, alpha];
            ui.draw()
                .rect(gui.rect(0.0, top + step as f32, gui.width, 1.0), color);
            ui.draw().rect(
                gui.rect(0.0, bottom - 1.0 - step as f32, gui.width, 1.0),
                color,
            );
        }
    }

    /// Applies the wheel to the scroll offset.
    pub(super) fn scroll_list(&mut self, ui: &Ui, content: f32, visible: f32, row: f32) {
        let max = (content - (visible - 4.0)).max(0.0);
        for event in &ui.input().events {
            if let InputEvent::Wheel { delta } = event {
                self.scroll += delta * row / 2.0;
            }
        }
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// The vanilla scrollbar of a `GuiSlot`.
    pub(super) fn scrollbar(
        ui: &mut Ui,
        gui: Gui,
        x: f32,
        (top, bottom): (f32, f32),
        content: f32,
        scroll: f32,
    ) {
        let visible = bottom - top;
        let max = (content - (visible - 4.0)).max(0.0);
        if max <= 0.0 {
            return;
        }
        let thumb = (visible * visible / content).clamp(32.0, visible - 8.0);
        let y = (scroll * (visible - thumb) / max + top).max(top);
        ui.draw()
            .rect(gui.rect(x, top, 6.0, visible), [0.0, 0.0, 0.0, 1.0]);
        ui.draw()
            .rect(gui.rect(x, y, 6.0, thumb), [0.5, 0.5, 0.5, 1.0]);
        ui.draw()
            .rect(gui.rect(x, y, 5.0, thumb - 1.0), [0.75, 0.75, 0.75, 1.0]);
    }
}

/// The longest start of `text` that is at most `max_width` GUI units wide,
/// like `FontRenderer.trimStringToWidth`.
pub(super) fn fit_text(atlas: &FontAtlas, text: &str, max_width: f32) -> String {
    let mut fitted = String::new();
    for character in text.chars() {
        fitted.push(character);
        if atlas.measure(&fitted, 1.0) > max_width {
            fitted.pop();
            break;
        }
    }
    fitted
}

/// `text` without its `§x` formatting codes, for a single-colour line.
pub(super) fn strip_formatting(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\u{a7}' {
            characters.next();
        } else {
            plain.push(character);
        }
    }
    plain
}
