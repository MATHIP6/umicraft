// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vanilla distance rule.
//!
//! `SoundManager.playSound` creates every positioned source with an
//! attenuation distance of `16`, multiplied by the sound volume when that is
//! above `1` (`float f1 = 16.0F; if (f > 1.0F) f1 *= f;`) and the LINEAR model
//! (`ISound.AttenuationType.LINEAR`). The sound library (paulscode on OpenAL)
//! applies the OpenAL linear-distance-clamped model with a reference distance
//! of `0`, which is `gain = 1 - distance / max_distance` clamped to `0..=1`
//! (OpenAL 1.1 specification, section 3.4.2).

/// The distance in blocks beyond which a sound of `volume` is silent.
#[must_use]
pub fn attenuation_distance(volume: f32) -> f32 {
    16.0 * volume.max(1.0)
}

/// The gain, `0.0..=1.0`, of a source `distance` blocks from the listener.
///
/// `volume` is the `ISound` volume (not the normalised one): it only widens
/// the audible distance, its loudness is applied separately.
#[must_use]
pub fn distance_gain(distance: f64, volume: f32) -> f32 {
    let max = f64::from(attenuation_distance(volume));
    // Narrowing to f32 is fine: the value is within 0..=1.
    (1.0 - distance / max).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_sixteen_blocks_at_normal_volume() {
        assert_eq!(attenuation_distance(1.0), 16.0);
        assert_eq!(attenuation_distance(0.25), 16.0);
    }

    #[test]
    fn loud_sounds_carry_further() {
        assert_eq!(attenuation_distance(4.0), 64.0);
    }

    #[test]
    fn gain_falls_linearly() {
        assert_eq!(distance_gain(0.0, 1.0), 1.0);
        assert_eq!(distance_gain(8.0, 1.0), 0.5);
        assert_eq!(distance_gain(16.0, 1.0), 0.0);
        assert_eq!(distance_gain(100.0, 1.0), 0.0);
    }

    #[test]
    fn gain_scales_with_the_wider_distance() {
        assert_eq!(distance_gain(32.0, 4.0), 0.5);
    }
}
