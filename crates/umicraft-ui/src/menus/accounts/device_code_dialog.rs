// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Microsoft device-code dialog shown while the player signs in.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
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

/// The three buttons that let the player use the sign-in page: the dialog is
/// a pure function of the viewport so tests and drawing share the rectangles.
#[derive(Debug, Clone, Copy)]
pub(super) struct DeviceLayout {
    /// Opens the page in the browser.
    pub open: Rect,
    /// Copies the page address.
    pub copy_link: Rect,
    /// Copies the code.
    pub copy_code: Rect,
}

/// Where the dialog's buttons are, above Cancel (GUI units: 200 wide, like the
/// connecting screen's button, split in two for the copy buttons).
pub(super) fn device_layout(viewport: Rect) -> DeviceLayout {
    let gui = Gui::new(viewport);
    let x = gui.cx() - 100.0;
    DeviceLayout {
        open: gui.rect(x, 136.0, 200.0, 20.0),
        copy_link: gui.rect(x, 160.0, 98.0, 20.0),
        copy_code: gui.rect(x + 102.0, 160.0, 98.0, 20.0),
    }
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
    line(ui, lang.tr("umicraft.accounts.device.title"), 30.0, WHITE);
    line(ui, lang.tr("umicraft.accounts.device.open"), 50.0, WHITE);
    line(ui, &prompt.url, 62.0, URL_COLOR);
    line(ui, lang.tr("umicraft.accounts.device.enter"), 82.0, WHITE);
    let code = TextPlacement {
        factor: 2.0,
        ..TextPlacement::centered([cx, 96.0], CODE_COLOR)
    };
    gui.text(ui, atlas, &prompt.user_code, code);
    line(
        ui,
        lang.tr("umicraft.accounts.device.waiting"),
        122.0,
        DIM_GRAY,
    );

    ui.push_id("device-code");
    let layout = device_layout(ui.viewport());
    let button = |ui: &mut Ui, name: &str, rect: Rect, key: &str| {
        let id = ui.id(name);
        gui.button(ui, id, rect, atlas, lang.tr(key), true)
    };
    let open = button(
        ui,
        "open",
        layout.open,
        "umicraft.accounts.device.button.open",
    );
    let copy_link = button(
        ui,
        "copy-link",
        layout.copy_link,
        "umicraft.accounts.device.button.copy_link",
    );
    let copy_code = button(
        ui,
        "copy-code",
        layout.copy_code,
        "umicraft.accounts.device.button.copy_code",
    );
    let cancel = button(
        ui,
        "cancel",
        connecting_cancel_rect(ui.viewport()),
        "gui.cancel",
    );
    ui.pop_id();
    if open {
        return Some(AccountsOutcome::OpenUrl(prompt.url.clone()));
    }
    if copy_link {
        return Some(AccountsOutcome::CopyText(prompt.url.clone()));
    }
    if copy_code {
        return Some(AccountsOutcome::CopyText(prompt.user_code.clone()));
    }
    (cancel || escape_pressed(ui)).then_some(AccountsOutcome::CancelLogin)
}
