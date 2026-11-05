// SPDX-License-Identifier: MIT OR Apache-2.0

//! The audio state machine: turns requests into voices.
//!
//! It is the counterpart of MCP `SoundManager`. It is plain synchronous code
//! with no thread of its own, so tests drive it with a [`NullBackend`]; the
//! audio thread ([`crate::thread`]) calls it from its loop.
//!
//! [`NullBackend`]: crate::backend::NullBackend

use std::io;

use crate::attenuation::distance_gain;
use crate::backend::{AudioBackend, Voice};
use crate::cache::SoundCache;
use crate::category::{CategoryVolumes, SoundCategory};
use crate::decode::{decode_ogg, DecodedSound};
use crate::error::AudioError;
use crate::handle::AudioMessage;
use crate::index::{PickedSound, SeededRng, SoundIndex};
use crate::limits::{DelayedSounds, SourceLimit};
use crate::request::{Listener, SoundRequest};

/// Reads a sound file by resource path (`sounds/dig/stone1.ogg`).
///
/// The caller decides where files live (the user's asset store, a test map).
pub trait SoundLoader: Send {
    /// The bytes of the file.
    ///
    /// # Errors
    ///
    /// Any I/O failure, including a missing file.
    fn load(&mut self, resource_path: &str) -> io::Result<Vec<u8>>;
}

/// The audio state: index, cache, limits and the backend.
pub struct Engine<B: AudioBackend> {
    backend: B,
    loader: Box<dyn SoundLoader>,
    index: SoundIndex,
    cache: SoundCache,
    volumes: CategoryVolumes,
    listener: Listener,
    limit: SourceLimit,
    delayed: DelayedSounds<SoundRequest>,
    rng: SeededRng,
}

impl<B: AudioBackend> Engine<B> {
    /// An engine with an empty index; `seed` makes the weighted picks reproducible.
    pub fn new(backend: B, loader: Box<dyn SoundLoader>, seed: u32) -> Self {
        Self {
            backend,
            loader,
            index: SoundIndex::new(),
            cache: SoundCache::default(),
            volumes: CategoryVolumes::default(),
            listener: Listener {
                position: [0.0; 3],
                yaw: 0.0,
            },
            limit: SourceLimit::default(),
            delayed: DelayedSounds::default(),
            rng: SeededRng::new(seed),
        }
    }

    /// The backend, for tests to inspect.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// The backend, mutably.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Applies one message; returns `false` for [`AudioMessage::Shutdown`].
    pub fn handle(&mut self, message: AudioMessage) -> bool {
        match message {
            AudioMessage::Play(request) => self.request(request),
            AudioMessage::Listener(listener) => self.listener = listener,
            AudioMessage::Volume(category, volume) => self.volumes.set(category, volume),
            AudioMessage::Index(index) => self.index = *index,
            AudioMessage::Shutdown => return false,
        }
        true
    }

    /// Runs one 20 Hz tick: frees finished sources and starts the sounds whose
    /// delay is over (`SoundManager.updateAllSounds`).
    pub fn tick(&mut self) {
        let reaped = self.backend.reap();
        for _ in 0..reaped.normal {
            self.limit.release(false);
        }
        for _ in 0..reaped.streaming {
            self.limit.release(true);
        }
        let mut due = Vec::new();
        self.delayed.tick(|request| due.push(request));
        for request in due {
            self.play_now(&request);
        }
    }

    fn request(&mut self, request: SoundRequest) {
        if request.delay_ticks > 0 {
            let delay = request.delay_ticks;
            self.delayed.schedule(request, delay);
        } else {
            self.play_now(&request);
        }
    }

    /// `SoundManager.playSound`: pick, normalise, attenuate, take a source, play.
    fn play_now(&mut self, request: &SoundRequest) {
        if self.volumes.get(SoundCategory::Master) <= 0.0 {
            return; // "Skipped playing soundEvent: master volume was zero"
        }
        let Some(picked) = self.index.pick(&request.event, &mut self.rng) else {
            tracing::debug!(event = &*request.event, "unknown or empty sound event");
            return;
        };
        let category = request.category.unwrap_or(picked.category);
        let Some(voice) = self.build_voice(request, &picked, category) else {
            return;
        };
        if !self.limit.try_acquire(voice.streaming) {
            return; // no free channel: the sound is dropped, as in vanilla
        }
        if let Err(error) = self.backend.play(voice) {
            self.limit.release(picked.stream);
            tracing::warn!(%error, event = &*request.event, "cannot play sound");
        }
    }

    /// The voice for a pick, or `None` when it is inaudible or cannot be loaded.
    fn build_voice(
        &mut self,
        request: &SoundRequest,
        picked: &PickedSound,
        category: SoundCategory,
    ) -> Option<Voice> {
        // `getNormalizedVolume`: clamp(sound volume * entry volume) * category volume.
        let mut volume =
            (request.volume * picked.volume).clamp(0.0, 1.0) * self.volumes.effective(category);
        let mut pan = 0.0;
        if let Some(position) = request.position {
            let distance = self.listener.distance_to(position);
            volume *= distance_gain(distance, request.volume);
            pan = self.listener.pan_to(position);
        }
        if volume <= 0.0 {
            return None; // "Skipped playing sound, volume was zero"
        }
        let sound = self.sound(&picked.resource_path())?;
        Some(Voice {
            sound,
            volume,
            // `getNormalizedPitch`: clamp(sound pitch * entry pitch, 0.5, 2.0).
            pitch: (request.pitch * picked.pitch).clamp(0.5, 2.0),
            pan,
            streaming: picked.stream,
        })
    }

    /// The decoded sound of a resource path, from the cache or the loader.
    fn sound(&mut self, resource_path: &str) -> Option<DecodedSound> {
        if let Some(sound) = self.cache.get(resource_path) {
            return Some(sound);
        }
        match self.decode(resource_path) {
            Ok(sound) => {
                self.cache.insert(resource_path.to_owned(), sound.clone());
                Some(sound)
            }
            Err(error) => {
                tracing::warn!(%error, "cannot load sound");
                None
            }
        }
    }

    fn decode(&mut self, resource_path: &str) -> Result<DecodedSound, AudioError> {
        let bytes = self
            .loader
            .load(resource_path)
            .map_err(|source| AudioError::Load {
                path: resource_path.to_owned(),
                source,
            })?;
        decode_ogg(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::backend::NullBackend;

    const TONE: &[u8] = include_bytes!("../tests/fixtures/tone_440hz_mono.ogg");

    struct MapLoader(HashMap<String, Vec<u8>>);

    impl SoundLoader for MapLoader {
        fn load(&mut self, path: &str) -> io::Result<Vec<u8>> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))
        }
    }

    fn engine() -> Engine<NullBackend> {
        let files = HashMap::from([("sounds/tone.ogg".to_owned(), TONE.to_vec())]);
        let mut engine = Engine::new(NullBackend::new(), Box::new(MapLoader(files)), 1);
        let index = SoundIndex::parse(
            r#"{
            "tone": {"category": "block", "sounds": [{"name": "tone", "volume": 0.5}]},
            "gone": {"sounds": ["nothing"]}
        }"#,
        )
        .expect("valid");
        assert!(engine.handle(AudioMessage::Index(Box::new(index))));
        engine
    }

    fn play(engine: &mut Engine<NullBackend>, request: SoundRequest) {
        engine.handle(AudioMessage::Play(request));
    }

    #[test]
    fn plays_a_sound_with_normalised_volume_and_pitch() {
        let mut engine = engine();
        play(&mut engine, SoundRequest::new("tone").with_pitch(4.0));
        let voice = &engine.backend().played[0];
        assert_eq!(voice.volume, 0.5);
        assert_eq!(voice.pitch, 2.0);
        assert_eq!(voice.pan, 0.0);
    }

    #[test]
    fn category_and_master_volumes_apply() {
        let mut engine = engine();
        engine.handle(AudioMessage::Volume(SoundCategory::Blocks, 0.5));
        engine.handle(AudioMessage::Volume(SoundCategory::Master, 0.5));
        play(&mut engine, SoundRequest::new("tone"));
        assert_eq!(engine.backend().played[0].volume, 0.125);
        engine.handle(AudioMessage::Volume(SoundCategory::Master, 0.0));
        play(&mut engine, SoundRequest::new("tone"));
        assert_eq!(engine.backend().played.len(), 1);
    }

    #[test]
    fn positioned_sounds_attenuate_and_pan() {
        let mut engine = engine();
        play(&mut engine, SoundRequest::new("tone").at([-8.0, 0.0, 0.0]));
        let voice = &engine.backend().played[0];
        assert_eq!(voice.volume, 0.25); // 0.5 * (1 - 8 / 16)
        assert!(voice.pan > 0.99);
        play(&mut engine, SoundRequest::new("tone").at([20.0, 0.0, 0.0]));
        assert_eq!(
            engine.backend().played.len(),
            1,
            "beyond 16 blocks is silent"
        );
    }

    #[test]
    fn unknown_events_and_missing_files_play_nothing() {
        let mut engine = engine();
        play(&mut engine, SoundRequest::new("nope"));
        play(&mut engine, SoundRequest::new("gone"));
        assert!(engine.backend().played.is_empty());
    }

    #[test]
    fn the_source_limit_drops_extra_sounds_until_some_finish() {
        let mut engine = engine();
        for _ in 0..40 {
            play(&mut engine, SoundRequest::new("tone"));
        }
        assert_eq!(engine.backend().played.len(), 28);
        engine.backend_mut().finish_all();
        engine.tick();
        play(&mut engine, SoundRequest::new("tone"));
        assert_eq!(engine.backend().played.len(), 29);
    }

    #[test]
    fn delayed_sounds_wait_for_their_tick() {
        let mut engine = engine();
        play(&mut engine, SoundRequest::new("tone").delayed(2));
        assert!(engine.backend().played.is_empty());
        engine.tick();
        assert!(engine.backend().played.is_empty());
        engine.tick();
        assert_eq!(engine.backend().played.len(), 1);
    }

    #[test]
    fn shutdown_stops_the_loop() {
        assert!(!engine().handle(AudioMessage::Shutdown));
    }
}
