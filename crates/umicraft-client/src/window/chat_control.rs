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
use crate::clipboard;
use crate::game::GameEvent;
use crate::screen::Screen;

/// Rows a Page Up or Page Down scrolls the open chat.
pub(super) const CHAT_PAGE: isize = 19;
/// Rows one mouse-wheel notch scrolls the open chat.
pub(super) const CHAT_WHEEL_ROWS: isize = 7;
/// The furthest the chat can be scrolled: more than the 100 messages kept.
pub(super) const CHAT_SCROLL_MAX: isize = 2_000;

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
        let (word, extend) = (self.ctrl_held, self.shift_held);
        if self.ctrl_held && self.chat_shortcut(code) {
            return;
        }
        match code {
            KeyCode::Escape => self.set_screen(Screen::Chat.after_escape(self.options_origin)),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit_chat(),
            KeyCode::Tab => self.complete_chat(),
            KeyCode::ArrowUp => self.chat_input.history_previous(&mut self.sent_messages),
            KeyCode::ArrowDown => self.chat_input.history_next(&mut self.sent_messages),
            KeyCode::PageUp => self.scroll_chat(CHAT_PAGE),
            KeyCode::PageDown => self.scroll_chat(-CHAT_PAGE),
            KeyCode::Backspace => self.chat_input.backspace(word),
            KeyCode::Delete => self.chat_input.delete(word),
            KeyCode::ArrowLeft => self.chat_input.left(word, extend),
            KeyCode::ArrowRight => self.chat_input.right(word, extend),
            KeyCode::Home => self.chat_input.home(extend),
            KeyCode::End => self.chat_input.end(extend),
            _ => {
                // Control combinations are shortcuts, not text.
                if !self.ctrl_held {
                    if let Some(text) = event.text.as_ref() {
                        self.chat_input.insert(text);
                    }
                }
            }
        }
    }
    /// Control+A, +C, +X, +V, +Z and +Y on the chat line (`GuiScreen.isKeyComboCtrl*`).
    /// Returns whether `code` was one of them; the clipboard is the system's.
    fn chat_shortcut(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::KeyA => self.chat_input.select_all(),
            KeyCode::KeyC => copy_to_clipboard(&self.chat_input.selected_text()),
            KeyCode::KeyX => copy_to_clipboard(&self.chat_input.cut()),
            KeyCode::KeyV => {
                if let Some(text) = clipboard::read() {
                    self.chat_input.paste(&text);
                }
            }
            KeyCode::KeyZ if self.shift_held => self.chat_input.redo(),
            KeyCode::KeyZ => self.chat_input.undo(),
            KeyCode::KeyY => self.chat_input.redo(),
            _ => return false,
        }
        true
    }
    /// Opens the chat with `initial` already typed (`/` for the command key).
    pub(super) fn open_chat(&mut self, initial: &str) {
        self.set_screen(Screen::Chat);
        self.chat_input = ChatInput::new(initial);
        self.sent_messages.reset();
        self.chat_scroll = 0;
        self.chat_opened = Instant::now();
    }
    /// Sends the typed line, remembers it for the history and closes the chat.
    pub(super) fn submit_chat(&mut self) {
        let message = commands::outgoing(&self.chat_input.text());
        self.set_screen(Screen::None);
        if let Some(message) = message {
            self.sent_messages.push(&message);
            self.send_event(GameEvent::Chat(message));
        }
    }
    /// Presses Tab in the chat: asks the server or cycles its answer.
    pub(super) fn complete_chat(&mut self) {
        match self.chat_input.tab() {
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
        match self.chat_line {
            Some(hit) if hit.inside => {
                self.chat_input
                    .press(hit.index, self.shift_held, Instant::now());
            }
            _ => self.chat_click(),
        }
    }
    /// Acts on a left click in the open chat: `run_command` sends,
    /// `suggest_command` fills the input line, Shift inserts the run's
    /// insertion text (`GuiChat.mouseClicked`). Links are never followed.
    pub(super) fn chat_click(&mut self) {
        let Some(run) = self.chat_hover.as_ref() else {
            return;
        };
        match on_click(
            run.click.as_ref(),
            run.insertion.as_deref(),
            self.shift_held,
        ) {
            ClickOutcome::Send(text) => {
                self.send_event(GameEvent::Chat(text));
            }
            ClickOutcome::Suggest(text) => self.chat_input.set_text(&text),
            ClickOutcome::Insert(text) => self.chat_input.insert(&text),
            ClickOutcome::Nothing => {}
        }
    }
    /// Scrolls the open chat by `rows` (positive is towards older messages).
    pub(super) fn scroll_chat(&mut self, rows: isize) {
        self.chat_scroll = (self.chat_scroll as isize + rows).clamp(0, CHAT_SCROLL_MAX) as usize;
    }
    /// Hands a new tab-completion answer to the chat input.
    pub(super) fn process_completions(&mut self) {
        let Some(world) = self.world.as_ref() else {
            return;
        };
        let serial = world.hud().completions_serial;
        if serial == self.seen_completions {
            return;
        }
        let matches = world.hud().completions.clone();
        self.seen_completions = serial;
        if self.screen == Screen::Chat {
            if let Some(listing) = self.chat_input.on_completions(&matches) {
                self.send_event(GameEvent::LocalChat(listing));
            }
        }
    }
}
