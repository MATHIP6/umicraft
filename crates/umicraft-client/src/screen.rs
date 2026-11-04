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
//!
//! Since P10 (ADR-0044) the same type also names the menus that surround a
//! session: the title screen, the server list and its dialogs, the connecting
//! and disconnected screens and the statistics. They are drawn by
//! `menu_flow`; this module only decides who owns the input and where Escape
//! leads.

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
    /// The creative inventory, shown in creative mode instead of the survival one.
    Creative,
    /// The menu opened with Escape.
    GameMenu,
    /// The options, opened from the game menu.
    Options,
    /// The sign editor.
    Sign,
    /// A book and quill or a written book.
    Book,
    /// The Umicraft debug screen (F4; ADR-0036, F3 is the vanilla text screen).
    Debug,
    /// The title screen the client starts on when no server is given.
    Title,
    /// The multiplayer server list.
    ServerList,
    /// The add, edit or direct-connect form of the server list.
    ServerEdit,
    /// The "really delete this server" question.
    ConfirmDelete,
    /// The session is starting; no world is shown yet.
    Connecting,
    /// The session ended; the reason is shown with a way back to the list.
    Disconnected,
    /// The statistics, opened from the game menu.
    Stats,
}

impl Screen {
    /// Whether the immediate-mode UI receives the pointer and the keys.
    #[must_use]
    pub const fn uses_ui(self) -> bool {
        matches!(
            self,
            Self::Inventory
                | Self::Creative
                | Self::GameMenu
                | Self::Options
                | Self::Sign
                | Self::Book
                | Self::Debug
                | Self::Stats
        ) || self.is_menu()
    }

    /// Whether this is a full-screen menu that replaces the world view: the
    /// title screen, the server list with its dialogs, connecting and
    /// disconnected. They draw the dirt or panorama background themselves.
    #[must_use]
    pub const fn is_menu(self) -> bool {
        matches!(
            self,
            Self::Title
                | Self::ServerList
                | Self::ServerEdit
                | Self::ConfirmDelete
                | Self::Connecting
                | Self::Disconnected
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
    /// the menu close; the options go back to `options_origin`, the screen they
    /// were opened from ([`Screen::Title`] or [`Screen::GameMenu`]). The
    /// server list leads to the title screen, its dialogs and the
    /// disconnected screen to the list, cancelling a connection to the list
    /// and the statistics to the game menu. The title screen stays. The debug
    /// screen is closed by F4 only, because Escape is a key its widgets may
    /// consume.
    #[must_use]
    pub const fn after_escape(self, options_origin: Self) -> Self {
        match self {
            Self::None => Self::GameMenu,
            Self::Chat
            | Self::Inventory
            | Self::Creative
            | Self::GameMenu
            | Self::Sign
            | Self::Book => Self::None,
            Self::Options => options_origin,
            Self::Debug => Self::Debug,
            Self::Title => Self::Title,
            Self::ServerList => Self::Title,
            Self::ServerEdit | Self::ConfirmDelete | Self::Connecting | Self::Disconnected => {
                Self::ServerList
            }
            Self::Stats => Self::GameMenu,
        }
    }

    /// The screen after a fresh F4 press: the debug screen opens over any
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
            Screen::Creative,
            Screen::GameMenu,
            Screen::Options,
            Screen::Sign,
            Screen::Book,
            Screen::Debug,
            Screen::Title,
            Screen::ServerList,
            Screen::ServerEdit,
            Screen::ConfirmDelete,
            Screen::Connecting,
            Screen::Disconnected,
            Screen::Stats,
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
        assert_eq!(
            Screen::None.after_escape(Screen::GameMenu),
            Screen::GameMenu
        );
        assert_eq!(
            Screen::GameMenu.after_escape(Screen::GameMenu),
            Screen::None
        );
        assert_eq!(Screen::Chat.after_escape(Screen::GameMenu), Screen::None);
        assert_eq!(
            Screen::Inventory.after_escape(Screen::GameMenu),
            Screen::None
        );
        assert_eq!(
            Screen::Options.after_escape(Screen::GameMenu),
            Screen::GameMenu
        );
        assert_eq!(Screen::Debug.after_escape(Screen::GameMenu), Screen::Debug);
    }

    #[test]
    fn f4_toggles_the_debug_screen_over_any_screen() {
        assert_eq!(Screen::None.after_debug_toggle(), Screen::Debug);
        assert_eq!(Screen::Chat.after_debug_toggle(), Screen::Debug);
        assert_eq!(Screen::Debug.after_debug_toggle(), Screen::None);
    }

    #[test]
    fn every_menu_screen_uses_the_ui() {
        for screen in [
            Screen::Title,
            Screen::ServerList,
            Screen::ServerEdit,
            Screen::ConfirmDelete,
            Screen::Connecting,
            Screen::Disconnected,
            Screen::Stats,
        ] {
            assert!(screen.uses_ui(), "{screen:?} is drawn by the UI");
        }
        assert!(Screen::Title.is_menu());
        assert!(!Screen::Stats.is_menu(), "the statistics open over a game");
        assert!(!Screen::GameMenu.is_menu());
    }

    #[test]
    fn escape_walks_back_through_the_menus() {
        let origin = Screen::Title;
        assert_eq!(Screen::ServerList.after_escape(origin), Screen::Title);
        assert_eq!(Screen::ServerEdit.after_escape(origin), Screen::ServerList);
        assert_eq!(
            Screen::ConfirmDelete.after_escape(origin),
            Screen::ServerList
        );
        assert_eq!(Screen::Connecting.after_escape(origin), Screen::ServerList);
        assert_eq!(
            Screen::Disconnected.after_escape(origin),
            Screen::ServerList
        );
        assert_eq!(Screen::Stats.after_escape(origin), Screen::GameMenu);
        assert_eq!(Screen::Title.after_escape(origin), Screen::Title);
    }

    #[test]
    fn the_options_return_to_the_screen_that_opened_them() {
        assert_eq!(Screen::Options.after_escape(Screen::Title), Screen::Title);
        assert_eq!(
            Screen::Options.after_escape(Screen::GameMenu),
            Screen::GameMenu
        );
    }
}
