// SPDX-License-Identifier: MIT OR Apache-2.0

//! The account chooser (ADR-0042 style, pure UI: no login, no network).
//!
//! Layout and look follow the server list: a title with an optional status
//! line, a scrollable list of rows (head, name, coloured badge) and two rows
//! of buttons (see [`layout`]). Add, Edit, Delete and the Microsoft device
//! code each open a full-screen dialog; the screen reports what the player
//! chose as an [`AccountsOutcome`] and the caller does the work, then feeds
//! the result back through [`AccountsScreen::set_rows`],
//! [`AccountsScreen::set_error`] and [`AccountsScreen::set_busy`].
//!
//! Double click: like the server list, two clicks on one row within five
//! [`AccountsScreen::tick`] calls play it.

mod add_dialog;
mod device_code_dialog;
#[cfg(test)]
mod dialog_tests;
mod edit_dialog;
mod layout;
mod list;
mod name_field;
mod row;
#[cfg(test)]
mod tests;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::text::FontAtlas;

use super::common::ScrollbarDrag;
use super::confirm::{ConfirmOutcome, ConfirmScreen};

use add_dialog::AddDialog;
use edit_dialog::EditDialog;

pub use device_code_dialog::DeviceCodePrompt;
pub use layout::{
    account_row_at, account_row_rect, accounts_layout, name_dialog_layout, offset_to_show,
    AccountsLayout, NameDialogLayout,
};
pub use name_field::{is_valid_username, MAX_USERNAME_CHARS};
pub use row::{AccountBadge, AccountRow};

/// The ticks within which a second click on a row plays it (250 ms).
pub(super) const DOUBLE_CLICK_TICKS: u32 = 5;
/// The red of the error line (`§c`).
pub(super) const ERROR_RED: [f32; 4] = [1.0, 0.333, 0.333, 1.0];

/// What the player chose; the caller acts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountsOutcome {
    /// Play with account `i` (Play button, double click or Enter).
    Play(usize),
    /// Create an offline account with this valid username.
    AddOffline(String),
    /// Start a Microsoft login; the caller answers with
    /// [`AccountsScreen::set_device_code`].
    AddMicrosoft,
    /// Rename offline account `index` to the valid `username`.
    EditOffline {
        /// The account's row.
        index: usize,
        /// The new username.
        username: String,
    },
    /// Delete account `i` (already confirmed by the player).
    Delete(usize),
    /// Refresh the sessions.
    Refresh,
    /// Leave the screen (Cancel or Escape).
    Cancel,
    /// Stop the Microsoft login in progress (the dialog's Cancel or Escape).
    CancelLogin,
    /// Open this address in the player's browser (the sign-in page). The
    /// caller decides whether it is safe to open.
    OpenUrl(String),
    /// Put this text on the clipboard (the sign-in page or the code).
    CopyText(String),
}

/// What a dialog answered: an outcome, or a plain close.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DialogEvent {
    Outcome(AccountsOutcome),
    Close,
}

/// The dialog on top of the list, if any.
#[derive(Debug, Clone)]
enum Dialog {
    None,
    Add(AddDialog),
    Edit(EditDialog),
    Delete {
        index: usize,
        confirm: ConfirmScreen,
    },
}

/// The rows, selection, scroll, click history and dialogs of the chooser.
#[derive(Debug, Clone)]
pub struct AccountsScreen {
    rows: Vec<AccountRow>,
    selected: Option<usize>,
    offset: f32,
    ticks: u32,
    last_click: Option<(usize, u32)>,
    bar: ScrollbarDrag,
    dialog: Dialog,
    device_code: Option<DeviceCodePrompt>,
    error: Option<String>,
    busy: bool,
}

impl AccountsScreen {
    /// Creates the screen with `rows` and `selected` (dropped when out of
    /// range).
    #[must_use]
    pub fn new(rows: Vec<AccountRow>, selected: Option<usize>) -> Self {
        let mut screen = Self {
            rows: Vec::new(),
            selected,
            offset: 0.0,
            ticks: 0,
            last_click: None,
            bar: ScrollbarDrag::default(),
            dialog: Dialog::None,
            device_code: None,
            error: None,
            busy: false,
        };
        screen.set_rows(rows);
        screen
    }

    /// Replaces the rows; the selection stays when it still points at a row.
    pub fn set_rows(&mut self, rows: Vec<AccountRow>) {
        self.rows = rows;
        self.selected = self.selected.filter(|&index| index < self.rows.len());
        self.last_click = None;
    }

    /// Shows (or, with `None`, hides) the Microsoft sign-in dialog.
    pub fn set_device_code(&mut self, prompt: Option<DeviceCodePrompt>) {
        self.device_code = prompt;
    }

    /// Shows (or clears) an error line under the title.
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    /// Greys the buttons that start work while the caller is busy.
    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    /// The selected row.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Advances the click clock and the caret blink by one game tick.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        match &mut self.dialog {
            Dialog::Add(dialog) => dialog.tick(),
            Dialog::Edit(dialog) => dialog.tick(),
            Dialog::None | Dialog::Delete { .. } => {}
        }
    }

    /// Draws one frame and returns the player's choice. The device-code
    /// dialog, when set, covers everything else.
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<AccountsOutcome> {
        if let Some(prompt) = &self.device_code {
            return device_code_dialog::show(ui, atlas, lang, prompt);
        }
        let event = match &mut self.dialog {
            Dialog::None => return self.show_list(ui, atlas, lang),
            Dialog::Add(dialog) => dialog.show(ui, atlas, lang),
            Dialog::Edit(dialog) => dialog.show(ui, atlas, lang),
            Dialog::Delete { index, confirm } => {
                confirm.show(ui, atlas, lang).map(|answer| match answer {
                    ConfirmOutcome::Yes => DialogEvent::Outcome(AccountsOutcome::Delete(*index)),
                    ConfirmOutcome::No => DialogEvent::Close,
                })
            }
        };
        let event = event?;
        self.dialog = Dialog::None;
        match event {
            DialogEvent::Outcome(outcome) => Some(outcome),
            DialogEvent::Close => None,
        }
    }
}
