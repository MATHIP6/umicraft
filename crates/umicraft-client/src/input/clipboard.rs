// SPDX-License-Identifier: MIT OR Apache-2.0

//! The system clipboard for the text fields (Control+C, Control+X, Control+V;
//! MCP `GuiScreen.getClipboardString` and `setClipboardString`).
//!
//! One handle is kept for the life of the process: on Linux the clipboard owner
//! has to stay alive to serve a paste. A clipboard that cannot be opened (no
//! display, a locked clipboard) reads as empty and ignores writes, like the
//! reference, which returns an empty string on any failure.
//!
//! On Wayland the data-control backend is needed for other applications to see
//! a copy: the X11 backend alone only reaches X11 clients.

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

#[cfg(test)]
mod tests {
    use std::process::Command;

    /// Regression test: on Wayland, a copy must reach `wl-paste` (the X11 backend
    /// alone left it on the old content). Needs a Wayland session and `wl-paste`,
    /// so it only runs on request: `cargo test -p umicraft-client -- --ignored wayland`.
    #[test]
    #[ignore = "needs a Wayland session and wl-paste"]
    fn wayland_applications_see_a_copy() {
        super::write("umicraft-wayland-check");
        let pasted = Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .expect("wl-paste must be installed for this test");
        assert_eq!(pasted.stdout, b"umicraft-wayland-check");
    }
}
