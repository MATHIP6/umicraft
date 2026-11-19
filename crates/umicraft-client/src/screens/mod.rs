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

pub(crate) mod achievements;
pub(crate) mod anvil_field;
pub(crate) mod connection_end;
pub(crate) mod creative;
pub(crate) mod effect_list;
pub(crate) mod inventory;
pub(crate) mod menu_flow;
pub(crate) mod menu_rows;
pub(crate) mod options;
pub(crate) mod portrait;
pub(crate) mod splash;
pub(crate) mod stats;
pub(crate) mod text;
pub(crate) mod tooltip;

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
    /// The question whether to install the resource pack a server offers.
    PackPrompt,
    /// The sign editor.
    Sign,
    /// A book and quill or a written book.
    Book,
    /// The Umicraft debug screen (F4; ADR-0036, F3 is the vanilla text screen).
    Debug,
    /// The title screen the client starts on when no server is given.
    Title,
    /// The loading / splash screen shown while heavy assets load asynchronously.
    Loading,
    /// The multiplayer server list.
    ServerList,
    /// The add, edit or direct-connect form of the server list.
    ServerEdit,
    /// The "really delete this server" question.
    ConfirmDelete,
    /// The account chooser, opened from the server list (ROADMAP F1).
    Accounts,
    /// The session is starting; no world is shown yet.
    Connecting,
    /// The session ended; the reason is shown with a way back to the list.
    Disconnected,
    /// The statistics, opened from the game menu.
    Stats,
    /// The achievements, opened from the game menu.
    Achievements,
    /// The mods menu, opened over a game with the right Shift key. The HUD
    /// widgets are moved on it too.
    Mods,
}

impl Screen {
    /// The stable snake_case name mods see in the screen events (ADR-0086).
    ///
    /// Part of the modding API: a name is never changed or reused once released.
    /// [`Screen::None`] is "no screen" (the game has the input), so it has no
    /// opened or closed event; its name only keeps this total.
    #[must_use]
    pub const fn mod_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Chat => "chat",
            Self::Inventory => "inventory",
            Self::Creative => "creative_inventory",
            Self::GameMenu => "game_menu",
            Self::Options => "options",
            Self::PackPrompt => "pack_prompt",
            Self::Sign => "sign",
            Self::Book => "book",
            Self::Debug => "debug",
            Self::Title => "title",
            Self::Loading => "loading",
            Self::ServerList => "server_list",
            Self::ServerEdit => "server_edit",
            Self::ConfirmDelete => "confirm_delete",
            Self::Accounts => "accounts",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Stats => "stats",
            Self::Achievements => "achievements",
            Self::Mods => "mods",
        }
    }

    /// Whether the immediate-mode UI receives the pointer and the keys.
    #[must_use]
    pub const fn uses_ui(self) -> bool {
        matches!(
            self,
            Self::Inventory
                | Self::Creative
                | Self::GameMenu
                | Self::Options
                | Self::PackPrompt
                | Self::Sign
                | Self::Book
                | Self::Debug
                | Self::Stats
                | Self::Achievements
                | Self::Mods
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
                | Self::Accounts
                | Self::Connecting
                | Self::Disconnected
                | Self::Loading
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
            | Self::PackPrompt
            | Self::Sign
            | Self::Book
            | Self::Mods => Self::None,
            Self::Options => options_origin,
            Self::Debug => Self::Debug,
            Self::Loading => Self::Loading,
            Self::Title => Self::Title,
            Self::ServerList => Self::Title,
            Self::ServerEdit
            | Self::ConfirmDelete
            | Self::Accounts
            | Self::Connecting
            | Self::Disconnected => Self::ServerList,
            Self::Stats | Self::Achievements => Self::GameMenu,
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
    fn mod_names_are_unique_snake_case() {
        let all = [
            Screen::None,
            Screen::Chat,
            Screen::Inventory,
            Screen::Creative,
            Screen::GameMenu,
            Screen::Options,
            Screen::PackPrompt,
            Screen::Sign,
            Screen::Book,
            Screen::Debug,
            Screen::Title,
            Screen::Loading,
            Screen::ServerList,
            Screen::ServerEdit,
            Screen::ConfirmDelete,
            Screen::Accounts,
            Screen::Connecting,
            Screen::Disconnected,
            Screen::Stats,
            Screen::Achievements,
            Screen::Mods,
        ];
        let names: std::collections::HashSet<_> = all.iter().map(|s| s.mod_name()).collect();
        assert_eq!(names.len(), all.len(), "every screen has its own name");
        for name in names {
            assert!(
                name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{name} is snake_case"
            );
        }
    }

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
            Screen::Achievements,
            Screen::Mods,
            Screen::Loading,
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
    fn the_mods_menu_opens_over_the_game_and_escape_closes_it() {
        assert!(Screen::Mods.uses_ui());
        assert!(!Screen::Mods.is_menu(), "it opens over a game");
        assert_eq!(Screen::Mods.after_escape(Screen::GameMenu), Screen::None);
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
            Screen::Achievements,
            Screen::Loading,
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
        assert_eq!(Screen::Achievements.after_escape(origin), Screen::GameMenu);
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
