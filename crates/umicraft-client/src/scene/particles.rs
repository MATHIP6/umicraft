// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread publishes for the particle pass (P8).
//!
//! The simulation lives in [`umicraft_game::particles`]; each tick the game
//! thread copies the live particles with the light where they stand, so the
//! render thread interpolates, scales and tints them with its own partial tick
//! and never reads the world (as for the weather, ADR-0031).

use glam::DVec3;
use umicraft_game::particles::behaviours::{
    large_explode_quad, FOOTPRINT_TEXTURE, LARGE_EXPLODE_FRAME_SPAN, LARGE_EXPLODE_TEXTURE,
};
use umicraft_game::particles::{Behaviour, Layer, Particle, ParticleEngine, Texture};
use umicraft_mesh::tint::{fixed_color, tint_class};
use umicraft_mesh::ModelMeshContext;
use umicraft_render::{ParticleSheet, ParticleSprite};
use umicraft_world::{BlockPos, BlockState, World};

/// Cells per side of `particles.png`.
const SHEET_GRID: f32 = 16.0;
/// The share of a cell a picture covers (`0.0624375` in `renderParticle`,
/// just under 1/16 so neighbours do not bleed in).
const CELL_SPAN: f32 = 0.062_437_5;

/// The plains biome, whose colours tint the fragments of leaves and plants:
/// the game thread has no biome at hand for a fragment (see
/// `docs/tasks/p8/README.md`).
const TINT_BIOME: u8 = 1;
/// The grass block's id: its fragments are never tinted (`EntityDiggingFX`).
const GRASS_BLOCK: u16 = 2;

/// The picture of a block for its fragments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockSprite {
    /// The layer of the block atlas.
    pub layer: u32,
    /// The colour multiplier of the block.
    pub tint: [f32; 3],
}

/// The particle picture of `state`: the `particle` texture of its first model
/// variant (`BlockModelShapes.getTexture`) and its tint.
#[must_use]
pub fn block_sprite(context: &ModelMeshContext, state: BlockState) -> Option<BlockSprite> {
    let model = context.models.model(state)?;
    let key = model.variants.first()?.model.particle.as_deref()?;
    let layer = context.atlas.layer(key)?;
    let rgb = match (fixed_color(state), &context.tints, state.id()) {
        (Some(fixed), _, _) => fixed,
        (None, Some(tints), id) if id != GRASS_BLOCK => tints.color(tint_class(state), TINT_BIOME),
        _ => [0xFF; 3],
    };
    let tint = rgb.map(|channel| f32::from(channel) / 255.0);
    Some(BlockSprite { layer, tint })
}

/// Where the pictures of the particles are: `blocks` finds the picture of a
/// block, `items` the atlas layer of an item's particle picture.
pub struct Pictures<B, I> {
    /// The picture of a block for its fragments.
    pub blocks: B,
    /// The atlas layer of the particle picture of an item (`id`, `damage`).
    pub items: I,
}

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
/// its picture is not available (an item sprite, or a picture beside
/// `particles.png` that is not drawn yet). `blocks` finds the picture of a
/// block.
#[must_use]
pub fn sprite(
    draw: &ParticleDraw,
    partial_ticks: f32,
    origin: DVec3,
    pictures: &Pictures<
        impl Fn(BlockState) -> Option<BlockSprite>,
        impl Fn(u16, u16) -> Option<u32>,
    >,
) -> Option<ParticleSprite> {
    let particle = &draw.particle;
    let (uv_min, uv_max, atlas_layer, tint) = match particle.texture {
        Texture::Cell { index } => {
            let u0 = f32::from(index % 16) / SHEET_GRID;
            let v0 = f32::from(index / 16) / SHEET_GRID;
            ([u0, v0], [u0 + CELL_SPAN, v0 + CELL_SPAN], None, [1.0; 3])
        }
        Texture::Block { state } => {
            let block = (pictures.blocks)(state)?;
            // A fragment shows a quarter of the sprite, picked by its jitter
            // (`EntityDiggingFX.renderParticle`).
            let u0 = particle.jitter_u / 4.0;
            let v0 = particle.jitter_v / 4.0;
            (
                [u0, v0],
                [u0 + 0.25, v0 + 0.25],
                Some(block.layer),
                block.tint,
            )
        }
        Texture::Item { id, damage } => {
            // Like a fragment, an item piece shows a quarter of its picture
            // (`EntityBreakingFX.renderParticle`), untinted.
            let layer = (pictures.items)(id, damage)?;
            let u0 = particle.jitter_u / 4.0;
            let v0 = particle.jitter_v / 4.0;
            ([u0, v0], [u0 + 0.25, v0 + 0.25], Some(layer), [1.0; 3])
        }
        Texture::Region { min, max } => (min, max, None, [1.0; 3]),
        Texture::Sprite { name } => return named_sprite(draw, name, partial_ticks, origin),
    };
    let (block, sky) = particle.light(
        draw.block_light.saturating_mul(16),
        draw.sky_light.saturating_mul(16),
        partial_ticks,
    );
    let [red, green, blue] = particle.color;
    let behaviour = particle.behaviour;
    Some(ParticleSprite {
        sheet: ParticleSheet::Particles,
        // Subtracted in f64: an absolute f32 centre jitters far from the
        // world origin (F2-1b).
        center: (particle.interpolated_position(partial_ticks) - origin).as_vec3(),
        half_size: 0.1 * behaviour.render_scale(particle, partial_ticks),
        uv_min,
        uv_max,
        atlas_layer,
        color: [
            red * tint[0],
            green * tint[1],
            blue * tint[2],
            behaviour.render_alpha(particle, partial_ticks),
        ],
        light: [f32::from(block) / 16.0, f32::from(sky) / 16.0],
        flat: false,
    })
}

/// The quad of a particle that brings its own picture beside `particles.png`:
/// the footprint (`EntityFootStepFX`) and the large explosion
/// (`EntityLargeExplodeFX`). `None` for any other name, and for an explosion
/// past its 16th frame.
fn named_sprite(
    draw: &ParticleDraw,
    name: &str,
    partial_ticks: f32,
    origin: DVec3,
) -> Option<ParticleSprite> {
    let particle = &draw.particle;
    let center = (particle.interpolated_position(partial_ticks) - origin).as_vec3();
    let [red, green, blue] = particle.color;
    match name {
        FOOTPRINT_TEXTURE => {
            let (block, sky) = particle.light(
                draw.block_light.saturating_mul(16),
                draw.sky_light.saturating_mul(16),
                partial_ticks,
            );
            Some(ParticleSprite {
                center,
                half_size: 0.1 * particle.behaviour.render_scale(particle, partial_ticks),
                uv_min: [0.0, 0.0],
                uv_max: [1.0, 1.0],
                atlas_layer: None,
                sheet: ParticleSheet::Footprint,
                color: [
                    red,
                    green,
                    blue,
                    particle.behaviour.render_alpha(particle, partial_ticks),
                ],
                light: [f32::from(block) / 16.0, f32::from(sky) / 16.0],
                flat: true,
            })
        }
        LARGE_EXPLODE_TEXTURE => {
            let frame = large_explode_quad(particle, partial_ticks)?;
            Some(ParticleSprite {
                center,
                half_size: frame.half_extent,
                uv_min: [frame.u, frame.v],
                uv_max: [
                    frame.u + LARGE_EXPLODE_FRAME_SPAN,
                    frame.v + LARGE_EXPLODE_FRAME_SPAN,
                ],
                atlas_layer: None,
                sheet: ParticleSheet::Explosion,
                color: [red, green, blue, 1.0],
                // `lightmap(0, 240)`: no block light, full sky light.
                light: [0.0, 15.0],
                flat: false,
            })
        }
        _ => None,
    }
}

/// How far through its life the elder guardian apparition is among `draws`,
/// `0..=1` (MCP `MobAppearance.renderParticle`: `(age + partial) / maxAge`);
/// `None` while there is none.
#[must_use]
pub fn mob_appearance(draws: &[ParticleDraw], partial_ticks: f32) -> Option<f32> {
    let particle = draws
        .iter()
        .map(|draw| &draw.particle)
        .find(|particle| particle.behaviour == Behaviour::MobAppearance)?;
    Some(((particle.age as f32 + partial_ticks) / particle.max_age as f32).min(1.0))
}

/// Fills `out` with the quads of every particle that can be drawn, for
/// `Renderer::set_particles`. `out` keeps its capacity from frame to frame
/// (F2-5c), so a steady scene allocates nothing here.
pub fn sprites_into(
    out: &mut Vec<ParticleSprite>,
    draws: &[ParticleDraw],
    partial_ticks: f32,
    origin: DVec3,
    pictures: &Pictures<
        impl Fn(BlockState) -> Option<BlockSprite>,
        impl Fn(u16, u16) -> Option<u32>,
    >,
) {
    out.clear();
    out.extend(
        draws
            .iter()
            .filter_map(|draw| sprite(draw, partial_ticks, origin, pictures)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::particles::{create, ParticleKind};

    fn no_block(_: BlockState) -> Option<BlockSprite> {
        None
    }

    fn no_item(_: u16, _: u16) -> Option<u32> {
        None
    }

    type NoPictures = Pictures<fn(BlockState) -> Option<BlockSprite>, fn(u16, u16) -> Option<u32>>;

    fn no_pictures() -> NoPictures {
        Pictures {
            blocks: no_block,
            items: no_item,
        }
    }

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
    fn a_particle_far_from_the_origin_is_placed_relative_to_the_render_origin() {
        // F2-1b: the centre is subtracted in f64, so a 1 cm step survives at
        // x = 1 000 000 where an absolute f32 centre has a 6 cm grain.
        let origin = DVec3::new(1_000_000.0, 64.0, 1_000_000.0);
        let mut engine = ParticleEngine::new(1);
        let mut at = |x: f64| {
            let mut particle = create(
                ParticleKind::Flame,
                DVec3::new(1_000_000.0 + x, 64.5, 1_000_000.5),
                DVec3::ZERO,
                engine.rng(),
            )
            .expect("flame is ported");
            particle.texture = Texture::Cell { index: 0 };
            ParticleDraw {
                particle,
                block_light: 0,
                sky_light: 15,
            }
        };
        let centre = |draw: &ParticleDraw| {
            sprite(draw, 0.0, origin, &no_pictures())
                .expect("a sheet cell")
                .center
        };
        let first = centre(&at(0.5));
        let second = centre(&at(0.51));
        assert!((first.x - 0.5).abs() < 1e-6, "{first}");
        assert!((second.x - first.x - 0.01).abs() < 1e-5, "{second}");
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
        let sprite = sprite(&draw, 0.5, DVec3::ZERO, &no_pictures()).expect("a sheet cell");
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
        assert!(sprite(&draw, 0.0, DVec3::ZERO, &no_pictures()).is_none());
    }

    fn created(kind: ParticleKind) -> ParticleDraw {
        let mut engine = ParticleEngine::new(1);
        let particle = create(kind, DVec3::new(2.0, 64.0, 2.0), DVec3::ZERO, engine.rng())
            .expect("the kind is ported");
        ParticleDraw {
            particle,
            block_light: 0,
            sky_light: 15,
        }
    }

    #[test]
    fn a_footprint_is_a_flat_quarter_block_with_the_whole_picture() {
        let draw = created(ParticleKind::Footstep);
        let sprite = sprite(&draw, 0.0, DVec3::ZERO, &no_pictures()).expect("a footprint");
        assert_eq!(sprite.sheet, ParticleSheet::Footprint);
        assert!(sprite.flat);
        assert_eq!((sprite.uv_min, sprite.uv_max), ([0.0, 0.0], [1.0, 1.0]));
        // MCP `EntityFootStepFX`: half a side of 0.125, peak opacity 0.2.
        assert!((sprite.half_size - 0.125).abs() < 1e-6);
        assert!((sprite.color[3] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn the_large_explosion_walks_its_sheet_and_ends_after_the_last_frame() {
        let mut draw = created(ParticleKind::LargeExplode);
        draw.particle.max_age = 8;
        let first = sprite(&draw, 0.0, DVec3::ZERO, &no_pictures()).expect("frame 0");
        assert_eq!(first.sheet, ParticleSheet::Explosion);
        assert_eq!(first.uv_min, [0.0, 0.0]);
        assert_eq!(first.light, [0.0, 15.0]);
        // Age 4 of 8: frame 7 is column 3 of row 1.
        draw.particle.age = 4;
        let middle = sprite(&draw, 0.0, DVec3::ZERO, &no_pictures()).expect("frame 7");
        assert_eq!(middle.uv_min, [0.75, 0.25]);
        assert!((middle.uv_max[0] - 0.99975).abs() < 1e-6);
        // Past the 16th frame nothing is drawn.
        draw.particle.age = 9;
        assert!(sprite(&draw, 0.0, DVec3::ZERO, &no_pictures()).is_none());
    }

    #[test]
    fn the_firework_flash_is_a_four_cell_region_that_swells_with_its_age() {
        let mut engine = ParticleEngine::new(1);
        engine.launch_firework(
            DVec3::new(0.5, 90.0, 0.5),
            DVec3::ZERO,
            vec![umicraft_game::particles::FireworkExplosion {
                shape: 4,
                trail: false,
                flicker: false,
                colors: vec![0xFF0000],
                fades: Vec::new(),
            }],
            DVec3::ZERO,
        );
        engine.tick(&umicraft_world::World::new(0));
        let draws = collect(&engine, None);
        let flash = draws
            .iter()
            .find(|draw| draw.particle.behaviour == Behaviour::FireworkFlash)
            .expect("the shell threw a flash");
        // Age 0 at partial 2: sin((0 + 2 - 1) * pi / 4) * 7.1 blocks.
        let quad = sprite(flash, 2.0, DVec3::ZERO, &no_pictures()).expect("a region");
        assert_eq!(quad.uv_min, [0.25, 0.125]);
        assert_eq!(quad.uv_max, [0.5, 0.375]);
        assert!((quad.half_size - 7.1 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert_eq!(quad.color[..3], [1.0, 0.0, 0.0]);
        assert!(quad.atlas_layer.is_none() && !quad.flat);
    }

    #[test]
    fn the_apparition_reports_how_far_through_its_thirty_ticks_it_is() {
        let draw = |behaviour| {
            let mut particle = flame();
            particle.behaviour = behaviour;
            particle.max_age = 30;
            particle.age = 12;
            ParticleDraw {
                particle,
                block_light: 0,
                sky_light: 15,
            }
        };
        assert_eq!(mob_appearance(&[draw(Behaviour::Plain)], 0.0), None);
        let haunting = [draw(Behaviour::Plain), draw(Behaviour::MobAppearance)];
        assert_eq!(mob_appearance(&haunting, 0.0), Some(0.4));
        assert_eq!(mob_appearance(&haunting, 3.0), Some(0.5));
        assert_eq!(mob_appearance(&haunting, 99.0), Some(1.0));
    }

    #[test]
    fn an_item_piece_samples_a_quarter_of_its_picture_untinted() {
        // A slime landing throws pieces of the slime ball.
        let mut particle = flame();
        particle.texture = Texture::Item { id: 341, damage: 0 };
        particle.jitter_u = 2.0;
        particle.jitter_v = 1.0;
        let draw = ParticleDraw {
            particle,
            block_light: 0,
            sky_light: 15,
        };
        let pictures = Pictures {
            blocks: no_block,
            items: |id, damage| (id == 341 && damage == 0).then_some(311),
        };
        let piece = sprite(&draw, 0.0, DVec3::ZERO, &pictures).expect("an item piece");
        assert_eq!(piece.atlas_layer, Some(311));
        assert_eq!(piece.uv_min, [0.5, 0.25]);
        assert_eq!(piece.uv_max, [0.75, 0.5]);
        assert_eq!(piece.color[..3], [1.0; 3]);
        // An atlas without the picture draws nothing.
        assert!(sprite(&draw, 0.0, DVec3::ZERO, &no_pictures()).is_none());
    }

    #[test]
    fn a_block_fragment_samples_a_quarter_of_its_layer_tinted() {
        let mut particle = flame();
        let state = BlockState::new(18, 0);
        particle.texture = Texture::Block { state };
        particle.jitter_u = 2.0;
        particle.jitter_v = 1.0;
        particle.color = [0.6; 3];
        let draw = ParticleDraw {
            particle,
            block_light: 0,
            sky_light: 15,
        };
        let pictures = Pictures {
            blocks: |_: BlockState| {
                Some(BlockSprite {
                    layer: 9,
                    tint: [0.5, 1.0, 0.0],
                })
            },
            items: no_item,
        };
        let sprite = sprite(&draw, 0.0, DVec3::ZERO, &pictures).expect("a block sprite");
        assert_eq!(sprite.atlas_layer, Some(9));
        assert_eq!(sprite.uv_min, [0.5, 0.25]);
        assert_eq!(sprite.uv_max, [0.75, 0.5]);
        assert_eq!(sprite.color[..3], [0.3, 0.6, 0.0]);
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
