// SPDX-License-Identifier: MIT OR Apache-2.0

//! Decoding of streamed sounds (the music) off the audio thread.
//!
//! A music track is 3-4 minutes of Ogg Vorbis. Decoding it whole takes tens of
//! milliseconds in a release build and far longer in a debug build; on the
//! audio thread that would hold back every footstep and click queued behind it
//! (ADR-0052). So the [`Engine`](crate::engine::Engine) reads the file bytes,
//! hands them to this worker thread, and starts the voice on a later tick when
//! the decoded PCM comes back through a channel it only polls.

use std::io;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{Builder, JoinHandle};

use crate::backend::Voice;
use crate::decode::{decode_ogg, DecodedSound};
use crate::error::AudioError;

/// How a sound is to be played, fixed when it was requested.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayParams {
    /// The final loudness, as in [`Voice::volume`](crate::backend::Voice::volume).
    pub volume: f32,
    /// The playback speed.
    pub pitch: f32,
    /// The stereo position.
    pub pan: f32,
    /// The stop generation the request was made in; a stop since then cancels a
    /// stream that was still decoding.
    pub generation: u32,
}

impl PlayParams {
    /// The voice that plays `sound` with these parameters.
    #[must_use]
    pub fn into_voice(self, sound: DecodedSound, streaming: bool) -> Voice {
        Voice {
            sound,
            volume: self.volume,
            pitch: self.pitch,
            pan: self.pan,
            streaming,
        }
    }
}

/// A finished decode.
#[derive(Debug)]
pub struct Decoded {
    /// The PCM, or why the file could not be decoded.
    pub sound: Result<DecodedSound, AudioError>,
    /// How to play it.
    pub params: PlayParams,
}

struct Job {
    bytes: Vec<u8>,
    params: PlayParams,
}

struct Worker {
    jobs: Sender<Job>,
    results: Receiver<Decoded>,
    // Detached on drop: the thread ends when `jobs` is gone, after the decode
    // in progress. Joining would make the audio thread wait for it.
    _thread: JoinHandle<()>,
}

/// A lazily started decoding thread.
#[derive(Default)]
pub struct StreamDecoder {
    worker: Option<Worker>,
}

impl StreamDecoder {
    /// Queues `bytes` for decoding; never waits for the decode.
    ///
    /// # Errors
    ///
    /// The OS refused to start the worker, or it is gone.
    pub fn submit(&mut self, bytes: Vec<u8>, params: PlayParams) -> io::Result<()> {
        let worker = match &mut self.worker {
            Some(worker) => worker,
            empty => empty.insert(start_worker()?),
        };
        worker
            .jobs
            .send(Job { bytes, params })
            .map_err(|_| io::Error::other("the decoder thread ended"))
    }

    /// The next finished decode, if any; never waits.
    pub fn poll(&mut self) -> Option<Decoded> {
        self.worker.as_ref()?.results.try_recv().ok()
    }
}

fn start_worker() -> io::Result<Worker> {
    let (jobs, incoming) = channel::<Job>();
    let (done, results) = channel();
    let thread = Builder::new()
        .name("umicraft-audio-decode".into())
        .spawn(move || {
            for job in incoming {
                let decoded = Decoded {
                    sound: decode_ogg(&job.bytes),
                    params: job.params,
                };
                if done.send(decoded).is_err() {
                    return; // the engine is gone
                }
            }
        })?;
    Ok(Worker {
        jobs,
        results,
        _thread: thread,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    const TONE: &[u8] = include_bytes!("../tests/fixtures/tone_440hz_mono.ogg");

    fn params(generation: u32) -> PlayParams {
        PlayParams {
            volume: 1.0,
            pitch: 1.0,
            pan: 0.0,
            generation,
        }
    }

    fn wait(decoder: &mut StreamDecoder) -> Decoded {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(done) = decoder.poll() {
                return done;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("the decode did not finish");
    }

    #[test]
    fn decodes_in_order_and_keeps_the_params() {
        let mut decoder = StreamDecoder::default();
        assert!(decoder.poll().is_none(), "no worker, nothing to poll");
        decoder.submit(TONE.to_vec(), params(1)).expect("submits");
        decoder
            .submit(b"garbage".to_vec(), params(2))
            .expect("submits");
        let first = wait(&mut decoder);
        assert!(first.sound.is_ok());
        assert_eq!(first.params.generation, 1);
        let second = wait(&mut decoder);
        assert!(second.sound.is_err());
        assert_eq!(second.params.generation, 2);
    }
}
