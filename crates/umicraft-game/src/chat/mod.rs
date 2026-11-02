// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat: components, styles, language table, click and hover events,
//! wrapping, the input line with its history and Tab completion, and the
//! chat visibility filter (P9-d).
//!
//! The behaviour follows MCP `GuiNewChat`, `GuiChat`, `GuiScreen`
//! (`handleComponentClick`, `handleComponentHover`), `GuiUtilRenderComponents`
//! and `FontRenderer` (the `§` codes). Everything here is I/O-free: the
//! client feeds it keys, packets and the font metrics.

pub mod commands;
pub mod complete;
pub mod component;
pub mod events;
pub mod history;
pub mod input;
pub mod lang;
pub mod style;
pub mod wrap;

pub use component::{local, parse, plain, unformatted_text, Run};
pub use events::{on_click, ClickAction, ClickEvent, ClickOutcome, HoverEvent};
pub use history::SentHistory;
pub use input::{ChatInput, TabOutcome};
pub use lang::LangTable;
pub use style::{Color, Style};

/// Where a chat packet asks the client to show its text (`S02PacketChat`
/// type byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// 0: a chat message.
    Chat,
    /// 1: a system message (command feedback, join and leave messages).
    System,
    /// 2: the action bar above the hotbar, not the chat.
    ActionBar,
}

impl Position {
    /// The position of a packet's type byte. Unknown values are chat, as
    /// `NetHandlerPlayClient.handleChat` only singles out type 2.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::System,
            2 => Self::ActionBar,
            _ => Self::Chat,
        }
    }
}

/// Whether the chat shows a message at `position` with the chat visibility
/// `mode` (`EnumChatVisibility` wire value: 0 full, 1 commands only, 2
/// hidden).
///
/// "Commands only" lets system messages through and drops chat messages;
/// hidden drops both (`GuiNewChat.drawChat` draws nothing when hidden, and a
/// server honouring the `ClientSettings` mode sends nothing else). The action
/// bar is not chat and is never filtered here.
#[must_use]
pub const fn shows(mode: u8, position: Position) -> bool {
    match position {
        Position::ActionBar => true,
        Position::System => mode < 2,
        Position::Chat => mode == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_byte_selects_the_position() {
        assert_eq!(Position::from_byte(0), Position::Chat);
        assert_eq!(Position::from_byte(1), Position::System);
        assert_eq!(Position::from_byte(2), Position::ActionBar);
        assert_eq!(Position::from_byte(9), Position::Chat);
    }

    #[test]
    fn visibility_filters_chat_and_system_messages() {
        assert!(shows(0, Position::Chat) && shows(0, Position::System));
        assert!(!shows(1, Position::Chat) && shows(1, Position::System));
        assert!(!shows(2, Position::Chat) && !shows(2, Position::System));
        assert!(shows(2, Position::ActionBar), "the action bar is not chat");
    }
}
