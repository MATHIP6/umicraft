// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Microsoft device-code dialog shown while the player signs in.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::Color;
use crate::menus::background::draw_menu_background;
use crate::menus::common::{escape_pressed, Gui, TextPlacement, DIM_GRAY, WHITE};
use crate::menus::connecting::connecting_cancel_rect;
use crate::text::FontAtlas;

use super::AccountsOutcome;

/// The aqua of the address (`§b`).
const URL_COLOR: Color = [0.333, 1.0, 1.0, 1.0];
/// The yellow of the code (`§e`).
const CODE_COLOR: Color = [1.0, 1.0, 0.333, 1.0];

/// The code and the page the player must visit to finish a Microsoft login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodePrompt {
    /// The short code to type on the page.
    pub user_code: String,
    /// The address of the page.
    pub url: String,
}

/// Draws one frame of the dialog; Cancel or Escape stops the login.
pub(super) fn show(
    ui: &mut Ui,
    atlas: &FontAtlas,
    lang: &Lang,
    prompt: &DeviceCodePrompt,
) -> Option<AccountsOutcome> {
    draw_menu_background(ui);
    let gui = Gui::new(ui.viewport());
    let cx = gui.cx();
    let line = |ui: &mut Ui, text: &str, y: f32, color: Color| {
        gui.text(ui, atlas, text, TextPlacement::centered([cx, y], color));
    };
    line(ui, lang.tr("umicraft.accounts.device.title"), 40.0, WHITE);
    line(ui, lang.tr("umicraft.accounts.device.open"), 70.0, WHITE);
    line(ui, &prompt.url, 82.0, URL_COLOR);
    line(ui, lang.tr("umicraft.accounts.device.enter"), 104.0, WHITE);
    let code = TextPlacement {
        factor: 2.0,
        ..TextPlacement::centered([cx, 118.0], CODE_COLOR)
    };
    gui.text(ui, atlas, &prompt.user_code, code);
    line(
        ui,
        lang.tr("umicraft.accounts.device.waiting"),
        148.0,
        DIM_GRAY,
    );

    ui.push_id("device-code");
    let cancel_id = ui.id("cancel");
    let rect = connecting_cancel_rect(ui.viewport());
    let cancel = gui.button(ui, cancel_id, rect, atlas, lang.tr("gui.cancel"), true);
    ui.pop_id();
    (cancel || escape_pressed(ui)).then_some(AccountsOutcome::CancelLogin)
}
