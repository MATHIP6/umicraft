// SPDX-License-Identifier: MIT OR Apache-2.0

//! The audio thread: it owns the [`Engine`] and the backend.
//!
//! Nothing else touches the mixer. The game talks to it through an
//! [`AudioHandle`]; the thread drains the messages and runs a 20 Hz tick, the
//! rate of `SoundManager.updateAllSounds`.

use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

use crate::backend::{AudioBackend, CpalBackend, NullBackend};
use crate::engine::{Engine, SoundLoader};
use crate::error::AudioError;
use crate::handle::{channel, AudioHandle, AudioMessage, QUEUE_CAPACITY};

/// The length of one game tick.
const TICK: Duration = Duration::from_millis(50);

/// A running audio thread and the handle that feeds it.
#[derive(Debug)]
pub struct AudioThread {
    handle: AudioHandle,
    join: JoinHandle<()>,
}

impl AudioThread {
    /// The handle to hand to the game and render threads.
    #[must_use]
    pub fn handle(&self) -> AudioHandle {
        self.handle.clone()
    }

    /// Asks the thread to stop and waits for it. Returns `false` when the thread
    /// panicked.
    pub fn stop(self) -> bool {
        // A full queue cannot hold the shutdown; the closed channel below ends
        // the loop as well once the last handle is gone.
        self.handle.shutdown();
        drop(self.handle);
        self.join.join().is_ok()
    }
}

/// Starts the audio thread on the system audio device.
///
/// If no device can be opened the thread keeps running on a [`NullBackend`], so
/// the game still starts on a machine without sound.
///
/// # Errors
///
/// The OS refused to start a thread.
pub fn spawn(loader: Box<dyn SoundLoader>, seed: u32) -> io::Result<AudioThread> {
    spawn_with(
        move || match CpalBackend::new() {
            Ok(backend) => Ok(Box::new(backend) as Box<dyn AudioBackend>),
            Err(error) => {
                tracing::warn!(%error, "no audio device, sounds are muted");
                Ok(Box::new(NullBackend::new()))
            }
        },
        loader,
        seed,
    )
}

/// Starts the audio thread with a backend built *on* that thread (an audio
/// stream is often not `Send`).
///
/// # Errors
///
/// The OS refused to start a thread. A backend that fails to build ends the
/// thread after a log line; the handle then drops every message.
pub fn spawn_with<B, F>(
    make_backend: F,
    loader: Box<dyn SoundLoader>,
    seed: u32,
) -> io::Result<AudioThread>
where
    B: AudioBackend,
    F: FnOnce() -> Result<B, AudioError> + Send + 'static,
{
    let (handle, receiver) = channel(QUEUE_CAPACITY);
    let join = Builder::new()
        .name("umicraft-audio".into())
        .spawn(move || match make_backend() {
            Ok(backend) => {
                run(Engine::new(backend, loader, seed), &receiver);
            }
            Err(error) => tracing::error!(%error, "audio backend failed to start"),
        })?;
    Ok(AudioThread { handle, join })
}

/// The loop: apply messages as they come, tick every 50 ms. Returns the engine
/// when a shutdown arrives or every handle is dropped.
fn run<B: AudioBackend>(mut engine: Engine<B>, receiver: &Receiver<AudioMessage>) -> Engine<B> {
    let mut next_tick = Instant::now() + TICK;
    loop {
        let now = Instant::now();
        if now >= next_tick {
            engine.tick();
            // Skip missed ticks instead of replaying a burst after a stall.
            next_tick = (next_tick + TICK).max(now);
        }
        match receiver.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(message) => {
                if !engine.handle(message) {
                    return engine;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return engine,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use super::*;
    use crate::index::SoundIndex;
    use crate::request::SoundRequest;

    struct Empty;

    impl SoundLoader for Empty {
        fn load(&mut self, path: &str) -> io::Result<Vec<u8>> {
            Err(io::Error::new(io::ErrorKind::NotFound, path.to_owned()))
        }
    }

    #[test]
    fn the_loop_applies_messages_until_shutdown() {
        let (sender, receiver) = sync_channel(8);
        let engine = Engine::new(NullBackend::new(), Box::new(Empty), 1);
        let index = SoundIndex::parse(r#"{"a": {"sounds": ["x"]}}"#).expect("valid");
        sender
            .send(AudioMessage::Index(Box::new(index)))
            .expect("room");
        sender
            .send(AudioMessage::Play(SoundRequest::new("a")))
            .expect("room");
        sender.send(AudioMessage::Shutdown).expect("room");
        sender
            .send(AudioMessage::Play(SoundRequest::new("a")))
            .expect("room");
        let engine = run(engine, &receiver);
        // The file is missing, so nothing plays; the loop still stopped at Shutdown
        // and left the later message unread.
        assert!(engine.backend().played.is_empty());
        assert_eq!(receiver.try_iter().count(), 1);
    }

    #[test]
    fn the_loop_ends_when_every_handle_is_gone() {
        let (sender, receiver) = sync_channel::<AudioMessage>(1);
        drop(sender);
        run(
            Engine::new(NullBackend::new(), Box::new(Empty), 1),
            &receiver,
        );
    }

    #[test]
    fn the_thread_starts_serves_the_handle_and_stops() {
        let thread = spawn_with(|| Ok(NullBackend::new()), Box::new(Empty), 1).expect("spawns");
        let handle = thread.handle();
        assert!(handle.play(SoundRequest::new("a")));
        assert!(thread.stop());
        assert!(!handle.play(SoundRequest::new("a")), "the thread is gone");
    }

    #[test]
    fn a_failing_backend_ends_the_thread_quietly() {
        let thread = spawn_with::<NullBackend, _>(
            || Err(AudioError::Backend("no device".into())),
            Box::new(Empty),
            1,
        )
        .expect("spawns");
        assert!(thread.stop());
    }
}
