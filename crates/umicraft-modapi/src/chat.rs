// SPDX-License-Identifier: MIT OR Apache-2.0

//! Local chat lines.

use crate::host;

/// Prints lines in the player's own chat; nothing is sent to the server.
///
/// Cheap to copy and usable from any thread. Get one with
/// [`InitCx::chat`](crate::InitCx::chat).
#[derive(Debug, Clone, Copy)]
pub struct Chat;

impl Chat {
    /// Shows `text` as a local chat line.
    ///
    /// Fire and forget: the host drops lines when its queue is full, and a
    /// mod can do nothing useful about that, so there is no error to handle.
    /// Does nothing before `init`.
    pub fn print(&self, text: &str) {
        let _ = host::call(|host, me| (host.chat_print)(me, host::utf8(text)));
    }
}
