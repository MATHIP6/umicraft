// SPDX-License-Identifier: MIT OR Apache-2.0

//! The connecting screen (MCP `GuiConnecting`).
//!
//! The status text is centred at `(cx, height / 2 - 50)` and the Cancel
//! button sits at `(cx - 100, height / 4 + 132)`. Vanilla shows
//! `connect.connecting` until the login starts and `connect.authorizing`
//! after it; Umicraft adds an optional second line for the target address or
//! the last progress step.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::game_menu::draw_default_background;
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::common::{escape_pressed, Gui, TextPlacement, DIM_GRAY, LINE_HEIGHT};

/// Where the connection attempt is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectStage {
    /// The TCP connection and the handshake (`connect.connecting`).
    Connecting,
    /// The login exchange (`connect.authorizing`).
    LoggingIn,
}

impl ConnectStage {
    /// The translation key of the stage text.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Connecting => "connect.connecting",
            Self::LoggingIn => "connect.authorizing",
        }
    }
}

/// What the player did on the connecting screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectingOutcome {
    /// Abort the attempt (the Cancel button or Escape).
    Cancel,
}

/// The rectangle of the Cancel button for `viewport`.
#[must_use]
pub fn connecting_cancel_rect(viewport: Rect) -> Rect {
    let gui = Gui::new(viewport);
    gui.rect(gui.cx() - 100.0, gui.h_div(4.0) + 132.0, 200.0, 20.0)
}

/// The connecting screen; it holds no state, the caller owns the attempt.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectingScreen;

impl ConnectingScreen {
    /// Creates the screen.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Draws one frame. `status` is an optional second line (for example the
    /// address being contacted).
    pub fn show(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        stage: ConnectStage,
        status: Option<&str>,
    ) -> Option<ConnectingOutcome> {
        draw_default_background(ui);
        let gui = Gui::new(ui.viewport());
        let y = (gui.height / 2.0).floor() - 50.0;
        gui.title(ui, atlas, lang.tr(stage.key()), gui.cx(), y);
        if let Some(status) = status {
            let at = [gui.cx(), y + LINE_HEIGHT + 4.0];
            gui.text(ui, atlas, status, TextPlacement::centered(at, DIM_GRAY));
        }

        ui.push_id("connecting");
        let cancel_id = ui.id("cancel");
        let rect = connecting_cancel_rect(ui.viewport());
        let cancel = gui.button(ui, cancel_id, rect, atlas, lang.tr("gui.cancel"), true);
        ui.pop_id();
        (cancel || escape_pressed(ui)).then_some(ConnectingOutcome::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, UiInput};
    use crate::menus::common::testing::{click_at, frame, key, VIEWPORT};

    fn run(input: UiInput, stage: ConnectStage) -> Option<ConnectingOutcome> {
        frame(input, |ui| {
            ConnectingScreen::new().show(
                ui,
                &FontAtlas::fallback(),
                &Lang::builtin(),
                stage,
                Some("host:25565"),
            )
        })
    }

    #[test]
    fn cancel_button_and_escape_cancel() {
        let rect = connecting_cancel_rect(VIEWPORT);
        // scale 3: (213 - 100) * 3, (60 + 132) * 3
        assert_eq!((rect.x, rect.y), (339.0, 576.0));
        assert_eq!(
            run(click_at(rect), ConnectStage::Connecting),
            Some(ConnectingOutcome::Cancel)
        );
        assert_eq!(
            run(key(Key::Escape), ConnectStage::LoggingIn),
            Some(ConnectingOutcome::Cancel)
        );
        assert_eq!(run(UiInput::default(), ConnectStage::Connecting), None);
    }

    #[test]
    fn the_stage_selects_the_vanilla_key() {
        assert_eq!(ConnectStage::Connecting.key(), "connect.connecting");
        assert_eq!(ConnectStage::LoggingIn.key(), "connect.authorizing");
    }
}
