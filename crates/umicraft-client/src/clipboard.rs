// SPDX-License-Identifier: MIT OR Apache-2.0

//! The system clipboard for the text fields (Control+C, Control+X, Control+V;
//! MCP `GuiScreen.getClipboardString` and `setClipboardString`).
//!
//! One handle is kept for the life of the process: on Linux the clipboard owner
//! has to stay alive to serve a paste. A clipboard that cannot be opened (no
//! display, a locked clipboard) reads as empty and ignores writes, like the
//! reference, which returns an empty string on any failure.

use std::sync::{Mutex, OnceLock};

/// The process-wide handle, opened on first use.
fn handle() -> &'static Mutex<Option<arboard::Clipboard>> {
    static HANDLE: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    HANDLE.get_or_init(|| Mutex::new(arboard::Clipboard::new().ok()))
}

/// The text on the clipboard, if it holds any.
#[must_use]
pub fn read() -> Option<String> {
    let mut guard = handle().lock().ok()?;
    guard
        .as_mut()?
        .get_text()
        .ok()
        .filter(|text| !text.is_empty())
}

/// Puts `text` on the clipboard.
pub fn write(text: &str) {
    if let Ok(mut guard) = handle().lock() {
        if let Some(clipboard) = guard.as_mut() {
            if let Err(error) = clipboard.set_text(text) {
                tracing::debug!(%error, "the clipboard refused the text");
            }
        }
    }
}
