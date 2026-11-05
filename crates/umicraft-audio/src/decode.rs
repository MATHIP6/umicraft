// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ogg Vorbis to PCM.
//!
//! A pure function on bytes, so the audio thread can decode on a cache miss and
//! tests can decode a fixture. Vanilla decodes with JOrbis (`CodecJOrbis` in
//! `SoundManager`); `lewton` is the pure-Rust equivalent (ADR-0050).

use std::io::Cursor;
use std::sync::Arc;

use lewton::inside_ogg::OggStreamReader;

use crate::error::AudioError;

/// A decoded sound: stereo frames at the sample rate of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSound {
    /// The sample rate in Hz.
    pub sample_rate: u32,
    /// Left/right samples in `-1.0..=1.0`. Shared, so a cached sound is cheap
    /// to hand to several voices.
    pub frames: Arc<[[f32; 2]]>,
}

impl DecodedSound {
    /// The size of the sample data in bytes, the unit the cache is bounded in.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.frames.len() * std::mem::size_of::<[f32; 2]>()
    }

    /// The length in seconds.
    #[must_use]
    pub fn duration_secs(&self) -> f32 {
        self.frames.len() as f32 / self.sample_rate as f32
    }
}

/// Decodes a whole Ogg Vorbis file.
///
/// Mono is played on both sides; with more than two channels only the first
/// two are kept (vanilla sounds are mono or stereo).
///
/// # Errors
///
/// [`AudioError::Decode`] when the bytes are not a valid Vorbis stream and
/// [`AudioError::EmptySound`] when it holds no frame.
pub fn decode_ogg(bytes: &[u8]) -> Result<DecodedSound, AudioError> {
    let mut reader =
        OggStreamReader::new(Cursor::new(bytes)).map_err(|e| AudioError::Decode(e.to_string()))?;
    let channels = usize::from(reader.ident_hdr.audio_channels);
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let mut frames = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| AudioError::Decode(e.to_string()))?
    {
        frames.extend(packet.chunks_exact(channels).map(to_frame));
    }
    if frames.is_empty() {
        return Err(AudioError::EmptySound);
    }
    Ok(DecodedSound {
        sample_rate,
        frames: frames.into(),
    })
}

fn to_frame(channels: &[i16]) -> [f32; 2] {
    let left = sample(channels[0]);
    let right = channels.get(1).map_or(left, |s| sample(*s));
    [left, right]
}

fn sample(value: i16) -> f32 {
    f32::from(value) / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An original 0.1 s 440 Hz sine, mono, 22.05 kHz, made with ffmpeg
    /// (`ffmpeg -f lavfi -i sine=frequency=440:duration=0.1:sample_rate=22050 -c:a libvorbis -q:a 0`).
    const TONE: &[u8] = include_bytes!("../tests/fixtures/tone_440hz_mono.ogg");

    #[test]
    fn decodes_the_fixture() {
        let sound = decode_ogg(TONE).expect("fixture decodes");
        assert_eq!(sound.sample_rate, 22_050);
        assert!(
            (2_000..3_000).contains(&sound.frames.len()),
            "{}",
            sound.frames.len()
        );
        assert!((sound.duration_secs() - 0.1).abs() < 0.02);
        assert_eq!(sound.byte_size(), sound.frames.len() * 8);
    }

    #[test]
    fn mono_fills_both_sides_and_has_signal() {
        let sound = decode_ogg(TONE).expect("fixture decodes");
        assert!(sound.frames.iter().all(|[l, r]| l == r));
        let peak = sound
            .frames
            .iter()
            .fold(0.0_f32, |p, [l, _]| p.max(l.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
    }

    #[test]
    fn rejects_garbage_and_truncated_files() {
        assert!(matches!(decode_ogg(b"not ogg"), Err(AudioError::Decode(_))));
        assert!(decode_ogg(&TONE[..40]).is_err());
        assert!(decode_ogg(&[]).is_err());
    }
}
