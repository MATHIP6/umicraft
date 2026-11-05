// SPDX-License-Identifier: MIT OR Apache-2.0

//! The audio state machine: turns requests into voices.
//!
//! It is the counterpart of MCP `SoundManager`. It is plain synchronous code
//! with no thread of its own, so tests drive it with a [`NullBackend`]; the
//! audio thread ([`crate::thread`]) calls it from its loop.
//!
//! [`NullBackend`]: crate::backend::NullBackend

use std::io;
use std::sync::Arc;

use crate::attenuation::distance_gain;
use crate::backend::AudioBackend;
use crate::cache::SoundCache;
use crate::category::{CategoryVolumes, SoundCategory};
use crate::decode::{decode_ogg, DecodedSound};
use crate::error::AudioError;
use crate::handle::{AudioMessage, MusicActivity};
use crate::index::{PickedSound, SeededRng, SoundIndex};
use crate::limits::{DelayedSounds, SourceLimit};
use crate::request::{Listener, SoundRequest};
use crate::stream::{Decoded, PlayParams, StreamDecoder};

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
    /// Decodes streamed files (music) off this thread.
    streams: StreamDecoder,
    /// Which music tracks are in flight, read by the game thread.
    music: Arc<MusicActivity>,
    /// Bumped by every stop; a stream decoded for an older one is discarded.
    stop_generation: u32,
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
            streams: StreamDecoder::default(),
            music: Arc::default(),
            stop_generation: 0,
        }
    }

    /// Reports the music tracks to `music`, the state of an
    /// [`AudioHandle`](crate::handle::AudioHandle).
    #[must_use]
    pub fn with_music_activity(mut self, music: Arc<MusicActivity>) -> Self {
        self.music = music;
        self
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
            AudioMessage::StopMusic => self.stop_music(),
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
            self.music.finish();
        }
        while let Some(decoded) = self.streams.poll() {
            self.start_decoded(decoded);
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

    /// `SoundManager.playSound`. A music request that does not end up as a
    /// streaming voice is reported finished at once, so the ticker moves on.
    fn play_now(&mut self, request: &SoundRequest) {
        let streaming = self.start(request);
        if !streaming && request.category == Some(SoundCategory::Music) {
            self.music.finish();
        }
    }

    /// Picks, normalises, attenuates, takes a source and plays. Returns whether
    /// a streaming voice is now loading or playing.
    fn start(&mut self, request: &SoundRequest) -> bool {
        if self.volumes.get(SoundCategory::Master) <= 0.0 {
            return false; // "Skipped playing soundEvent: master volume was zero"
        }
        let Some(picked) = self.index.pick(&request.event, &mut self.rng) else {
            tracing::debug!(event = &*request.event, "unknown or empty sound event");
            return false;
        };
        let category = request.category.unwrap_or(picked.category);
        let Some(params) = self.play_params(request, &picked, category) else {
            return false;
        };
        let path = picked.resource_path();
        if picked.stream {
            return self.queue_stream(&path, params);
        }
        let Some(sound) = self.sound(&path) else {
            return false;
        };
        if !self.limit.try_acquire(false) {
            return false; // no free channel: the sound is dropped, as in vanilla
        }
        if let Err(error) = self.backend.play(params.into_voice(sound, false)) {
            self.limit.release(false);
            tracing::warn!(%error, event = &*request.event, "cannot play sound");
        }
        false
    }

    /// The loudness, pitch and pan of a pick, or `None` when it is inaudible.
    fn play_params(
        &self,
        request: &SoundRequest,
        picked: &PickedSound,
        category: SoundCategory,
    ) -> Option<PlayParams> {
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
        Some(PlayParams {
            volume,
            // `getNormalizedPitch`: clamp(sound pitch * entry pitch, 0.5, 2.0).
            pitch: (request.pitch * picked.pitch).clamp(0.5, 2.0),
            pan,
            generation: self.stop_generation,
        })
    }

    /// Reads a streamed file and sends it to the decoder thread; the voice
    /// starts in [`Self::start_decoded`]. Holds a streaming source meanwhile.
    fn queue_stream(&mut self, resource_path: &str, params: PlayParams) -> bool {
        if !self.limit.try_acquire(true) {
            return false;
        }
        let queued = self
            .loader
            .load(resource_path)
            .map_err(|source| AudioError::Load {
                path: resource_path.to_owned(),
                source,
            })
            .and_then(|bytes| {
                self.streams
                    .submit(bytes, params)
                    .map_err(|error| AudioError::Backend(error.to_string()))
            });
        if let Err(error) = &queued {
            tracing::warn!(%error, "cannot stream sound");
            self.limit.release(true);
        }
        queued.is_ok()
    }

    /// Plays a stream the decoder finished, unless a stop cancelled it.
    fn start_decoded(&mut self, decoded: Decoded) {
        let Decoded { sound, params } = decoded;
        let started = match sound {
            _ if params.generation != self.stop_generation => false,
            Ok(sound) => match self.backend.play(params.into_voice(sound, true)) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(%error, "cannot play stream");
                    false
                }
            },
            Err(error) => {
                tracing::warn!(%error, "cannot decode stream");
                false
            }
        };
        if !started {
            self.limit.release(true);
            self.music.finish();
        }
    }

    /// Ends the music: the audible track now, a still-decoding one on arrival.
    fn stop_music(&mut self) {
        self.stop_generation = self.stop_generation.wrapping_add(1);
        self.backend.stop_streaming();
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

    /// An engine with a streamed music event and the handle-side activity.
    fn music_engine() -> (Engine<NullBackend>, Arc<MusicActivity>) {
        let files = HashMap::from([("sounds/tone.ogg".to_owned(), TONE.to_vec())]);
        let music = Arc::new(MusicActivity::default());
        let mut engine = Engine::new(NullBackend::new(), Box::new(MapLoader(files)), 1)
            .with_music_activity(Arc::clone(&music));
        let index = SoundIndex::parse(
            r#"{
            "music.menu": {"category": "music", "sounds": [{"name": "tone", "stream": true}]},
            "tone": {"sounds": ["tone"]},
            "music.bad": {"category": "music", "sounds": [{"name": "nothing", "stream": true}]}
        }"#,
        )
        .expect("valid");
        engine.handle(AudioMessage::Index(Box::new(index)));
        (engine, music)
    }

    fn play_music(engine: &mut Engine<NullBackend>, music: &MusicActivity, event: &str) {
        music.request();
        let mut request = SoundRequest::new(event);
        request.category = Some(SoundCategory::Music);
        engine.handle(AudioMessage::Play(request));
    }

    /// Ticks until the decoder thread delivered a voice (or a deadline passes).
    fn tick_until_played(engine: &mut Engine<NullBackend>, voices: usize) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while engine.backend().played.len() < voices && std::time::Instant::now() < deadline {
            engine.tick();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn music_decodes_off_thread_and_stays_in_flight_until_it_ends() {
        let (mut engine, music) = music_engine();
        play_music(&mut engine, &music, "music.menu");
        assert!(
            engine.backend().played.is_empty(),
            "decoding is asynchronous"
        );
        assert!(music.is_playing());
        tick_until_played(&mut engine, 1);
        let voice = &engine.backend().played[0];
        assert!(voice.streaming);
        assert!(music.is_playing());
        engine.backend_mut().finish_all();
        engine.tick();
        assert!(!music.is_playing());
    }

    #[test]
    fn a_failed_music_request_finishes_at_once() {
        let (mut engine, music) = music_engine();
        play_music(&mut engine, &music, "music.nothing"); // unknown event
        assert!(!music.is_playing());
        play_music(&mut engine, &music, "music.bad"); // missing file
        assert!(!music.is_playing());
        engine.handle(AudioMessage::Volume(SoundCategory::Master, 0.0));
        play_music(&mut engine, &music, "music.menu");
        assert!(!music.is_playing(), "silent master plays nothing");
    }

    #[test]
    fn stopping_ends_the_playing_track_and_cancels_a_decoding_one() {
        let (mut engine, music) = music_engine();
        play_music(&mut engine, &music, "music.menu");
        tick_until_played(&mut engine, 1);
        engine.handle(AudioMessage::StopMusic);
        engine.tick();
        assert!(!music.is_playing());

        play_music(&mut engine, &music, "music.menu");
        engine.handle(AudioMessage::StopMusic); // before the decode comes back
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while music.is_playing() && std::time::Instant::now() < deadline {
            engine.tick();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!music.is_playing());
        assert_eq!(
            engine.backend().played.len(),
            1,
            "the cancelled track never played"
        );
    }

    #[test]
    fn sound_effects_are_not_held_back_by_a_decoding_track() {
        let (mut engine, music) = music_engine();
        play_music(&mut engine, &music, "music.menu");
        // The effect plays in the same drain, without waiting for the decoder.
        play(&mut engine, SoundRequest::new("tone"));
        let played = &engine.backend().played;
        assert_eq!(played.len(), 1);
        assert!(!played[0].streaming);
    }

    #[test]
    fn shutdown_stops_the_loop() {
        assert!(!engine().handle(AudioMessage::Shutdown));
    }
}
