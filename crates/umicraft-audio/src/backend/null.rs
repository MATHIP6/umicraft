// SPDX-License-Identifier: MIT OR Apache-2.0

//! A backend that plays nothing: for tests and machines without a sound device.

use super::{AudioBackend, Reaped, Voice};
use crate::error::AudioError;

/// Records the voices it is asked to play and never opens a device.
///
/// A voice stays "playing" until [`NullBackend::finish_all`] is called, so tests
/// control exactly when sources come free.
#[derive(Debug, Default)]
pub struct NullBackend {
    /// Every voice started so far, oldest first.
    pub played: Vec<Voice>,
    /// Every gain change of a looping voice, `(key, volume, pan)`, oldest first.
    pub adjusted: Vec<(u64, f32, f32)>,
    /// The keys of the looping voices still playing.
    pub looping: Vec<u64>,
    playing: Reaped,
    finished: Reaped,
}

impl NullBackend {
    /// A backend with no voice.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks every playing voice as ended; the next `reap` reports them.
    pub fn finish_all(&mut self) {
        self.looping.clear();
        self.finished.normal += std::mem::take(&mut self.playing.normal);
        self.finished.streaming += std::mem::take(&mut self.playing.streaming);
    }
}

impl AudioBackend for NullBackend {
    fn play(&mut self, voice: Voice) -> Result<(), AudioError> {
        if voice.streaming {
            self.playing.streaming += 1;
        } else {
            self.playing.normal += 1;
        }
        if let Some(key) = voice.loop_key {
            self.looping.push(key);
        }
        self.played.push(voice);
        Ok(())
    }

    fn reap(&mut self) -> Reaped {
        std::mem::take(&mut self.finished)
    }

    fn stop_streaming(&mut self) {
        self.finished.streaming += std::mem::take(&mut self.playing.streaming);
    }

    fn adjust_loop(&mut self, key: u64, volume: f32, pan: f32) -> Result<(), AudioError> {
        if self.looping.contains(&key) {
            self.adjusted.push((key, volume, pan));
        }
        Ok(())
    }

    fn stop_loop(&mut self, key: u64) -> Result<(), AudioError> {
        let before = self.looping.len();
        self.looping.retain(|&playing| playing != key);
        let stopped = before - self.looping.len();
        self.playing.normal = self.playing.normal.saturating_sub(stopped);
        self.finished.normal += stopped;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::DecodedSound;

    fn voice(streaming: bool) -> Voice {
        Voice {
            sound: DecodedSound {
                sample_rate: 44_100,
                frames: vec![[0.0; 2]; 4].into(),
            },
            volume: 1.0,
            pitch: 1.0,
            pan: 0.0,
            streaming,
            loop_key: None,
        }
    }

    #[test]
    fn records_voices_and_reaps_only_finished_ones() {
        let mut backend = NullBackend::new();
        backend.play(voice(false)).expect("null never fails");
        backend.play(voice(true)).expect("null never fails");
        assert_eq!(backend.played.len(), 2);
        assert_eq!(backend.reap(), Reaped::default());
        backend.finish_all();
        assert_eq!(
            backend.reap(),
            Reaped {
                normal: 1,
                streaming: 1
            }
        );
        assert_eq!(backend.reap(), Reaped::default());
    }

    #[test]
    fn stopping_streams_reaps_only_the_streaming_voices() {
        let mut backend = NullBackend::new();
        backend.play(voice(false)).expect("null never fails");
        backend.play(voice(true)).expect("null never fails");
        backend.stop_streaming();
        assert_eq!(
            backend.reap(),
            Reaped {
                normal: 0,
                streaming: 1
            }
        );
    }
}
