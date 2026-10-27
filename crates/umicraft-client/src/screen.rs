// SPDX-License-Identifier: MIT OR Apache-2.0

//! The screen the windowed client shows, and the transitions between screens
//! (M10, ADR-0017).
//!
//! Before M10 the window kept one boolean per screen (`chat_open`,
//! `inventory_open`, `show_debug_screen`), so every event handler had to
//! repeat the same `a || b || c` test and two screens could be open at once.
//! [`Screen`] replaces them: exactly one screen is current, and the input
//! router asks it who owns the keyboard and the mouse. The type is plain data
//! with no window, GPU or network, so the transitions are unit-tested.

/// The screen that owns the keyboard and the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// No screen: the keys and the mouse drive the game.
    #[default]
    None,
    /// The chat line is being typed.
    Chat,
    /// The player inventory (a server container is shown by the same screen).
    Inventory,
    /// The menu opened with Escape.
    GameMenu,
    /// The options, opened from the game menu.
    Options,
    /// The M4 debug screen (F2).
    Debug,
}

impl Screen {
    /// Whether the immediate-mode UI receives the pointer and the keys.
    #[must_use]
    pub const fn uses_ui(self) -> bool {
        matches!(
            self,
            Self::Inventory | Self::GameMenu | Self::Options | Self::Debug
        )
    }

    /// Whether the game ignores movement, look and click input.
    #[must_use]
    pub const fn blocks_game_input(self) -> bool {
        !matches!(self, Self::None)
    }

    /// The screen after a fresh Escape press.
    ///
    /// With no screen, Escape opens the game menu. The chat, the inventory and
    /// the menu close; the options go back to the menu they were opened from.
    /// The debug screen is closed by F2 only, because Escape is a key its
    /// widgets may consume.
    #[must_use]
    pub const fn after_escape(self) -> Self {
        match self {
            Self::None => Self::GameMenu,
            Self::Chat | Self::Inventory | Self::GameMenu => Self::None,
            Self::Options => Self::GameMenu,
            Self::Debug => Self::Debug,
        }
    }

    /// The screen after a fresh F2 press: the debug screen opens over any
    /// other screen and a second press closes it.
    #[must_use]
    pub const fn after_debug_toggle(self) -> Self {
        match self {
            Self::Debug => Self::None,
            _ => Self::Debug,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Screen;

    #[test]
    fn only_the_empty_screen_lets_the_game_read_input() {
        assert!(!Screen::None.blocks_game_input());
        for screen in [
            Screen::Chat,
            Screen::Inventory,
            Screen::GameMenu,
            Screen::Options,
            Screen::Debug,
        ] {
            assert!(screen.blocks_game_input(), "{screen:?} must own the input");
        }
    }

    #[test]
    fn the_chat_does_not_use_the_ui_but_the_other_screens_do() {
        assert!(!Screen::None.uses_ui());
        assert!(!Screen::Chat.uses_ui());
        assert!(Screen::Inventory.uses_ui());
        assert!(Screen::GameMenu.uses_ui());
        assert!(Screen::Options.uses_ui());
        assert!(Screen::Debug.uses_ui());
    }

    #[test]
    fn escape_opens_the_menu_and_closes_every_other_screen_but_debug() {
        assert_eq!(Screen::None.after_escape(), Screen::GameMenu);
        assert_eq!(Screen::GameMenu.after_escape(), Screen::None);
        assert_eq!(Screen::Chat.after_escape(), Screen::None);
        assert_eq!(Screen::Inventory.after_escape(), Screen::None);
        assert_eq!(Screen::Options.after_escape(), Screen::GameMenu);
        assert_eq!(Screen::Debug.after_escape(), Screen::Debug);
    }

    #[test]
    fn f2_toggles_the_debug_screen_over_any_screen() {
        assert_eq!(Screen::None.after_debug_toggle(), Screen::Debug);
        assert_eq!(Screen::Chat.after_debug_toggle(), Screen::Debug);
        assert_eq!(Screen::Debug.after_debug_toggle(), Screen::None);
    }
}
