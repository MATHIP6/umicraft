// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat history of the session.

use std::time::Instant;

use umicraft_game::chat::{self as chat_rules, Position};
use umicraft_render::ChatLineView;

use super::state::Game;

/// A chat message from the chat JSON of the server, in the language of the
/// downloaded `.lang` file.
fn chat_line(json: &str) -> ChatLineView {
    crate::chat::line_from_runs(
        &chat_rules::parse(json, crate::chat::lang().table()),
        Instant::now(),
    )
}

/// A line the client writes itself. It is never mistaken for JSON (a player
/// named `null` or `123`).
pub(super) fn local_chat_line(text: &str) -> ChatLineView {
    crate::chat::line_from_runs(&chat_rules::local(text), Instant::now())
}

/// Shows a chat packet unless the chat visibility the player chose filters it
/// out (`GuiNewChat.drawChat` draws nothing when hidden; "commands only"
/// keeps the system messages).
pub(super) fn show_chat(game: &mut Game, json: &str, position: Position) {
    game.status_data.chat_messages += 1;
    if chat_rules::shows(game.chat.mode, position) {
        let line = chat_line(json);
        push_chat(game, line);
    }
}

/// Adds a message to the chat, which the HUD shows next.
pub(super) fn push_chat(game: &mut Game, line: ChatLineView) {
    game.chat.push(line);
    game.hud_dirty = true;
}
