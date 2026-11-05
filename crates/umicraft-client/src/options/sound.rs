// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sound volumes (`GuiScreenOptionsSounds`).
//!
//! The page edits and saves them; `crate::audio_bridge::settings` pushes them to
//! the audio thread at start-up and on every change.

use serde::{Deserialize, Serialize};

pub use umicraft_audio::SoundCategory;

/// The language key of a category label, which only the options page needs.
///
/// The enum itself lives in `umicraft-audio` (the engine and the options share
/// one type); the label is a UI concern, so it is an extension here.
pub trait CategoryLabel {
    /// The language key of the label (`soundCategory.<category name>`).
    fn lang_key(self) -> &'static str;
}

impl CategoryLabel for SoundCategory {
    fn lang_key(self) -> &'static str {
        match self {
            Self::Master => "soundCategory.master",
            Self::Music => "soundCategory.music",
            Self::Records => "soundCategory.record",
            Self::Weather => "soundCategory.weather",
            Self::Blocks => "soundCategory.block",
            Self::Mobs => "soundCategory.hostile",
            Self::Animals => "soundCategory.neutral",
            Self::Players => "soundCategory.player",
            Self::Ambient => "soundCategory.ambient",
        }
    }
}

/// The volume of every category, `0.0..=1.0`; `1.0` is the vanilla default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundOptions {
    /// Master volume.
    pub master: f32,
    /// Music volume.
    pub music: f32,
    /// Records volume.
    pub records: f32,
    /// Weather volume.
    pub weather: f32,
    /// Blocks volume.
    pub blocks: f32,
    /// Hostile creatures volume.
    pub mobs: f32,
    /// Friendly creatures volume.
    pub animals: f32,
    /// Players volume.
    pub players: f32,
    /// Ambient volume.
    pub ambient: f32,
}

impl Default for SoundOptions {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 1.0,
            records: 1.0,
            weather: 1.0,
            blocks: 1.0,
            mobs: 1.0,
            animals: 1.0,
            players: 1.0,
            ambient: 1.0,
        }
    }
}

impl SoundOptions {
    /// The volume of `category`.
    #[must_use]
    pub fn volume(&self, category: SoundCategory) -> f32 {
        *self.slot(category)
    }

    /// Sets the volume of `category`, kept in `0.0..=1.0`.
    pub fn set_volume(&mut self, category: SoundCategory, volume: f32) {
        *self.slot_mut(category) = clamp_volume(volume, 1.0);
    }

    /// Puts every volume back in its range; a value that is not a number
    /// returns to the default.
    pub(super) fn clamp(&mut self) {
        for category in SoundCategory::ALL {
            let slot = self.slot_mut(category);
            *slot = clamp_volume(*slot, 1.0);
        }
    }

    fn slot(&self, category: SoundCategory) -> &f32 {
        match category {
            SoundCategory::Master => &self.master,
            SoundCategory::Music => &self.music,
            SoundCategory::Records => &self.records,
            SoundCategory::Weather => &self.weather,
            SoundCategory::Blocks => &self.blocks,
            SoundCategory::Mobs => &self.mobs,
            SoundCategory::Animals => &self.animals,
            SoundCategory::Players => &self.players,
            SoundCategory::Ambient => &self.ambient,
        }
    }

    fn slot_mut(&mut self, category: SoundCategory) -> &mut f32 {
        match category {
            SoundCategory::Master => &mut self.master,
            SoundCategory::Music => &mut self.music,
            SoundCategory::Records => &mut self.records,
            SoundCategory::Weather => &mut self.weather,
            SoundCategory::Blocks => &mut self.blocks,
            SoundCategory::Mobs => &mut self.mobs,
            SoundCategory::Animals => &mut self.animals,
            SoundCategory::Players => &mut self.players,
            SoundCategory::Ambient => &mut self.ambient,
        }
    }
}

/// `value` in `0.0..=1.0`, or `default` when it is not a number.
fn clamp_volume(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_starts_at_full_volume() {
        let sound = SoundOptions::default();
        for category in SoundCategory::ALL {
            assert_eq!(sound.volume(category), 1.0, "{category:?}");
        }
    }

    #[test]
    fn a_category_is_set_without_touching_the_others() {
        for category in SoundCategory::ALL {
            let mut sound = SoundOptions::default();
            sound.set_volume(category, 0.25);
            for other in SoundCategory::ALL {
                let expected = if other == category { 0.25 } else { 1.0 };
                assert_eq!(sound.volume(other), expected, "{category:?}/{other:?}");
            }
        }
    }

    #[test]
    fn volumes_are_clamped_and_a_nan_returns_to_full() {
        let mut sound = SoundOptions::default();
        sound.set_volume(SoundCategory::Music, 7.0);
        assert_eq!(sound.music, 1.0);
        sound.set_volume(SoundCategory::Music, -1.0);
        assert_eq!(sound.music, 0.0);
        sound.records = f32::NAN;
        sound.weather = 2.0;
        sound.clamp();
        assert_eq!((sound.records, sound.weather), (1.0, 1.0));
    }

    #[test]
    fn the_labels_use_the_vanilla_category_names() {
        let keys: Vec<_> = SoundCategory::ALL.map(SoundCategory::lang_key).to_vec();
        assert_eq!(
            keys,
            [
                "soundCategory.master",
                "soundCategory.music",
                "soundCategory.record",
                "soundCategory.weather",
                "soundCategory.block",
                "soundCategory.hostile",
                "soundCategory.neutral",
                "soundCategory.player",
                "soundCategory.ambient",
            ]
        );
    }
}
