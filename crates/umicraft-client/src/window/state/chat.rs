// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat line of the window: the text being typed, the sent history, the
//! scroll and what the pointer hovers.

use std::time::Instant;

use umicraft_game::chat::{ChatInput, SentHistory};
use umicraft_render::InputHit;

/// The chat line: what is typed, the history, the scroll and the hover.
pub(in crate::window) struct ChatUi {
    /// The line the player is typing while [`Screen::Chat`] is current.
    pub(in crate::window) input: ChatInput,
    /// The messages the player sent, for the up and down keys.
    pub(in crate::window) sent: SentHistory,
    /// How many rows the open chat is scrolled up.
    pub(in crate::window) scroll: usize,
    /// When the newest message the scroll already accounted for arrived
    /// (`GuiNewChat` keeps a scrolled-back chat on the rows being read).
    pub(in crate::window) newest_seen: Option<Instant>,
    /// When the chat opened; the caret blinks from here.
    pub(in crate::window) opened: Instant,
    /// The completion answer the chat already handled.
    pub(in crate::window) seen_completions: u64,
    /// The chat run under the pointer in the last frame (click and hover).
    pub(in crate::window) hover: Option<umicraft_render::ChatRunView>,
    /// Where the pointer is on the chat input line, as of the last frame.
    pub(in crate::window) line: Option<InputHit>,
}
