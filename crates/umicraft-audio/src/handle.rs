// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game's end of the audio channel.
//!
//! [`AudioHandle`] is the only thing the game and render threads hold. Every
//! call is a `try_send` on a bounded channel: when the audio thread is behind
//! the message is dropped and counted, so a stuck audio device can never stall
//! the simulation (spec §5.3, `docs/threading.md`).

use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Stop the audio thread.
    Shutdown,
}

/// A cloneable sender of [`AudioMessage`]s that never blocks.
#[derive(Debug, Clone)]
pub struct AudioHandle {
    sender: SyncSender<AudioMessage>,
    dropped: Arc<AtomicU64>,
}

/// Creates a handle and the receiver the audio thread drains.
#[must_use]
pub fn channel(capacity: usize) -> (AudioHandle, Receiver<AudioMessage>) {
    let (sender, receiver) = sync_channel(capacity);
    let handle = AudioHandle {
        sender,
        dropped: Arc::new(AtomicU64::new(0)),
    };
    (handle, receiver)
}

impl AudioHandle {
    /// Asks for a sound. Returns `false` when the queue was full (or the audio
    /// thread is gone) and the request was dropped; it never blocks.
    pub fn play(&self, request: SoundRequest) -> bool {
        self.send(AudioMessage::Play(request))
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
