// SPDX-License-Identifier: MIT OR Apache-2.0

//! The [`SoundCue`]: one sound the client has been asked to play.
//!
//! A cue is plain data so that the packet decoders can be tested without an
//! audio device; a backend turns cues into playback.

use std::borrow::Cow;

/// The volume slider a cue belongs to (MCP `SoundCategory`, without `MASTER`
/// which is the sum of all the others and never the category of a sound).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueCategory {
    /// Background music (`music`).
    Music,
    /// Jukebox records and note blocks (`record`).
    Records,
    /// Rain and thunder (`weather`).
    Weather,
    /// Blocks: steps, digging, doors, pistons (`block`).
    Blocks,
    /// Hostile mobs (`hostile`).
    Hostile,
    /// Passive and neutral mobs (`neutral`).
    Neutral,
    /// Sounds made by players (`player`).
    Players,
    /// Cave and world ambience (`ambient`).
    Ambient,
}

/// One sound to play, positioned in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundCue {
    /// Sound event name as in 1.8 `sounds.json`, e.g. `random.click`.
    pub name: Cow<'static, str>,
    /// World position of the source.
    pub pos: [f64; 3],
    /// Linear volume; above 1.0 widens the audible range instead of loudness.
    pub volume: f32,
    /// Pitch multiplier (1.0 is the recorded pitch).
    pub pitch: f32,
    /// Volume slider the cue obeys.
    pub category: CueCategory,
}
