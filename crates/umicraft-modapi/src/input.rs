// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attack and use input (API 1.5): what [`PlayerInput`] tells and how
//! [`Controller`] presses the bindings and turns the look.
//!
//! Everything here needs `input = true` in `mod.toml`; without it the host
//! answers `DENIED`.

use umicraft_modapi_sys::{
    UMI_INPUT_ACTION_ATTACK, UMI_INPUT_ACTION_USE, UMI_POINTED_BLOCK, UMI_POINTED_ENTITY,
    UMI_USE_ACTION_BLOCK, UMI_USE_ACTION_BOW, UMI_USE_ACTION_DRINK, UMI_USE_ACTION_EAT,
};

use crate::{host, ModError, PlayerInput};

/// Most presses one [`Controller::press`] call makes.
pub const MAX_PRESSES: u8 = 8;

/// A binding [`Controller`] can press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// The attack binding (left button by default).
    Attack,
    /// The use binding (right button by default).
    Use,
}

impl InputAction {
    fn wire(self) -> u32 {
        match self {
            Self::Attack => UMI_INPUT_ACTION_ATTACK,
            Self::Use => UMI_INPUT_ACTION_USE,
        }
    }
}

/// What using an item does (MCP `EnumAction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseAction {
    /// Nothing lasting: used at once, or not at all.
    None,
    /// Eaten.
    Eat,
    /// Drunk.
    Drink,
    /// Held up to block (a sword).
    Block,
    /// Drawn (a bow).
    Bow,
}

/// What the crosshair points at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointed {
    /// Nothing in reach.
    Miss,
    /// A block.
    Block {
        /// The block's position.
        position: [i32; 3],
        /// The face the ray enters: 0 down, 1 up, 2 north, 3 south, 4 west,
        /// 5 east.
        face: u8,
        /// The block as `id << 4 | metadata`, as [`World::block`](crate::World::block)
        /// reads it.
        state: u32,
        /// Where the ray hits, in blocks.
        point: [f64; 3],
        /// From the eye to `point`, in blocks.
        distance: f32,
    },
    /// An entity.
    Entity {
        /// Its server id, as [`Entity::id`](crate::Entity::id) gives it.
        id: i32,
        /// Where the ray enters its box, in blocks.
        point: [f64; 3],
        /// From the eye to `point`, in blocks.
        distance: f32,
    },
}

impl PlayerInput {
    /// Monotonic time in milliseconds, the clock of [`Frame`](crate::Frame).
    pub fn now_ms(&self) -> u64 {
        self.0.now_ms
    }

    /// Game ticks since the session started, at the tick the hand facts
    /// come from.
    pub fn tick(&self) -> u64 {
        self.0.tick
    }

    /// Whether the attack binding is held, whatever key or button it uses.
    pub fn attack_held(&self) -> bool {
        self.0.attack_held != 0
    }

    /// Whether the use binding is held.
    pub fn use_held(&self) -> bool {
        self.0.use_held != 0
    }

    /// Attack presses not sent to the game yet, the player's and the mods'.
    pub fn attack_presses(&self) -> u8 {
        self.0.attack_presses
    }

    /// Use presses not sent to the game yet.
    pub fn use_presses(&self) -> u8 {
        self.0.use_presses
    }

    /// Whether the game owns the input: no screen open, mouse captured, in a
    /// world. [`Controller`] answers [`ModError::Unavailable`] otherwise.
    pub fn game_focus(&self) -> bool {
        self.0.game_focus != 0
    }

    /// Whether an item is in use (eating, drinking, blocking, drawing a bow).
    pub fn using_item(&self) -> bool {
        self.0.using_item != 0
    }

    /// The arm swing, 0 to 1; 0 when the arm is still.
    pub fn swing_progress(&self) -> f32 {
        self.0.swing_progress
    }

    /// The held item as `(id, damage)`, `None` for an empty hand.
    pub fn held_item(&self) -> Option<(i32, i32)> {
        (self.0.held_item >= 0).then_some((self.0.held_item, self.0.held_damage))
    }

    /// What using the held item does.
    pub fn held_use_action(&self) -> UseAction {
        match self.0.held_use_action {
            UMI_USE_ACTION_EAT => UseAction::Eat,
            UMI_USE_ACTION_DRINK => UseAction::Drink,
            UMI_USE_ACTION_BLOCK => UseAction::Block,
            UMI_USE_ACTION_BOW => UseAction::Bow,
            _ => UseAction::None,
        }
    }

    /// Whether the held item places a block.
    pub fn held_is_block(&self) -> bool {
        self.0.held_is_block != 0
    }

    /// What the crosshair pointed at during the last tick.
    pub fn pointed(&self) -> Pointed {
        let raw = &self.0;
        match raw.pointed_kind {
            UMI_POINTED_BLOCK => Pointed::Block {
                position: raw.pointed_block_pos,
                face: raw.pointed_face,
                state: raw.pointed_block,
                point: raw.pointed_point,
                distance: raw.pointed_distance,
            },
            UMI_POINTED_ENTITY => Pointed::Entity {
                id: raw.pointed_entity,
                point: raw.pointed_point,
                distance: raw.pointed_distance,
            },
            _ => Pointed::Miss,
        }
    }
}

/// Presses the attack and use bindings and turns the player's look, through the
/// client's own input path: the same rules as the player's clicks apply, and
/// the server sees what a player could send.
///
/// Cheap to copy and usable from any thread. Get one with
/// [`InitCx::input`](crate::InitCx::input); needs `input = true` in `mod.toml`.
///
/// # Errors
///
/// Every call fails with:
/// - [`ModError::HostRefused`] with `DENIED` without the permission or while the
///   mod is disabled (`UNSUPPORTED` before `init`), with `INVALID_ARGUMENT` for a
///   bad argument;
/// - [`ModError::Unavailable`] while the game does not own the input (a screen is
///   open, the mouse is free, no world);
/// - [`ModError::Busy`] when the host's queue is full; retry later.
#[derive(Debug, Clone, Copy)]
pub struct Controller;

impl Controller {
    /// Presses `action` `count` times (1 to [`MAX_PRESSES`]), as the player
    /// clicking: the next game tick clicks once per press.
    ///
    /// ```ignore
    /// cx.input().press(InputAction::Use, 1)?;
    /// ```
    pub fn press(&self, action: InputAction, count: u8) -> Result<(), ModError> {
        host::call(|host, me| (host.input_press)(me, action.wire(), u32::from(count)))
            .and_then(ModError::check)
    }

    /// Drops the presses of `action` not sent to the game yet, the player's
    /// included, without a release.
    pub fn clear_press(&self, action: InputAction) -> Result<(), ModError> {
        host::call(|host, me| (host.input_clear_press)(me, action.wire())).and_then(ModError::check)
    }

    /// Turns the player's look by `yaw` and `pitch` degrees (positive turns
    /// right and looks down), each within ±180. The pitch stays within ±90 as
    /// with the mouse; the server sees the new look in the next movement
    /// packet.
    pub fn add_look(&self, yaw: f32, pitch: f32) -> Result<(), ModError> {
        host::call(|host, me| (host.input_add_look)(me, yaw, pitch)).and_then(ModError::check)
    }
}
