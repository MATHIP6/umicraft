// SPDX-License-Identifier: MIT OR Apache-2.0

//! The actions a key or mouse button can be bound to.
//!
//! The list is the `GameSettings.keyBindings` array of the reference minus
//! the entries without a backend here (smooth camera, spectator outlines and
//! the four streaming keys, see `docs/parity.md`).

use winit::keyboard::KeyCode;

use super::Binding;

/// What a key can be bound to. `F1`, `F3`, `F4` and `Escape` are fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Attack or break the block (vanilla `key.attack`).
    Attack,
    /// Use the held item or place a block (`key.use`).
    Use,
    /// Walk forward.
    Forward,
    /// Strafe left.
    Left,
    /// Walk backwards.
    Back,
    /// Strafe right.
    Right,
    /// Jump.
    Jump,
    /// Sneak.
    Sneak,
    /// Sprint.
    Sprint,
    /// Drop the held item (the whole stack with Ctrl).
    Drop,
    /// Open the inventory.
    Inventory,
    /// Open the chat.
    Chat,
    /// Show the tab list while held.
    PlayerList,
    /// Pick the targeted block (`key.pickItem`).
    PickBlock,
    /// Open the chat with a slash already typed.
    Command,
    /// Save a screenshot.
    Screenshot,
    /// Cycle the camera perspective.
    TogglePerspective,
    /// Toggle fullscreen.
    Fullscreen,
    /// Select hotbar slot 1.
    Hotbar1,
    /// Select hotbar slot 2.
    Hotbar2,
    /// Select hotbar slot 3.
    Hotbar3,
    /// Select hotbar slot 4.
    Hotbar4,
    /// Select hotbar slot 5.
    Hotbar5,
    /// Select hotbar slot 6.
    Hotbar6,
    /// Select hotbar slot 7.
    Hotbar7,
    /// Select hotbar slot 8.
    Hotbar8,
    /// Select hotbar slot 9.
    Hotbar9,
}

impl Action {
    /// Every action, in the order of `GameSettings.keyBindings`.
    pub const ALL: [Self; 27] = [
        Self::Attack,
        Self::Use,
        Self::Forward,
        Self::Left,
        Self::Back,
        Self::Right,
        Self::Jump,
        Self::Sneak,
        Self::Sprint,
        Self::Drop,
        Self::Inventory,
        Self::Chat,
        Self::PlayerList,
        Self::PickBlock,
        Self::Command,
        Self::Screenshot,
        Self::TogglePerspective,
        Self::Fullscreen,
        Self::Hotbar1,
        Self::Hotbar2,
        Self::Hotbar3,
        Self::Hotbar4,
        Self::Hotbar5,
        Self::Hotbar6,
        Self::Hotbar7,
        Self::Hotbar8,
        Self::Hotbar9,
    ];

    /// The key of the action in `options.toml`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Attack => "attack",
            Self::Use => "use",
            Self::Forward => "forward",
            Self::Left => "left",
            Self::Back => "back",
            Self::Right => "right",
            Self::Jump => "jump",
            Self::Sneak => "sneak",
            Self::Sprint => "sprint",
            Self::Drop => "drop",
            Self::Inventory => "inventory",
            Self::Chat => "chat",
            Self::PlayerList => "player_list",
            Self::PickBlock => "pick_block",
            Self::Command => "command",
            Self::Screenshot => "screenshot",
            Self::TogglePerspective => "perspective",
            Self::Fullscreen => "fullscreen",
            Self::Hotbar1 => "hotbar1",
            Self::Hotbar2 => "hotbar2",
            Self::Hotbar3 => "hotbar3",
            Self::Hotbar4 => "hotbar4",
            Self::Hotbar5 => "hotbar5",
            Self::Hotbar6 => "hotbar6",
            Self::Hotbar7 => "hotbar7",
            Self::Hotbar8 => "hotbar8",
            Self::Hotbar9 => "hotbar9",
        }
    }

    /// The language key of the action's name (`KeyBinding` description).
    #[must_use]
    pub const fn lang_key(self) -> &'static str {
        match self {
            Self::Attack => "key.attack",
            Self::Use => "key.use",
            Self::Forward => "key.forward",
            Self::Left => "key.left",
            Self::Back => "key.back",
            Self::Right => "key.right",
            Self::Jump => "key.jump",
            Self::Sneak => "key.sneak",
            Self::Sprint => "key.sprint",
            Self::Drop => "key.drop",
            Self::Inventory => "key.inventory",
            Self::Chat => "key.chat",
            Self::PlayerList => "key.playerlist",
            Self::PickBlock => "key.pickItem",
            Self::Command => "key.command",
            Self::Screenshot => "key.screenshot",
            Self::TogglePerspective => "key.togglePerspective",
            Self::Fullscreen => "key.fullscreen",
            Self::Hotbar1 => "key.hotbar.1",
            Self::Hotbar2 => "key.hotbar.2",
            Self::Hotbar3 => "key.hotbar.3",
            Self::Hotbar4 => "key.hotbar.4",
            Self::Hotbar5 => "key.hotbar.5",
            Self::Hotbar6 => "key.hotbar.6",
            Self::Hotbar7 => "key.hotbar.7",
            Self::Hotbar8 => "key.hotbar.8",
            Self::Hotbar9 => "key.hotbar.9",
        }
    }

    /// The language key of the category heading on the controls page.
    #[must_use]
    pub const fn category(self) -> &'static str {
        match self {
            Self::Attack => "key.categories.gameplay",
            Self::Use => "key.categories.gameplay",
            Self::Forward => "key.categories.movement",
            Self::Left => "key.categories.movement",
            Self::Back => "key.categories.movement",
            Self::Right => "key.categories.movement",
            Self::Jump => "key.categories.movement",
            Self::Sneak => "key.categories.movement",
            Self::Sprint => "key.categories.movement",
            Self::Drop => "key.categories.gameplay",
            Self::Inventory => "key.categories.inventory",
            Self::Chat => "key.categories.multiplayer",
            Self::PlayerList => "key.categories.multiplayer",
            Self::PickBlock => "key.categories.gameplay",
            Self::Command => "key.categories.multiplayer",
            Self::Screenshot => "key.categories.misc",
            Self::TogglePerspective => "key.categories.misc",
            Self::Fullscreen => "key.categories.misc",
            Self::Hotbar1 => "key.categories.inventory",
            Self::Hotbar2 => "key.categories.inventory",
            Self::Hotbar3 => "key.categories.inventory",
            Self::Hotbar4 => "key.categories.inventory",
            Self::Hotbar5 => "key.categories.inventory",
            Self::Hotbar6 => "key.categories.inventory",
            Self::Hotbar7 => "key.categories.inventory",
            Self::Hotbar8 => "key.categories.inventory",
            Self::Hotbar9 => "key.categories.inventory",
        }
    }

    /// The default binding (`GameSettings`: mouse buttons 1 to 3 for attack,
    /// use and pick, W A S D, space, left shift, left control, Q, E, T, `/`,
    /// Tab, F2, F5, F11 and the digits).
    #[must_use]
    pub const fn default_binding(self) -> Binding {
        match self {
            Self::Attack => Binding::Mouse(0),
            Self::Use => Binding::Mouse(1),
            Self::Forward => Binding::Key(KeyCode::KeyW),
            Self::Left => Binding::Key(KeyCode::KeyA),
            Self::Back => Binding::Key(KeyCode::KeyS),
            Self::Right => Binding::Key(KeyCode::KeyD),
            Self::Jump => Binding::Key(KeyCode::Space),
            Self::Sneak => Binding::Key(KeyCode::ShiftLeft),
            Self::Sprint => Binding::Key(KeyCode::ControlLeft),
            Self::Drop => Binding::Key(KeyCode::KeyQ),
            Self::Inventory => Binding::Key(KeyCode::KeyE),
            Self::Chat => Binding::Key(KeyCode::KeyT),
            Self::PlayerList => Binding::Key(KeyCode::Tab),
            Self::PickBlock => Binding::Mouse(2),
            Self::Command => Binding::Key(KeyCode::Slash),
            Self::Screenshot => Binding::Key(KeyCode::F2),
            Self::TogglePerspective => Binding::Key(KeyCode::F5),
            Self::Fullscreen => Binding::Key(KeyCode::F11),
            Self::Hotbar1 => Binding::Key(KeyCode::Digit1),
            Self::Hotbar2 => Binding::Key(KeyCode::Digit2),
            Self::Hotbar3 => Binding::Key(KeyCode::Digit3),
            Self::Hotbar4 => Binding::Key(KeyCode::Digit4),
            Self::Hotbar5 => Binding::Key(KeyCode::Digit5),
            Self::Hotbar6 => Binding::Key(KeyCode::Digit6),
            Self::Hotbar7 => Binding::Key(KeyCode::Digit7),
            Self::Hotbar8 => Binding::Key(KeyCode::Digit8),
            Self::Hotbar9 => Binding::Key(KeyCode::Digit9),
        }
    }

    /// The hotbar slot of a hotbar action, `0..=8`.
    #[must_use]
    pub const fn hotbar_slot(self) -> Option<u8> {
        match self {
            Self::Hotbar1 => Some(0),
            Self::Hotbar2 => Some(1),
            Self::Hotbar3 => Some(2),
            Self::Hotbar4 => Some(3),
            Self::Hotbar5 => Some(4),
            Self::Hotbar6 => Some(5),
            Self::Hotbar7 => Some(6),
            Self::Hotbar8 => Some(7),
            Self::Hotbar9 => Some(8),
            _ => None,
        }
    }

    pub(super) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0)
    }
}
