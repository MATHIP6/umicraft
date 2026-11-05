// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mixing backends: the real one ([`CpalBackend`]) and the silent one
//! ([`NullBackend`]) for tests and headless runs.

mod cpal;
mod mixer;
mod null;

pub use self::cpal::CpalBackend;
pub use mixer::{Mixer, MAX_VOICES};
pub use null::NullBackend;

use crate::decode::DecodedSound;
use crate::error::AudioError;

/// One sound ready to mix: the decoded data and how to play it.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    /// The PCM to play.
    pub sound: DecodedSound,
    /// The final loudness, `0.0..=1.0`: entry and request volume, category
    /// volume and distance gain already applied.
    pub volume: f32,
    /// The playback speed, `0.5..=2.0` (`SoundManager.getNormalizedPitch`).
    pub pitch: f32,
    /// The stereo position, `-1.0` (left) to `1.0` (right).
    pub pan: f32,
    /// Whether the voice holds a streaming channel instead of a normal one.
    pub streaming: bool,
}

/// How many voices ended since the last [`AudioBackend::reap`], by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reaped {
    /// Normal voices that ended.
    pub normal: usize,
    /// Streaming voices that ended.
    pub streaming: usize,
}

/// A mixer the audio thread plays voices on. Owned by that thread only.
pub trait AudioBackend {
    /// Starts a voice.
    ///
    /// # Errors
    ///
    /// [`AudioError::Backend`] when the mixer refuses it (for example it is out
    /// of voice slots).
    fn play(&mut self, voice: Voice) -> Result<(), AudioError>;

    /// Removes the voices that finished and reports how many, so the caller
    /// can give their sources back.
    fn reap(&mut self) -> Reaped;

    /// Ends every streaming voice (the music track). They are reported by a
    /// later [`Self::reap`], like voices that ended by themselves.
    fn stop_streaming(&mut self);
}

impl AudioBackend for Box<dyn AudioBackend> {
    fn play(&mut self, voice: Voice) -> Result<(), AudioError> {
        (**self).play(voice)
    }

    fn reap(&mut self) -> Reaped {
        (**self).reap()
    }

    fn stop_streaming(&mut self) {
        (**self).stop_streaming();
    }
}
