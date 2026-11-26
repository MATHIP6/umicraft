// SPDX-License-Identifier: MIT OR Apache-2.0

//! Synthetic input: the policy behind the `input_*` host functions and the queue the
//! window drains before each input update (API 1.5, ADR-0098).
//!
//! A mod never touches the player state. It queues an [`InputRequest`]; the window
//! applies it to the presses and the look it sends the game thread anyway, so an
//! injected click takes the very path of a real one.

use crossbeam_channel::Receiver;
use umicraft_modapi_sys::{
    UmiStatus, UMI_INPUT_ACTION_ATTACK, UMI_INPUT_ACTION_USE, UMI_MAX_INPUT_PRESSES,
};

use crate::context::ModContext;
use crate::hook::warn_sparse;

/// Requests that may wait for the window before the input functions answer `BUSY`.
pub const INPUT_QUEUE_CAPACITY: usize = 256;

/// Largest look change one `input_add_look` call makes, in degrees.
pub const MAX_LOOK_DELTA: f32 = 180.0;

/// A binding a mod may press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// The attack binding (left button by default).
    Attack,
    /// The use binding (right button by default).
    Use,
}

impl InputAction {
    /// The action of an `UMI_INPUT_ACTION_*` value, `None` for an unknown one.
    fn from_wire(action: u32) -> Option<Self> {
        match action {
            UMI_INPUT_ACTION_ATTACK => Some(Self::Attack),
            UMI_INPUT_ACTION_USE => Some(Self::Use),
            _ => None,
        }
    }
}

/// One request a mod queued, applied by the window in call order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputRequest {
    /// `count` more presses of `action`.
    Press {
        /// The binding.
        action: InputAction,
        /// How many presses, `1..=UMI_MAX_INPUT_PRESSES`.
        count: u8,
    },
    /// Drop the presses of `action` not sent yet.
    ClearPress {
        /// The binding.
        action: InputAction,
    },
    /// Turn the look, in degrees; positive turns right and looks down.
    AddLook {
        /// Horizontal turn.
        yaw: f32,
        /// Vertical turn.
        pitch: f32,
    },
}

/// The requests mods queued, waiting for the window.
///
/// `Send + Clone`; all clones share one queue.
#[derive(Debug, Clone)]
pub struct InputQueue {
    inbox: Receiver<InputRequest>,
}

impl InputQueue {
    pub(crate) fn new(inbox: Receiver<InputRequest>) -> Self {
        Self { inbox }
    }

    /// Hands every queued request to `apply`, oldest first, without blocking or
    /// allocating.
    pub fn drain(&self, apply: impl FnMut(InputRequest)) {
        self.inbox.try_iter().for_each(apply);
    }

    /// Discards every queued request: the game lost the input before they were
    /// applied, as a real press is dropped then.
    pub fn clear(&self) {
        while self.inbox.try_recv().is_ok() {}
    }
}

/// Whether `delta` is a look change `input_add_look` accepts.
fn valid_look(delta: f32) -> bool {
    delta.is_finite() && delta.abs() <= MAX_LOOK_DELTA
}

impl ModContext {
    /// Queues `count` presses of the binding `action` (`UMI_INPUT_ACTION_*`).
    ///
    /// Returns `INVALID_ARGUMENT` for an unknown action or a count outside
    /// `1..=UMI_MAX_INPUT_PRESSES`, else as [`Self::queue_input`].
    pub fn input_press(&self, action: u32, count: u32) -> UmiStatus {
        let Some(action) = InputAction::from_wire(action) else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        let count = match u8::try_from(count) {
            Ok(count) if (1..=UMI_MAX_INPUT_PRESSES).contains(&u32::from(count)) => count,
            _ => return UmiStatus::INVALID_ARGUMENT,
        };
        self.queue_input(InputRequest::Press { action, count })
    }

    /// Queues the removal of the pending presses of `action`.
    ///
    /// Returns `INVALID_ARGUMENT` for an unknown action, else as
    /// [`Self::queue_input`].
    pub fn input_clear_press(&self, action: u32) -> UmiStatus {
        match InputAction::from_wire(action) {
            Some(action) => self.queue_input(InputRequest::ClearPress { action }),
            None => UmiStatus::INVALID_ARGUMENT,
        }
    }

    /// Queues a look change of `yaw` and `pitch` degrees.
    ///
    /// Returns `INVALID_ARGUMENT` for a value that is not finite or beyond
    /// ±[`MAX_LOOK_DELTA`], else as [`Self::queue_input`].
    pub fn input_add_look(&self, yaw: f32, pitch: f32) -> UmiStatus {
        if !valid_look(yaw) || !valid_look(pitch) {
            return UmiStatus::INVALID_ARGUMENT;
        }
        self.queue_input(InputRequest::AddLook { yaw, pitch })
    }

    /// Queues `request` for the window.
    ///
    /// Returns `DENIED` without the `input` permission, before `init` returned or
    /// while the mod is not enabled; `UNAVAILABLE` while the game does not own the
    /// input; `BUSY` when the queue is full (nothing is queued). Callable from any
    /// thread.
    fn queue_input(&self, request: InputRequest) -> UmiStatus {
        if !self.permissions().input || !self.is_enabled() {
            return UmiStatus::DENIED;
        }
        if !self.shared.game_focus() {
            return UmiStatus::UNAVAILABLE;
        }
        if self.shared.queue_input(request).is_ok() {
            return UmiStatus::OK;
        }
        let refused = self.counters().record_drop();
        warn_sparse(self, refused, "input queue full, request refused");
        UmiStatus::BUSY
    }
}

#[cfg(test)]
mod tests;
