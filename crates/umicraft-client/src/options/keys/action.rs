// SPDX-License-Identifier: MIT OR Apache-2.0

//! The actions a key can be bound to.

use winit::keyboard::KeyCode;

/// What a key can be bound to. Only the actions the client acts on are
/// listed; the function keys (`F1` to `F4`) and `Escape` are fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Walk forward.
    Forward,
    /// Walk backwards.
    Back,
    /// Strafe left.
    Left,
    /// Strafe right.
    Right,
    /// Jump.
    Jump,
    /// Sneak.
    Sneak,
    /// Sprint.
    Sprint,
    /// Open the inventory.
    Inventory,
    /// Open the chat.
    Chat,
    /// Open the chat with a slash already typed.
    Command,
    /// Show the tab list while held.
    PlayerList,
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
    /// Every action, in the order the controls screen lists them.
    pub const ALL: [Self; 20] = [
        Self::Forward,
        Self::Left,
        Self::Back,
        Self::Right,
        Self::Jump,
        Self::Sneak,
        Self::Sprint,
        Self::Inventory,
        Self::Chat,
        Self::Command,
        Self::PlayerList,
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
            Self::Forward => "forward",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
            Self::Jump => "jump",
            Self::Sneak => "sneak",
            Self::Sprint => "sprint",
            Self::Inventory => "inventory",
            Self::Chat => "chat",
            Self::Command => "command",
            Self::PlayerList => "player_list",
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

    /// The name shown on the controls screen.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Forward => "FORWARD",
            Self::Back => "BACK",
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
            Self::Jump => "JUMP",
            Self::Sneak => "SNEAK",
            Self::Sprint => "SPRINT",
            Self::Inventory => "INVENTORY",
            Self::Chat => "CHAT",
            Self::Command => "COMMAND",
            Self::PlayerList => "LIST PLAYERS",
            Self::Hotbar1 => "HOTBAR 1",
            Self::Hotbar2 => "HOTBAR 2",
            Self::Hotbar3 => "HOTBAR 3",
            Self::Hotbar4 => "HOTBAR 4",
            Self::Hotbar5 => "HOTBAR 5",
            Self::Hotbar6 => "HOTBAR 6",
            Self::Hotbar7 => "HOTBAR 7",
            Self::Hotbar8 => "HOTBAR 8",
            Self::Hotbar9 => "HOTBAR 9",
        }
    }

    /// The default key (`GameSettings`: W A S D, space, left shift, left
    /// control, E, T, `/`, Tab and the digits).
    #[must_use]
    pub const fn default_key(self) -> KeyCode {
        match self {
            Self::Forward => KeyCode::KeyW,
            Self::Back => KeyCode::KeyS,
            Self::Left => KeyCode::KeyA,
            Self::Right => KeyCode::KeyD,
            Self::Jump => KeyCode::Space,
            Self::Sneak => KeyCode::ShiftLeft,
            Self::Sprint => KeyCode::ControlLeft,
            Self::Inventory => KeyCode::KeyE,
            Self::Chat => KeyCode::KeyT,
            Self::Command => KeyCode::Slash,
            Self::PlayerList => KeyCode::Tab,
            Self::Hotbar1 => KeyCode::Digit1,
            Self::Hotbar2 => KeyCode::Digit2,
            Self::Hotbar3 => KeyCode::Digit3,
            Self::Hotbar4 => KeyCode::Digit4,
            Self::Hotbar5 => KeyCode::Digit5,
            Self::Hotbar6 => KeyCode::Digit6,
            Self::Hotbar7 => KeyCode::Digit7,
            Self::Hotbar8 => KeyCode::Digit8,
            Self::Hotbar9 => KeyCode::Digit9,
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
