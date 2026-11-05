// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client's end of the audio engine (P11-E).
//!
//! [`AudioBridge`] owns the audio thread of `umicraft-audio` and is the only
//! audio type the window touches. It turns the game's sound cues
//! ([`cues`]), the camera ([`listener`]), the menu clicks ([`ui_sounds`]) and
//! the sound options ([`settings`]) into messages for the audio thread, and
//! finds the user's sound files ([`loader`]).
//!
//! Every call is non-blocking (`AudioHandle` is a `try_send` on a bounded
//! queue, see `docs/threading.md`). A bridge without a handle is silent:
//! `--no-audio`, a machine with no downloaded assets, headless runs and tests.
//! A missing sound file or device never stops the client.

pub mod cues;
pub mod listener;
pub mod loader;
pub mod probe;
pub mod settings;
pub mod ui_sounds;

use std::time::{SystemTime, UNIX_EPOCH};

use umicraft_audio::thread::{self, AudioThread};
use umicraft_audio::{AudioHandle, Listener, SoundRequest};
use umicraft_game::sound::cue::SoundCue;

use crate::options::SoundOptions;
use settings::VolumeSync;

/// The audio thread and the handle that feeds it, or nothing.
#[derive(Debug, Default)]
pub struct AudioBridge {
    handle: Option<AudioHandle>,
    thread: Option<AudioThread>,
    volumes: VolumeSync,
}

impl AudioBridge {
    /// A silent bridge: no thread, no device, every call does nothing. Used by
    /// `--no-audio`, the headless modes and the tests.
    #[must_use]
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Starts the audio thread on the user's own sounds and applies the sound
    /// options.
    ///
    /// Falls back to [`AudioBridge::disabled`] with a warning when the assets
    /// are not downloaded or the thread cannot start.
    #[must_use]
    pub fn start(options: &SoundOptions) -> Self {
        let (loader, index) = match loader::open() {
            Ok(sounds) => sounds,
            Err(error) => {
                tracing::warn!(%error, "sounds are off");
                return Self::disabled();
            }
        };
        let thread = match thread::spawn(Box::new(loader), seed()) {
            Ok(thread) => thread,
            Err(error) => {
                tracing::warn!(%error, "cannot start the audio thread, sounds are off");
                return Self::disabled();
            }
        };
        let handle = thread.handle();
        handle.set_index(index);
        let mut bridge = Self {
            handle: Some(handle),
            thread: Some(thread),
            volumes: VolumeSync::default(),
        };
        bridge.sync_volumes(options);
        bridge
    }

    /// A bridge on an existing handle, whose receiver the test drains.
    #[cfg(test)]
    pub(crate) fn with_handle(handle: AudioHandle) -> Self {
        Self {
            handle: Some(handle),
            thread: None,
            volumes: VolumeSync::default(),
        }
    }

    /// Whether sounds can play.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.handle.is_some()
    }

    /// Plays a request; dropped when the queue is full or the bridge is silent.
    pub fn play(&self, request: SoundRequest) {
        if let Some(handle) = &self.handle {
            handle.play(request);
        }
    }

    /// Plays the cues the game thread produced, oldest first.
    pub fn play_cues(&self, cues: &[SoundCue]) {
        for cue in cues {
            self.play(cues::request_of(cue));
        }
    }

    /// Reports where the player's ears are.
    pub fn set_listener(&self, listener: Listener) {
        if let Some(handle) = &self.handle {
            handle.set_listener(listener);
        }
    }

    /// Pushes the sound options that changed since the last call.
    pub fn sync_volumes(&mut self, options: &SoundOptions) {
        if let Some(handle) = &self.handle {
            self.volumes.sync(handle, options);
        }
    }

    /// How many messages the audio queue dropped (a stuck device).
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.handle.as_ref().map_or(0, AudioHandle::dropped)
    }
}

impl Drop for AudioBridge {
    fn drop(&mut self) {
        // Our handle goes first: if the queue is full the shutdown message is
        // dropped, and the thread then ends because every handle is gone.
        self.handle = None;
        if let Some(thread) = self.thread.take() {
            if !thread.stop() {
                tracing::warn!("the audio thread panicked");
            }
        }
    }
}

/// A seed for the weighted sound picks (vanilla uses an unseeded `Random`).
fn seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        // Only the low bits matter for a pick seed.
        .map_or(1, |elapsed| elapsed.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use umicraft_audio::{channel, AudioMessage};
    use umicraft_game::sound::cue::CueCategory;

    use super::*;

    fn cue(name: &'static str) -> SoundCue {
        SoundCue {
            name: Cow::Borrowed(name),
            pos: [0.0; 3],
            volume: 1.0,
            pitch: 1.0,
            category: CueCategory::Blocks,
        }
    }

    #[test]
    fn a_disabled_bridge_is_silent_and_never_panics() {
        let mut bridge = AudioBridge::disabled();
        assert!(!bridge.is_enabled());
        bridge.play_cues(&[cue("dig.stone")]);
        bridge.set_listener(Listener {
            position: [0.0; 3],
            yaw: 0.0,
        });
        bridge.sync_volumes(&SoundOptions::default());
        assert_eq!(bridge.dropped(), 0);
    }

    #[test]
    fn cues_are_played_in_order() {
        let (handle, receiver) = channel(8);
        let bridge = AudioBridge::with_handle(handle);
        bridge.play_cues(&[cue("step.grass"), cue("dig.stone")]);
        let events: Vec<_> = receiver
            .try_iter()
            .filter_map(|message| match message {
                AudioMessage::Play(request) => Some(request.event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!((&*events[0], &*events[1]), ("step.grass", "dig.stone"));
    }

    #[test]
    fn a_full_queue_drops_cues_instead_of_blocking() {
        let (handle, _receiver) = channel(1);
        let bridge = AudioBridge::with_handle(handle);
        bridge.play_cues(&[cue("a"), cue("b"), cue("c")]);
        assert_eq!(bridge.dropped(), 2);
    }
}
