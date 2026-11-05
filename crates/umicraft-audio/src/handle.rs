// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game's end of the audio channel.
//!
//! [`AudioHandle`] is the only thing the game and render threads hold. Every
//! call is a `try_send` on a bounded channel: when the audio thread is behind
//! the message is dropped and counted, so a stuck audio device can never stall
//! the simulation (spec §5.3, `docs/threading.md`).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;

use crate::category::SoundCategory;
use crate::index::SoundIndex;
use crate::request::{Listener, SoundRequest};

/// The default queue length. A busy tick plays a few dozen sounds, so 256 only
/// fills when the audio thread is stuck.
pub const QUEUE_CAPACITY: usize = 256;

/// A message from the game to the audio thread.
#[derive(Debug)]
pub enum AudioMessage {
    /// Play a sound.
    Play(SoundRequest),
    /// The listener moved or turned.
    Listener(Listener),
    /// A category volume changed (the options page).
    Volume(SoundCategory, f32),
    /// Replace the sound index (a resource reload).
    Index(Box<SoundIndex>),
    /// Stop the music track (and any other streaming voice) at once.
    StopMusic,
    /// Stop the audio thread.
    Shutdown,
}

/// Which music tracks are still in flight, shared by the game and audio sides.
///
/// Two monotonic counters instead of one flag: the game side counts a track
/// *before* it queues the request, the audio side counts it finished when the
/// voice ends, fails or is dropped. "Playing" is `requested != finished`, so a
/// request still sitting in the queue already counts and the ticker cannot
/// mistake it for a track that ended (a plain flag would race with the audio
/// thread's tick).
#[derive(Debug, Default)]
pub struct MusicActivity {
    requested: AtomicU32,
    finished: AtomicU32,
}

impl MusicActivity {
    /// Whether a track was requested and has not finished yet.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.requested.load(Ordering::Acquire) != self.finished.load(Ordering::Acquire)
    }

    pub(crate) fn request(&self) {
        self.requested.fetch_add(1, Ordering::AcqRel);
    }

    /// Marks one requested track as finished; a no-op when none is in flight, so
    /// a streaming voice that is not music cannot unbalance the counters.
    pub fn finish(&self) {
        if self.is_playing() {
            self.finished.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// A cloneable sender of [`AudioMessage`]s that never blocks.
#[derive(Debug, Clone)]
pub struct AudioHandle {
    sender: SyncSender<AudioMessage>,
    dropped: Arc<AtomicU64>,
    music: Arc<MusicActivity>,
}

/// Creates a handle and the receiver the audio thread drains.
#[must_use]
pub fn channel(capacity: usize) -> (AudioHandle, Receiver<AudioMessage>) {
    let (sender, receiver) = sync_channel(capacity);
    let handle = AudioHandle {
        sender,
        dropped: Arc::new(AtomicU64::new(0)),
        music: Arc::default(),
    };
    (handle, receiver)
}

impl AudioHandle {
    /// Asks for a sound. Returns `false` when the queue was full (or the audio
    /// thread is gone) and the request was dropped; it never blocks.
    pub fn play(&self, request: SoundRequest) -> bool {
        self.send(AudioMessage::Play(request))
    }

    /// Asks for a music track (`category` is forced to music). Returns `false`
    /// when the queue was full and the track was dropped; it never blocks.
    pub fn play_music(&self, mut request: SoundRequest) -> bool {
        request.category = Some(SoundCategory::Music);
        self.music.request();
        let sent = self.send(AudioMessage::Play(request));
        if !sent {
            self.music.finish();
        }
        sent
    }

    /// Stops the music track now; dropped when the queue is full (the music
    /// ticker asks again on a later tick only if the situation still calls for it).
    pub fn stop_music(&self) -> bool {
        self.send(AudioMessage::StopMusic)
    }

    /// Whether a track asked for with [`Self::play_music`] is still queued,
    /// loading or audible. A lock-free read, safe on any thread.
    #[must_use]
    pub fn music_playing(&self) -> bool {
        self.music.is_playing()
    }

    /// The shared state the audio thread updates.
    #[must_use]
    pub fn music_activity(&self) -> Arc<MusicActivity> {
        Arc::clone(&self.music)
    }

    /// Reports the listener; dropped when the queue is full, the next call
    /// supersedes it anyway.
    pub fn set_listener(&self, listener: Listener) -> bool {
        self.send(AudioMessage::Listener(listener))
    }

    /// Sets the volume of a category.
    pub fn set_volume(&self, category: SoundCategory, volume: f32) -> bool {
        self.send(AudioMessage::Volume(category, volume))
    }

    /// Hands over a new sound index.
    pub fn set_index(&self, index: SoundIndex) -> bool {
        self.send(AudioMessage::Index(Box::new(index)))
    }

    /// Asks the audio thread to stop.
    pub fn shutdown(&self) -> bool {
        self.send(AudioMessage::Shutdown)
    }

    /// How many messages were dropped because the queue was full or closed.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn send(&self, message: AudioMessage) -> bool {
        let sent = self.sender.try_send(message).is_ok();
        if !sent {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_never_blocks_when_the_queue_is_full() {
        let (handle, receiver) = channel(2);
        assert!(handle.play(SoundRequest::new("a")));
        assert!(handle.play(SoundRequest::new("b")));
        // Nobody drains the receiver: these must return at once, not wait.
        for _ in 0..1_000 {
            assert!(!handle.play(SoundRequest::new("c")));
        }
        assert_eq!(handle.dropped(), 1_000);
        assert_eq!(receiver.try_iter().count(), 2);
    }

    #[test]
    fn a_queued_music_request_counts_as_playing_until_it_finishes() {
        let (handle, receiver) = channel(4);
        assert!(!handle.music_playing());
        assert!(handle.play_music(SoundRequest::new("music.menu")));
        assert!(handle.music_playing(), "queued, not yet read by the thread");
        let Some(AudioMessage::Play(request)) = receiver.try_iter().next() else {
            panic!("a play message");
        };
        assert_eq!(request.category, Some(SoundCategory::Music));
        handle.music_activity().finish();
        assert!(!handle.music_playing());
        handle.music_activity().finish(); // extra finishes are harmless
        assert!(handle.play_music(SoundRequest::new("music.menu")));
        assert!(handle.music_playing());
    }

    #[test]
    fn a_dropped_music_request_is_not_playing() {
        let (handle, _receiver) = channel(0);
        assert!(!handle.play_music(SoundRequest::new("music.menu")));
        assert!(!handle.music_playing());
        assert_eq!(handle.dropped(), 1);
    }

    #[test]
    fn stop_music_is_a_non_blocking_message() {
        let (handle, receiver) = channel(1);
        assert!(handle.stop_music());
        assert!(!handle.stop_music());
        assert!(matches!(receiver.try_recv(), Ok(AudioMessage::StopMusic)));
    }

    #[test]
    fn draining_makes_room_again() {
        let (handle, receiver) = channel(1);
        assert!(handle.play(SoundRequest::new("a")));
        assert!(!handle.play(SoundRequest::new("b")));
        receiver.try_recv().expect("one message");
        assert!(handle.play(SoundRequest::new("b")));
    }

    #[test]
    fn a_closed_audio_thread_drops_without_panicking() {
        let (handle, receiver) = channel(4);
        drop(receiver);
        assert!(!handle.play(SoundRequest::new("a")));
        assert!(!handle.shutdown());
        assert_eq!(handle.dropped(), 2);
    }

    #[test]
    fn clones_share_the_drop_counter() {
        let (handle, _receiver) = channel(0);
        let other = handle.clone();
        other.play(SoundRequest::new("a"));
        assert_eq!(handle.dropped(), 1);
    }
}
