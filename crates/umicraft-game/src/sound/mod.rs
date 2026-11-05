// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sound events of the 1.8 client, as pure data (no audio backend).
//!
//! - [`cue`]: the [`cue::SoundCue`] every source produces.
//! - [`sound_packet`]: Sound Effect (0x29) to a cue.
//! - [`effect_packet`]: Effect (0x28) world events to cues.

pub mod cue;
pub mod effect_packet;
pub mod sound_packet;
