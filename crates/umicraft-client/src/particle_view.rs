// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread publishes for the particle pass (P8).
//!
//! The simulation lives in [`umicraft_game::particles`]; each tick the game
//! thread copies the live particles with the light where they stand, so the
//! render thread interpolates, scales and tints them with its own partial tick
//! and never reads the world (as for the weather, ADR-0031).

use umicraft_game::particles::{Behaviour, Layer, Particle, ParticleEngine, Texture};
use umicraft_render::ParticleSprite;
use umicraft_world::{BlockPos, World};

/// Cells per side of `particles.png`.
const SHEET_GRID: f32 = 16.0;
/// The share of a cell a picture covers (`0.0624375` in `renderParticle`,
/// just under 1/16 so neighbours do not bleed in).
const CELL_SPAN: f32 = 0.062_437_5;

/// A particle of the newest tick with the light at its position.
#[derive(Debug, Clone)]
pub struct ParticleDraw {
    /// The simulation state after the tick.
    pub particle: Particle,
    /// Block light at the particle, `0..=15`.
    pub block_light: u8,
    /// Sky light at the particle, `0..=15`.
    pub sky_light: u8,
}

/// Copies the live particles of `engine`, in the reference's draw order
/// (layer by layer, the translucent ones first), with their light.
#[must_use]
pub fn collect(engine: &ParticleEngine, world: Option<&World>) -> Vec<ParticleDraw> {
    let mut draws = Vec::with_capacity(engine.len());
    for layer in Layer::ALL {
        for translucent in [true, false] {
            for particle in engine.particles(layer, translucent) {
                let light = world.map_or(umicraft_world::LightLevel::FULL_SKY, |world| {
                    world.light(light_position(particle))
                });
                draws.push(ParticleDraw {
                    particle: particle.clone(),
                    block_light: light.block,
                    sky_light: light.sky,
                });
            }
        }
    }
    draws
}

/// The block whose light a particle uses: the one at its eye, half a box above
/// its position (`Entity.getBrightnessForRender` adds `getEyeHeight`).
fn light_position(particle: &Particle) -> BlockPos {
    BlockPos::new(
        particle.position.x.floor() as i32,
        (particle.position.y + particle.size / 2.0).floor() as i32,
        particle.position.z.floor() as i32,
    )
}

/// The quad to draw for `draw` at `partial_ticks` into the tick; `None` when
/// its picture is not available yet (the sprites of the block and item
/// atlases and the pictures beside `particles.png`).
#[must_use]
pub fn sprite(draw: &ParticleDraw, partial_ticks: f32) -> Option<ParticleSprite> {
    let particle = &draw.particle;
    let Texture::Cell { index } = particle.texture else {
        return None;
    };
    let u0 = f32::from(index % 16) / SHEET_GRID;
    let v0 = f32::from(index / 16) / SHEET_GRID;
    let (block, sky) = particle.light(
        draw.block_light.saturating_mul(16),
        draw.sky_light.saturating_mul(16),
        partial_ticks,
    );
    let [red, green, blue] = particle.color;
    let behaviour = particle.behaviour;
    Some(ParticleSprite {
        center: particle.interpolated_position(partial_ticks).as_vec3(),
        half_size: 0.1 * behaviour.render_scale(particle, partial_ticks),
        uv_min: [u0, v0],
        uv_max: [u0 + CELL_SPAN, v0 + CELL_SPAN],
        atlas_layer: None,
        color: [
            red,
            green,
            blue,
            behaviour.render_alpha(particle, partial_ticks),
        ],
        light: [f32::from(block) / 16.0, f32::from(sky) / 16.0],
        flat: matches!(behaviour, Behaviour::Footstep),
    })
}

/// The quads of every particle that can be drawn, for `Renderer::set_particles`.
#[must_use]
pub fn sprites(draws: &[ParticleDraw], partial_ticks: f32) -> Vec<ParticleSprite> {
    draws
        .iter()
        .filter_map(|draw| sprite(draw, partial_ticks))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use umicraft_game::particles::{create, ParticleKind};

    fn flame() -> Particle {
        let mut engine = ParticleEngine::new(1);
        create(
            ParticleKind::Flame,
            DVec3::new(0.5, 64.0, 0.5),
            DVec3::ZERO,
            engine.rng(),
        )
        .expect("flame is ported")
    }

    #[test]
    fn a_sheet_particle_becomes_a_quad_in_its_cell() {
        let mut particle = flame();
        particle.texture = Texture::Cell { index: 0x23 };
        let draw = ParticleDraw {
            particle,
            block_light: 3,
            sky_light: 15,
        };
        let sprite = sprite(&draw, 0.5).expect("a sheet cell");
        assert_eq!(sprite.uv_min, [3.0 / 16.0, 2.0 / 16.0]);
        assert!(sprite.atlas_layer.is_none() && !sprite.flat);
        assert!(sprite.half_size > 0.0);
    }

    #[test]
    fn a_particle_with_a_named_sprite_waits_for_its_picture() {
        let mut particle = flame();
        particle.texture = Texture::Sprite {
            name: "blocks/stone",
        };
        let draw = ParticleDraw {
            particle,
            block_light: 0,
            sky_light: 15,
        };
        assert!(sprite(&draw, 0.0).is_none());
    }

    #[test]
    fn collecting_without_a_world_uses_full_sky_light() {
        let mut engine = ParticleEngine::new(1);
        let particle = flame();
        engine.add(particle);
        let draws = collect(&engine, None);
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].sky_light, 15);
    }
}
