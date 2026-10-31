// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread publishes for the weather pass (P7f).
//!
//! The reference asks the world, on every frame, for the precipitation height,
//! the biome and the light of each column near the camera
//! (`EntityRenderer.renderRainSnow`). Here the game thread samples them once
//! per tick, and only while it rains; the render thread builds the quads from
//! the grid and never touches the world.

use std::sync::Arc;

use umicraft_mesh::climate::biome_climate;
use umicraft_render::{PrecipitationCell, PrecipitationField, FIELD_RADIUS};
use umicraft_world::{BlockPos, World};

/// The weather inputs of one frame.
#[derive(Debug, Clone, Default)]
pub struct WeatherView {
    /// `World.getRainStrength`.
    pub rain_strength: f32,
    /// `World.getThunderStrength` (already weighted by the rain).
    pub thunder_strength: f32,
    /// `World.getLastLightningBolt`.
    pub lightning_ticks: u8,
    /// The columns rain and snow fall on; empty while it does not rain.
    pub precipitation: Arc<PrecipitationField>,
}

/// Samples the columns around block column `x`, `z`: those whose biome has
/// precipitation, with the height the drops reach and the light there.
pub fn sample_precipitation(world: &World, x: i32, z: i32) -> PrecipitationField {
    let mut field = PrecipitationField::new(x, z);
    for column_z in z - FIELD_RADIUS..=z + FIELD_RADIUS {
        for column_x in x - FIELD_RADIUS..=x + FIELD_RADIUS {
            let Some(biome) = world.biome(BlockPos::new(column_x, 0, column_z)) else {
                continue;
            };
            let climate = biome_climate(biome);
            let Some(height) = world
                .precipitation_height(column_x, column_z)
                .filter(|_| climate.precipitates)
            else {
                continue;
            };
            let light = world.light(BlockPos::new(column_x, height.max(0), column_z));
            field.set(
                column_x,
                column_z,
                PrecipitationCell {
                    height,
                    temperature: climate.temperature,
                    sky_light: light.sky,
                    block_light: light.block,
                },
            );
        }
    }
    field
}

impl WeatherView {
    /// The view for a camera at block column `x`, `z`. The grid is only
    /// sampled when it rains, as `renderRainSnow` returns at once otherwise.
    pub fn new(
        weather: &umicraft_game::server::weather::Weather,
        world: Option<&World>,
        x: i32,
        z: i32,
    ) -> Self {
        let precipitation = match world {
            Some(world) if weather.rain_strength() > 0.0 => {
                Arc::new(sample_precipitation(world, x, z))
            }
            _ => Arc::default(),
        };
        Self {
            rain_strength: weather.rain_strength(),
            thunder_strength: weather.effective_thunder(),
            lightning_ticks: weather.lightning_ticks(),
            precipitation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::server::weather::Weather;
    use umicraft_protocol::packets::play::clientbound::change_game_state::ChangeGameState;
    use umicraft_world::{BlockState, ChunkPayload, ChunkPos};

    const GRASS: BlockState = BlockState::new(2, 0);

    /// One loaded column at the origin with `biome` and a grass floor at y = 63.
    fn world_with(biome: u8) -> World {
        let mut world = World::new(0);
        let mut data = vec![0u8; 8192 + 2048 + 2048 + 256];
        data[8192 + 2048 + 2048..].fill(biome);
        let payload = ChunkPayload {
            primary_bit_mask: 1,
            ground_up: true,
            data: &data,
        };
        world.apply_chunk(ChunkPos::new(0, 0), payload).unwrap();
        for x in 0..16 {
            for z in 0..16 {
                world.set_block(BlockPos::new(x, 63, z), GRASS);
            }
        }
        world
    }

    fn raining() -> Weather {
        let mut weather = Weather::default();
        weather.apply(&ChangeGameState {
            reason: 7,
            value: 1.0,
        });
        weather
    }

    #[test]
    fn the_grid_holds_loaded_wet_columns_with_their_height_and_climate() {
        let field = sample_precipitation(&world_with(1), 8, 8);
        let cell = field.get(8, 8).expect("a plains column");
        assert_eq!(cell.height, 64);
        assert_eq!(cell.temperature, 0.8);
        // The edge of the grid still is inside the loaded column or beyond it.
        assert_eq!(field.get(-1, 8), None, "an unloaded chunk has no column");
        assert!(field.get(15, 15).is_some());
    }

    #[test]
    fn deserts_have_no_precipitation_and_snow_biomes_keep_their_cold() {
        assert!(sample_precipitation(&world_with(2), 8, 8).is_empty());
        let snowy = sample_precipitation(&world_with(12), 8, 8);
        assert!(snowy.get(8, 8).expect("ice plains").temperature < 0.15);
    }

    #[test]
    fn nothing_is_sampled_while_it_does_not_rain() {
        let world = world_with(1);
        let dry = WeatherView::new(&Weather::default(), Some(&world), 8, 8);
        assert!(dry.precipitation.is_empty());
        let wet = WeatherView::new(&raining(), Some(&world), 8, 8);
        assert!(!wet.precipitation.is_empty());
        assert_eq!(wet.rain_strength, 1.0);
    }

    #[test]
    fn thunder_is_weighted_by_the_rain_and_the_flash_is_passed_on() {
        let mut weather = raining();
        weather.apply(&ChangeGameState {
            reason: 8,
            value: 0.5,
        });
        weather.tick(true);
        let view = WeatherView::new(&weather, None, 0, 0);
        assert_eq!(view.thunder_strength, 0.5);
        assert_eq!(view.lightning_ticks, 2);
        assert!(view.precipitation.is_empty());
    }
}
