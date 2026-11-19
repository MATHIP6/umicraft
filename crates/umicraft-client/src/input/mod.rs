// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's input: the key state and the update that crosses into the
//! game thread.
//!
//! The window and the headless [`Script`](crate::headless::script::Script)
//! are two *input sources*: both produce
//! [`InputUpdate`] messages and the game thread cannot tell them apart. The
//! interactive client and the acceptance session therefore drive the exact
//! same input path (spec §11 wants the headless mode to run the real
//! integration suite, not a separate simulation).

use std::time::Instant;

use umicraft_game::item_use::DropKind;

pub(crate) mod clipboard;
pub mod mouse_look;
pub mod ui;

/// The movement key state, sampled by an input source.
///
/// The axes are the analogue values of the movement keys. The sign of the
/// strafe axis follows MCP `net/minecraft/util/MovementInput.java`, where a
/// positive number means "strafe left".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputState {
    /// The forward axis: `-1.0` is backwards, `1.0` forwards.
    pub forward: f32,
    /// The strafe axis: `-1.0` is to the right, `1.0` to the left.
    pub strafe: f32,
    /// Whether the jump key is held.
    pub jump: bool,
    /// Whether the sneak key is held.
    pub sneak: bool,
    /// Whether the sprint key is held.
    pub sprint: bool,
    /// Whether the attack / break button is held.
    pub attack: bool,
    /// Whether the use (right) button is held.
    pub use_item: bool,
}

impl InputState {
    /// A state that only holds the sprint key.
    ///
    /// The sprint key is the "sprint while moving" modifier; the game logic
    /// still needs a forward input above the vanilla threshold before the
    /// sprint starts (MCP `EntityPlayerSP.java:784-785`).
    #[must_use]
    pub const fn sprinting() -> Self {
        Self {
            forward: 0.0,
            strafe: 0.0,
            jump: false,
            sneak: false,
            sprint: true,
            attack: false,
            use_item: false,
        }
    }
}

/// One input update sent from an input source to the game thread.
///
/// The message is plain data: the game thread owns the player state and
/// applies the update to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputUpdate {
    /// The instant this input update was created/sampled at the front end.
    pub created_at: Instant,
    /// The new key state; it replaces the previous one.
    pub state: InputState,
    /// The horizontal mouse turn, the first argument of `Entity.setAngles`
    /// (whole pixels times the sensitivity factor, see
    /// [`crate::input::mouse_look::look_turn`]); positive turns to the right.
    pub look_yaw: f32,
    /// The vertical mouse turn; positive looks down.
    pub look_pitch: f32,
    /// The hotbar slot the player selected, if any (the mouse wheel, which
    /// `Minecraft.runTick` handles before the keys).
    pub select_slot: Option<u8>,
    /// The presses of each digit key since the last update (MCP
    /// `KeyBinding.pressTime`): the game takes them one per key and tick, in
    /// the order of the keys, not of the presses.
    pub hotbar_keys: [u8; 9],
    /// Whether the player started breaking the targeted block.
    pub break_block: bool,
    /// Whether the use (right) button went down since the last update.
    pub place_block: bool,
    /// Whether the pick block (middle) button went down.
    pub pick_block: bool,
    /// The drop the player asked for with the drop key, if any.
    pub drop_item: Option<DropKind>,
    /// Whether the player pressed the perspective key (`F5`).
    pub toggle_perspective: bool,
}

impl Default for InputUpdate {
    fn default() -> Self {
        Self {
            created_at: Instant::now(),
            state: InputState::default(),
            look_yaw: 0.0,
            look_pitch: 0.0,
            select_slot: None,
            hotbar_keys: [0; 9],
            break_block: false,
            place_block: false,
            pick_block: false,
            drop_item: None,
            toggle_perspective: false,
        }
    }
}
