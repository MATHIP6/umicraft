// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the chat: the keys of the open chat line, sending, history,
//! tab completion and clicks on chat runs (P9).
//!
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use std::time::Instant;

use umicraft_game::chat::{commands, on_click, ChatInput, ClickOutcome, TabOutcome};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::KeyCode;

use super::App;
use crate::game::GameEvent;
use crate::input::clipboard;
use crate::screens::Screen;

/// Rows a Page Up or Page Down scrolls the open chat.
pub(super) const CHAT_PAGE: isize = 19;
/// Rows one mouse-wheel notch scrolls the open chat.
pub(super) const CHAT_WHEEL_ROWS: isize = 7;
/// The furthest the chat can be scrolled: more than the 100 messages kept.
pub(super) const CHAT_SCROLL_MAX: isize = 2_000;

/// The partial command name when `text` is `/` followed by letters only: the one
/// shape of line whose completions can include a command of a mod (arguments are
/// the server's business).
fn command_prefix(text: &str) -> Option<&str> {
    let partial = text.strip_prefix('/')?;
    (!partial.contains(char::is_whitespace)).then_some(partial)
}

/// The server's completions plus the names of the mods' commands (`local`, without
/// the slash), sorted and without duplicates so the Tab cycle is stable. The server's
/// list is returned untouched when no mod has a matching command.
fn merge_completions(server: Vec<String>, local: Vec<String>) -> Vec<String> {
    if local.is_empty() {
        return server;
    }
    let mut merged = server;
    merged.extend(local.into_iter().map(|name| format!("/{name}")));
    merged.sort_by_key(|name| name.to_lowercase());
    merged.dedup();
    merged
}

/// Puts `text` on the clipboard, leaving it untouched when `text` is empty
/// (copying an empty selection must not wipe what the user had copied).
fn copy_to_clipboard(text: &str) {
    if !text.is_empty() {
        clipboard::write(text);
    }
}

impl App {
    /// Handles a key press while the chat input is open.
    pub(super) fn handle_chat_key(&mut self, event: KeyEvent) {
        let Some(code) = self.text_key_code(&event) else {
            return;
        };
        if event.state != ElementState::Pressed {
            return;
        }
        let (word, extend) = (self.input.ctrl_held, self.input.shift_held);
        if self.input.ctrl_held && self.chat_shortcut(code) {
            return;
        }
        match code {
            KeyCode::Escape => self.set_screen(Screen::Chat.after_escape(self.options_menu.origin)),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit_chat(),
            KeyCode::Tab => self.complete_chat(),
            KeyCode::ArrowUp => self.chat.input.history_previous(&mut self.chat.sent),
            KeyCode::ArrowDown => self.chat.input.history_next(&mut self.chat.sent),
            KeyCode::PageUp => self.scroll_chat(CHAT_PAGE),
            KeyCode::PageDown => self.scroll_chat(-CHAT_PAGE),
            KeyCode::Backspace => self.chat.input.backspace(word),
            KeyCode::Delete => self.chat.input.delete(word),
            KeyCode::ArrowLeft => self.chat.input.left(word, extend),
            KeyCode::ArrowRight => self.chat.input.right(word, extend),
            KeyCode::Home => self.chat.input.home(extend),
            KeyCode::End => self.chat.input.end(extend),
            _ => {
                // Control combinations are shortcuts, not text.
                if !self.input.ctrl_held {
                    if let Some(text) = event.text.as_ref() {
                        self.chat.input.insert(text);
                    }
                }
            }
        }
    }
    /// Control+A, +C, +X, +V, +Z and +Y on the chat line (`GuiScreen.isKeyComboCtrl*`).
    /// Returns whether `code` was one of them; the clipboard is the system's.
    fn chat_shortcut(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::KeyA => self.chat.input.select_all(),
            KeyCode::KeyC => copy_to_clipboard(&self.chat.input.selected_text()),
            KeyCode::KeyX => copy_to_clipboard(&self.chat.input.cut()),
            KeyCode::KeyV => {
                if let Some(text) = clipboard::read() {
                    self.chat.input.paste(&text);
                }
            }
            KeyCode::KeyZ if self.input.shift_held => self.chat.input.redo(),
            KeyCode::KeyZ => self.chat.input.undo(),
            KeyCode::KeyY => self.chat.input.redo(),
            _ => return false,
        }
        true
    }
    /// Opens the chat with `initial` already typed (`/` for the command key).
    pub(super) fn open_chat(&mut self, initial: &str) {
        self.set_screen(Screen::Chat);
        self.chat.input = ChatInput::new(initial);
        self.chat.sent.reset();
        self.chat.scroll = 0;
        self.chat.opened = Instant::now();
    }
    /// Sends the typed line, remembers it for the history and closes the chat.
    pub(super) fn submit_chat(&mut self) {
        let message = commands::outgoing(&self.chat.input.text());
        self.set_screen(Screen::None);
        if let Some(message) = message {
            self.chat.sent.push(&message);
            self.send_event(GameEvent::Chat(message));
        }
    }
    /// Presses Tab in the chat: asks the server or cycles its answer.
    pub(super) fn complete_chat(&mut self) {
        match self.chat.input.tab() {
            TabOutcome::Nothing => {}
            TabOutcome::Request(text) => {
                self.send_event(GameEvent::TabComplete(text));
            }
            TabOutcome::Cycled { listing } => {
                if let Some(listing) = listing {
                    self.send_event(GameEvent::LocalChat(listing));
                }
            }
        }
    }
    /// A left press in the open chat: on the input line it places the caret
    /// or selects (click, double click, triple click, Shift+click); elsewhere
    /// it acts on the chat run under the pointer.
    pub(super) fn chat_press(&mut self) {
        match self.chat.line {
            Some(hit) if hit.inside => {
                self.chat
                    .input
                    .press(hit.index, self.input.shift_held, Instant::now());
            }
            _ => self.chat_click(),
        }
    }
    /// Acts on a left click in the open chat: `run_command` sends,
    /// `suggest_command` fills the input line, Shift inserts the run's
    /// insertion text (`GuiChat.mouseClicked`). Links are never followed.
    pub(super) fn chat_click(&mut self) {
        let Some(run) = self.chat.hover.as_ref() else {
            return;
        };
        match on_click(
            run.click.as_ref(),
            run.insertion.as_deref(),
            self.input.shift_held,
        ) {
            ClickOutcome::Send(text) => {
                self.send_event(GameEvent::Chat(text));
            }
            ClickOutcome::Suggest(text) => self.chat.input.set_text(&text),
            ClickOutcome::Insert(text) => self.chat.input.insert(&text),
            ClickOutcome::Nothing => {}
        }
    }
    /// Scrolls the open chat by `rows` (positive is towards older messages).
    pub(super) fn scroll_chat(&mut self, rows: isize) {
        self.chat.scroll = (self.chat.scroll as isize + rows).clamp(0, CHAT_SCROLL_MAX) as usize;
    }
    /// Adds the commands of the mods to the server's answer when the typed line is a
    /// command name being completed.
    fn with_mod_commands(&self, server: Vec<String>) -> Vec<String> {
        let text = self.chat.input.text();
        let local = match (command_prefix(&text), &self.external_mods) {
            (Some(partial), Some(mods)) => mods.complete_commands(partial),
            _ => Vec::new(),
        };
        merge_completions(server, local)
    }

    /// Hands a new tab-completion answer to the chat input.
    pub(super) fn process_completions(&mut self) {
        let Some(world) = self.world.as_ref() else {
            return;
        };
        let serial = world.hud().completions_serial;
        if serial == self.chat.seen_completions {
            return;
        }
        let matches = self.with_mod_commands(world.hud().completions.clone());
        self.chat.seen_completions = serial;
        if self.screen == Screen::Chat {
            if let Some(listing) = self.chat.input.on_completions(&matches) {
                self.send_event(GameEvent::LocalChat(listing));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn only_a_bare_command_name_is_completed_with_the_mods() {
        assert_eq!(command_prefix("/he"), Some("he"));
        assert_eq!(command_prefix("/"), Some(""));
        assert_eq!(command_prefix("/give Ste"), None);
        assert_eq!(command_prefix("hello"), None);
    }

    #[test]
    fn local_names_are_merged_sorted_and_deduplicated() {
        let merged = merge_completions(names(&["/help", "/hat"]), names(&["hello", "help"]));
        assert_eq!(merged, names(&["/hat", "/hello", "/help"]));
    }

    #[test]
    fn the_servers_answer_is_kept_as_is_without_local_commands() {
        let server = names(&["/tp", "/give"]);
        assert_eq!(merge_completions(server.clone(), Vec::new()), server);
    }
}
