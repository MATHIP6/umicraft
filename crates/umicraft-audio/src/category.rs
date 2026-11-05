// SPDX-License-Identifier: MIT OR Apache-2.0

//! The nine sound categories and their volume table.
//!
//! Mirrors MCP `SoundCategory` and `SoundManager.getSoundCategoryVolume`. The
//! client keeps its own copy of the enum for the options page (it must not
//! depend on this crate yet); the two are joined in a later P11 lot.

use std::str::FromStr;

/// A volume category of MCP `SoundCategory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundCategory {
    /// The master volume, applied to every other category.
    Master,
    /// Music.
    Music,
    /// Jukebox and note block records.
    Records,
    /// Rain and thunder.
    Weather,
    /// Blocks.
    Blocks,
    /// Hostile creatures.
    Mobs,
    /// Friendly and neutral creatures.
    Animals,
    /// Players.
    Players,
    /// Ambient and environment sounds.
    Ambient,
}

impl SoundCategory {
    /// Every category, in the id order of MCP (`Master` first).
    pub const ALL: [Self; 9] = [
        Self::Master,
        Self::Music,
        Self::Records,
        Self::Weather,
        Self::Blocks,
        Self::Mobs,
        Self::Animals,
        Self::Players,
        Self::Ambient,
    ];

    /// The name used by `sounds.json` (`SoundCategory.getCategoryName`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Music => "music",
            Self::Records => "record",
            Self::Weather => "weather",
            Self::Blocks => "block",
            Self::Mobs => "hostile",
            Self::Animals => "neutral",
            Self::Players => "player",
            Self::Ambient => "ambient",
        }
    }

    /// The index of this category in [`SoundCategory::ALL`] and in a [`CategoryVolumes`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// The error of parsing an unknown category name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCategory;

impl FromStr for SoundCategory {
    type Err = UnknownCategory;

    /// Parses a `sounds.json` category name (`SoundCategory.getCategory`).
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|category| category.name() == name)
            .ok_or(UnknownCategory)
    }
}

/// The volume of every category, each `0.0..=1.0`; `1.0` is the vanilla default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CategoryVolumes([f32; 9]);

impl Default for CategoryVolumes {
    fn default() -> Self {
        Self([1.0; 9])
    }
}

impl CategoryVolumes {
    /// Sets the volume of `category`, clamped to `0.0..=1.0`.
    pub fn set(&mut self, category: SoundCategory, volume: f32) {
        self.0[category.index()] = volume.clamp(0.0, 1.0);
    }

    /// The own volume of `category`.
    #[must_use]
    pub fn get(&self, category: SoundCategory) -> f32 {
        self.0[category.index()]
    }

    /// The volume a sound of `category` plays at: the master volume is applied
    /// to every category but the master itself (`SoundManager` sets the master
    /// volume on the sound system and the others per channel).
    #[must_use]
    pub fn effective(&self, category: SoundCategory) -> f32 {
        match category {
            SoundCategory::Master => self.get(SoundCategory::Master),
            other => self.get(other) * self.get(SoundCategory::Master),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for category in SoundCategory::ALL {
            assert_eq!(category.name().parse(), Ok(category));
            assert_eq!(SoundCategory::ALL[category.index()], category);
        }
        assert_eq!("nope".parse::<SoundCategory>(), Err(UnknownCategory));
    }

    #[test]
    fn master_scales_the_others() {
        let mut volumes = CategoryVolumes::default();
        volumes.set(SoundCategory::Master, 0.5);
        volumes.set(SoundCategory::Blocks, 0.5);
        assert_eq!(volumes.effective(SoundCategory::Blocks), 0.25);
        assert_eq!(volumes.effective(SoundCategory::Master), 0.5);
    }

    #[test]
    fn set_clamps() {
        let mut volumes = CategoryVolumes::default();
        volumes.set(SoundCategory::Music, 3.0);
        assert_eq!(volumes.get(SoundCategory::Music), 1.0);
        volumes.set(SoundCategory::Music, -1.0);
        assert_eq!(volumes.get(SoundCategory::Music), 0.0);
    }
}
