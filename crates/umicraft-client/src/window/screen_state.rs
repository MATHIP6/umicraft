// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which screen is open, and what it lets through to the world.

use super::App;
use crate::anvil_field::AnvilField;
use crate::screen::Screen;

impl App {
    /// The screen that owns the input right now.
    ///
    /// A container the server opened is shown by the inventory screen even
    /// though the player did not press the inventory key, so it is derived
    /// from the world view when no screen is set.
    pub(super) fn screen(&self) -> Screen {
        if self.screen == Screen::None && self.container_is_open() {
            Screen::Inventory
        } else {
            self.screen
        }
    }

    /// Switches to `screen`.
    ///
    /// Every transition drops the pending input so no held key, look delta or
    /// click leaks across screens (a key released while another screen owned
    /// the keyboard would otherwise stay "held"). A screen that blocks the
    /// game also releases the cursor grab. The game and network threads keep
    /// running; the screens only read the render-side snapshot.
    pub(super) fn set_screen(&mut self, screen: Screen) {
        if screen == self.screen {
            return;
        }
        let was_blocking = self.screen.blocks_game_input();
        self.screen = screen;
        self.menu.enter(screen);
        self.ui_input.clear();
        self.chat.scroll = 0;
        self.input.keys.clear();
        self.input.buttons.clear();
        self.presses.select_slot = None;
        self.presses.break_block = false;
        self.presses.place_block = false;
        self.presses.pick_block = false;
        self.presses.drop_item = None;
        self.input.mouse.clear();
        if screen.blocks_game_input() {
            self.capture_mouse(false);
        } else if was_blocking && !self.container_is_open() && !self.death_screen_visible() {
            // MCP `displayGuiScreen(null)` grabs the mouse again, so closing
            // the chat hides the cursor without a click.
            self.capture_mouse(true);
        }
    }

    /// Whether a game is shown: a session runs and no full-screen menu
    /// (connecting, disconnected) covers it.
    pub(super) fn world_visible(&self) -> bool {
        self.session.is_some() && !self.screen().is_menu()
    }

    /// Whether a server-opened container is currently displayed.
    pub(super) fn container_is_open(&self) -> bool {
        self.world
            .as_ref()
            .is_some_and(|w| w.window_view().container.is_some())
    }

    /// Frees the mouse while a server container is displayed and takes it
    /// back when it closes (MCP `displayGuiScreen` ungrabs / grabs the mouse).
    pub(super) fn update_container_cursor(&mut self) {
        let open = self.container_is_open();
        if open != self.container_was_open {
            self.container_was_open = open;
            self.ui_input.clear();
            self.inventory_input.reset();
            self.anvil_field = AnvilField::default();
            let free = open || self.screen().blocks_game_input() || self.death_screen_visible();
            self.capture_mouse(!free);
        }
    }
}
