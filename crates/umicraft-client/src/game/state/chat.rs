// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat of the session: the history, the visibility the server was told
//! and the completions it offered.

use std::collections::VecDeque;

use umicraft_render::ChatLineView;

/// The chat history, its visibility and the tab completions.
#[derive(Default)]
pub(in crate::game) struct ChatState {
    /// The chat messages, oldest first.
    pub(in crate::game) lines: VecDeque<ChatLineView>,
    /// The chat visibility the server was told (`ClientSettings` byte).
    pub(in crate::game) mode: u8,
    /// The completions the server offered for the open chat line.
    pub(in crate::game) completions: Vec<String>,
    /// Counts the tab-completion answers.
    pub(in crate::game) completions_serial: u64,
}
