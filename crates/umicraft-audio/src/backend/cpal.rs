// SPDX-License-Identifier: MIT OR Apache-2.0

//! The real backend: the [`Mixer`] driven by a `cpal` output stream.
//!
//! cpal calls the data callback on its own realtime thread. The audio thread
//! hands voices to it through a bounded channel and reads how many ended from
//! atomics, so neither side ever waits for the other.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

use super::mixer::{Mixer, MAX_VOICES};
use super::{AudioBackend, Reaped, Voice};
use crate::error::AudioError;

/// Voices that ended, written by the stream callback and read by `reap`.
#[derive(Default)]
struct Ended {
    normal: AtomicUsize,
    streaming: AtomicUsize,
}

/// Plays voices on the default output device.
pub struct CpalBackend {
    voices: SyncSender<Voice>,
    ended: Arc<Ended>,
    /// Set by `stop_streaming`; the callback ends the streaming voices.
    stop_streams: Arc<AtomicBool>,
    /// Dropping the stream stops the device, so it is kept alive here.
    _stream: Stream,
}

impl CpalBackend {
    /// Opens the default output device at its default configuration.
    ///
    /// # Errors
    ///
    /// [`AudioError::Backend`] when there is no device, it cannot be queried, or
    /// the stream cannot be built or started.
    pub fn new() -> Result<Self, AudioError> {
        let backend_error = |e: cpal::Error| AudioError::Backend(e.to_string());
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| AudioError::Backend("no output device".into()))?;
        let config: cpal::StreamConfig = device
            .default_output_config()
            .map_err(backend_error)?
            .into();
        let channels = usize::from(config.channels);
        let mut mixer = Mixer::new(config.sample_rate);
        let (voices, incoming) = sync_channel::<Voice>(MAX_VOICES);
        let ended = Arc::new(Ended::default());
        let ended_in_callback = Arc::clone(&ended);
        let stop_streams = Arc::new(AtomicBool::new(false));
        let stop_in_callback = Arc::clone(&stop_streams);
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    while let Ok(voice) = incoming.try_recv() {
                        mixer.add(voice);
                    }
                    // After the drain: a voice queued before the stop is stopped too.
                    let stopped = if stop_in_callback.swap(false, Ordering::AcqRel) {
                        mixer.stop_streaming().streaming
                    } else {
                        0
                    };
                    let reaped = mixer.render(data, channels);
                    ended_in_callback
                        .normal
                        .fetch_add(reaped.normal, Ordering::Relaxed);
                    ended_in_callback
                        .streaming
                        .fetch_add(reaped.streaming + stopped, Ordering::Relaxed);
                },
                |error| tracing::warn!(%error, "audio stream error"),
                None,
            )
            .map_err(backend_error)?;
        stream.play().map_err(backend_error)?;
        Ok(Self {
            voices,
            ended,
            stop_streams,
            _stream: stream,
        })
    }
}

impl AudioBackend for CpalBackend {
    fn play(&mut self, voice: Voice) -> Result<(), AudioError> {
        self.voices
            .try_send(voice)
            .map_err(|_| AudioError::Backend("the mixer queue is full or closed".into()))
    }

    fn reap(&mut self) -> Reaped {
        Reaped {
            normal: self.ended.normal.swap(0, Ordering::Relaxed),
            streaming: self.ended.streaming.swap(0, Ordering::Relaxed),
        }
    }

    fn stop_streaming(&mut self) {
        self.stop_streams.store(true, Ordering::Release);
    }
}
