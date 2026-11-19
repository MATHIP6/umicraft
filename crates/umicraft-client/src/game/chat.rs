// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat history of the session.

use std::time::Instant;

use umicraft_game::chat::{self as chat_rules, Position};
use umicraft_render::ChatLineView;

use super::mod_events::incoming_chat;
use super::state::Game;

/// A chat message from the chat JSON of the server, in the language of the
/// downloaded `.lang` file.
fn chat_line(json: &str) -> ChatLineView {
    crate::hud::chat::line_from_runs(
        &chat_rules::parse(json, crate::hud::chat::lang().table()),
        Instant::now(),
    )
}

/// A line the client writes itself. It is never mistaken for JSON (a player
/// named `null` or `123`).
pub(super) fn local_chat_line(text: &str) -> ChatLineView {
    crate::hud::chat::line_from_runs(&chat_rules::local(text), Instant::now())
}

/// Shows a chat packet unless the chat visibility the player chose filters it
/// out (`GuiNewChat.drawChat` draws nothing when hidden; "commands only"
/// keeps the system messages).
///
/// The mods see the line first and may hide or rewrite it (`ChatReceived`); a
/// hidden line is not counted, as if the server never sent it.
pub(super) fn show_chat(game: &mut Game, json: &str, position: Position) {
    let Some(json) = incoming_chat(&mut game.bus, json, position) else {
        return;
    };
    game.status_data.chat_messages += 1;
    if chat_rules::shows(game.chat.mode, position) {
        let line = chat_line(&json);
        push_chat(game, line);
    }
}

/// Shows an action-bar packet above the hotbar, after the mods had their say.
pub(super) fn show_action_bar(game: &mut Game, json: &str) {
    if let Some(json) = incoming_chat(&mut game.bus, json, Position::ActionBar) {
        game.social.action_bar.show(&json);
        game.hud_dirty = true;
    }
}

/// Adds a message to the chat, which the HUD shows next.
pub(super) fn push_chat(game: &mut Game, line: ChatLineView) {
    game.chat.push(line);
    game.hud_dirty = true;
}
