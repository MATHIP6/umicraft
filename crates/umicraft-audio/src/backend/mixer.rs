// SPDX-License-Identifier: MIT OR Apache-2.0

//! The software mixer: sums voices into an output buffer.
//!
//! Pure code with no device, so the real-time callback of the cpal backend is
//! a thin shell around it and the mixing is unit-tested. It never allocates
//! while rendering: the voice list is reserved up front and a voice that does
//! not fit is dropped.

use super::{Reaped, Voice};
use crate::decode::DecodedSound;

/// The most voices mixed at once: the 28 + 4 channels of vanilla (`limits`)
/// with headroom.
pub const MAX_VOICES: usize = 64;

struct Active {
    sound: DecodedSound,
    /// The read position in source frames.
    position: f64,
    /// Source frames advanced per output frame: rate ratio times pitch.
    step: f64,
    gain_left: f32,
    gain_right: f32,
    streaming: bool,
}

/// Mixes [`Voice`]s into an interleaved `f32` buffer.
pub struct Mixer {
    output_rate: u32,
    voices: Vec<Active>,
}

impl Mixer {
    /// A mixer for a device running at `output_rate` Hz.
    #[must_use]
    pub fn new(output_rate: u32) -> Self {
        Self {
            output_rate,
            voices: Vec::with_capacity(MAX_VOICES),
        }
    }

    /// Starts a voice; returns `false` (and drops it) when the mixer is full.
    pub fn add(&mut self, voice: Voice) -> bool {
        if self.voices.len() >= MAX_VOICES {
            return false;
        }
        let ratio = f64::from(voice.sound.sample_rate) / f64::from(self.output_rate);
        // A linear pan keeps the near side at full volume and fades the far one.
        let gain_left = voice.volume * (1.0 - voice.pan).min(1.0);
        let gain_right = voice.volume * (1.0 + voice.pan).min(1.0);
        self.voices.push(Active {
            sound: voice.sound,
            position: 0.0,
            step: ratio * f64::from(voice.pitch),
            gain_left,
            gain_right,
            streaming: voice.streaming,
        });
        true
    }

    /// The number of voices still playing.
    #[must_use]
    pub fn len(&self) -> usize {
        self.voices.len()
    }

    /// Whether no voice is playing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    /// Fills `out` (interleaved, `channels` per frame) with the mix and removes
    /// the voices that ran out. Channels beyond the first two stay silent.
    pub fn render(&mut self, out: &mut [f32], channels: usize) -> Reaped {
        out.fill(0.0);
        let mut reaped = Reaped::default();
        self.voices.retain_mut(|voice| {
            let alive = mix_voice(voice, out, channels);
            if !alive {
                if voice.streaming {
                    reaped.streaming += 1;
                } else {
                    reaped.normal += 1;
                }
            }
            alive
        });
        for sample in out.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
        reaped
    }
}

/// Adds one voice into `out`; `false` once its last frame was read.
fn mix_voice(voice: &mut Active, out: &mut [f32], channels: usize) -> bool {
    let frames = voice.sound.frames.len();
    for frame in out.chunks_exact_mut(channels) {
        let index = voice.position as usize;
        if index >= frames {
            return false;
        }
        let [left, right] = interpolate(&voice.sound.frames, index, voice.position.fract());
        frame[0] += left * voice.gain_left;
        if let Some(slot) = frame.get_mut(1) {
            *slot += right * voice.gain_right;
        }
        voice.position += voice.step;
    }
    (voice.position as usize) < frames
}

/// Linear interpolation between a frame and the next (the last one holds).
fn interpolate(frames: &[[f32; 2]], index: usize, fraction: f64) -> [f32; 2] {
    let a = frames[index];
    let b = frames.get(index + 1).copied().unwrap_or(a);
    let t = fraction as f32;
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(frames: Vec<[f32; 2]>, rate: u32) -> Voice {
        Voice {
            sound: DecodedSound {
                sample_rate: rate,
                frames: frames.into(),
            },
            volume: 1.0,
            pitch: 1.0,
            pan: 0.0,
            streaming: false,
        }
    }

    #[test]
    fn plays_frames_in_order_then_ends() {
        let mut mixer = Mixer::new(100);
        mixer.add(voice(vec![[0.1, 0.1], [0.2, 0.2]], 100));
        let mut out = [0.0; 8]; // four stereo frames
        let reaped = mixer.render(&mut out, 2);
        assert_eq!(out, [0.1, 0.1, 0.2, 0.2, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(reaped.normal, 1);
        assert!(mixer.is_empty());
    }

    #[test]
    fn a_long_voice_continues_across_buffers() {
        let mut mixer = Mixer::new(100);
        mixer.add(voice(vec![[0.5, 0.5]; 6], 100));
        let mut out = [0.0; 8];
        assert_eq!(mixer.render(&mut out, 2), Reaped::default());
        assert_eq!(mixer.len(), 1);
        assert_eq!(mixer.render(&mut out, 2).normal, 1);
    }

    #[test]
    fn voices_sum_and_the_output_is_clamped() {
        let mut mixer = Mixer::new(100);
        mixer.add(voice(vec![[0.8, 0.8]; 2], 100));
        mixer.add(voice(vec![[0.8, 0.8]; 2], 100));
        let mut out = [0.0; 4];
        mixer.render(&mut out, 2);
        assert_eq!(out, [1.0; 4]);
    }

    #[test]
    fn pan_and_volume_scale_the_sides() {
        let mut mixer = Mixer::new(100);
        let mut v = voice(vec![[1.0, 1.0]; 2], 100);
        v.volume = 0.5;
        v.pan = 1.0;
        mixer.add(v);
        let mut out = [0.0; 2];
        mixer.render(&mut out, 2);
        assert_eq!(out, [0.0, 0.5]);
    }

    #[test]
    fn pitch_and_rate_change_the_step() {
        let mut mixer = Mixer::new(200);
        // 100 Hz source on a 200 Hz device: each source frame lasts two output frames.
        mixer.add(voice(vec![[0.0, 0.0], [1.0, 1.0]], 100));
        let mut out = [0.0; 6];
        mixer.render(&mut out, 2);
        assert_eq!(out[..4], [0.0, 0.0, 0.5, 0.5]);
    }

    #[test]
    fn mono_devices_get_the_left_side() {
        let mut mixer = Mixer::new(100);
        mixer.add(voice(vec![[0.25, 0.75]], 100));
        let mut out = [0.0; 2];
        mixer.render(&mut out, 1);
        assert_eq!(out, [0.25, 0.0]);
    }

    #[test]
    fn a_full_mixer_drops_the_voice() {
        let mut mixer = Mixer::new(100);
        for _ in 0..MAX_VOICES {
            assert!(mixer.add(voice(vec![[0.0; 2]; 4], 100)));
        }
        assert!(!mixer.add(voice(vec![[0.0; 2]; 4], 100)));
    }
}
