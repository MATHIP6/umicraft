// SPDX-License-Identifier: MIT OR Apache-2.0

//! The error type of the audio crate.

use thiserror::Error;

/// Everything that can go wrong in the audio engine.
///
/// None of these reach the game thread: the audio thread logs them and keeps
/// running, because a missing sound must never stop a game.
#[derive(Debug, Error)]
pub enum AudioError {
    /// `sounds.json` is not valid JSON or has the wrong shape.
    #[error("invalid sounds.json: {0}")]
    Index(#[from] serde_json::Error),
    /// A `sounds.json` entry breaks a rule of `SoundListSerializer`.
    #[error("invalid sound entry `{event}`: {reason}")]
    InvalidEntry {
        /// The event name the entry belongs to.
        event: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// A sound file could not be read.
    #[error("cannot read sound file `{path}`: {source}")]
    Load {
        /// The resource path that was requested.
        path: String,
        /// The I/O failure.
        source: std::io::Error,
    },
    /// The Ogg Vorbis stream could not be decoded.
    #[error("cannot decode Ogg Vorbis: {0}")]
    Decode(String),
    /// The stream decoded to no audio frames.
    #[error("the sound has no audio frames")]
    EmptySound,
    /// The audio device or its engine failed.
    #[error("audio backend failure: {0}")]
    Backend(String),
}
