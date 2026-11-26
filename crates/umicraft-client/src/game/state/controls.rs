// SPDX-License-Identifier: MIT OR Apache-2.0

//! The controls of the player between two ticks.

/// The keys held and the buttons pressed since the last tick.
#[derive(Default)]
pub(in crate::game) struct Controls {
    /// The latest key state.
    pub(in crate::game) input: crate::input::InputState,
    /// The attack presses since the last tick, all clicked by the next one.
    pub(in crate::game) attack_presses: u8,
    /// The use presses since the last tick, all clicked by the next one.
    pub(in crate::game) use_presses: u8,
    /// The `F5` presses since the last tick, applied by the next one.
    pub(in crate::game) perspective_presses: u8,
}
