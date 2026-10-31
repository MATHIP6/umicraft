// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the weather needs to know about a biome (P7f).
//!
//! Rain and snow are drawn in a column when the biome can rain or snow, and
//! the choice between the two is the temperature at the precipitation height.
//! The temperature table is the one of the biome tint ([`crate::tint`]).
//!
//! Source: MCP `BiomeGenBase.canSpawnLightningBolt` / `getEnableSnow`
//! (`enableRain`, `enableSnow`), `setDisableRain` on desert, desert hills,
//! Nether, End, savanna, savanna plateau and the three mesa biomes, and
//! `BiomeGenMutated` (id + 128), which copies the flags of its base biome.

use crate::tint::biome_temperature;

/// Ids of the biomes built with `setDisableRain`: desert, hell, sky (End),
/// desert hills, savanna, savanna plateau, mesa, mesa plateau F, mesa plateau.
const DRY: [u8; 9] = [2, 8, 9, 17, 35, 36, 37, 38, 39];

/// First id of the mutated variants (`createMutation` registers `id + 128`).
const MUTATION_OFFSET: u8 = 128;

/// The weather properties of one biome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiomeClimate {
    /// `BiomeGenBase.temperature`, before the height term of
    /// `getFloatTemperature`.
    pub temperature: f32,
    /// `canSpawnLightningBolt() || getEnableSnow()`: the biome has any
    /// precipitation. Snow biomes always do; deserts, savannas, mesas, the
    /// Nether and the End do not.
    pub precipitates: bool,
}

/// The climate of biome `id`; a mutated id (`base + 128`) takes its base
/// biome's values, an unknown id is a temperate wet biome.
///
/// ```
/// use umicraft_mesh::climate::biome_climate;
///
/// assert!(biome_climate(1).precipitates); // plains
/// assert!(!biome_climate(2).precipitates); // desert
/// assert!(!biome_climate(130).precipitates); // desert M
/// assert!(biome_climate(12).temperature < 0.15); // ice plains
/// ```
#[must_use]
pub fn biome_climate(id: u8) -> BiomeClimate {
    let base = if id >= MUTATION_OFFSET {
        id - MUTATION_OFFSET
    } else {
        id
    };
    BiomeClimate {
        temperature: biome_temperature(base),
        precipitates: !DRY.contains(&base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_biomes_do_not_precipitate() {
        for id in [2, 8, 9, 17, 35, 36, 37, 38, 39] {
            assert!(!biome_climate(id).precipitates, "{id}");
            assert!(!biome_climate(id + 128).precipitates, "{id} M");
        }
    }

    #[test]
    fn wet_and_snowy_biomes_precipitate() {
        for id in [0, 1, 3, 4, 5, 6, 10, 12, 21, 30, 140] {
            assert!(biome_climate(id).precipitates, "{id}");
        }
    }

    #[test]
    fn the_temperature_decides_between_rain_and_snow() {
        assert_eq!(biome_climate(1).temperature, 0.8);
        assert!(biome_climate(12).temperature < 0.15);
        assert!(biome_climate(30).temperature < 0.15);
        // Extreme hills are cool but still above the snow line at sea level.
        assert!(biome_climate(3).temperature >= 0.15);
    }

    #[test]
    fn a_mutation_copies_its_base() {
        assert_eq!(biome_climate(129), biome_climate(1));
        assert_eq!(biome_climate(140), biome_climate(12));
    }
}
