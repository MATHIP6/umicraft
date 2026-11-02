// SPDX-License-Identifier: MIT OR Apache-2.0

//! Outgoing chat text: the length limit and what makes a line a command.
//!
//! Sources: MCP `GuiChat.initGui` (`setMaxStringLength(100)`),
//! `GuiChat.keyTyped` (the line is trimmed, an empty line is not sent) and
//! `C01PacketChatMessage` (the server disconnects on more than 100
//! characters). A leading `/` makes the line a command; the key bound to
//! "command" opens the chat with that slash already typed
//! (`Minecraft.runTick`, `displayGuiScreen(new GuiChat("/"))`).

/// The longest message the client sends, in characters.
pub const MAX_LEN: usize = 100;

/// The text the command key puts into a newly opened chat.
pub const COMMAND_PREFIX: &str = "/";

/// Whether `message` is a command (starts with `/`).
#[must_use]
pub fn is_command(message: &str) -> bool {
    message.starts_with('/')
}

/// The message to send for the typed line: trimmed, `None` when empty or
/// longer than [`MAX_LEN`] characters.
///
/// The server limit counts characters, not bytes.
#[must_use]
pub fn outgoing(message: &str) -> Option<String> {
    let message = message.trim();
    (!message.is_empty() && message.chars().count() <= MAX_LEN).then(|| message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_slash_makes_a_command() {
        assert!(is_command("/gamemode 1"));
        assert!(!is_command("hello /not"));
        assert!(!is_command(""));
    }

    #[test]
    fn outgoing_text_is_trimmed_and_limited_in_characters() {
        assert_eq!(outgoing("  hi  ").as_deref(), Some("hi"));
        assert_eq!(outgoing("   "), None);
        assert!(outgoing(&"\u{e9}".repeat(MAX_LEN)).is_some());
        assert_eq!(outgoing(&"\u{e9}".repeat(MAX_LEN + 1)), None);
    }
}
