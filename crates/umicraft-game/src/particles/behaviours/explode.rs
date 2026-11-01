// SPDX-License-Identifier: MIT OR Apache-2.0

//! The explosion smoke puff and the large explosion flash (MCP
//! `EntityExplodeFX`, `EntityLargeExplodeFX`). The emitter of the huge
//! explosion lives elsewhere.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::shared::random_offset;
use super::{Behaviour, TickContext};
use crate::particles::particle::{Layer, Lighting, Particle, Texture};
use crate::shapes::BlockSource;

/// The random motion added to the packet speed, MCP `EntityExplodeFX.java:15`
/// (`0.05F` widened).
const PUFF_SPREAD: f64 = 0.050_000_000_745_058_06;
/// The upward drift per tick, MCP `EntityExplodeFX.java:40`.
const PUFF_LIFT: f64 = 0.004;
/// The damping per tick, MCP `EntityExplodeFX.java:42` (`0.9F` widened).
const PUFF_DRAG: f64 = 0.899_999_976_158_142_1;
/// The ground friction per tick, MCP `EntityExplodeFX.java:47` (`0.7F`
/// widened).
const PUFF_GROUND_FRICTION: f64 = 0.699_999_988_079_071;
/// The picture of the large explosion in the user's resources, MCP
/// `EntityLargeExplodeFX.java:20`. The renderer binds it for layer
/// [`Layer::Lit`] and picks the frame with [`large_explode_quad`].
pub const LARGE_EXPLODE_TEXTURE: &str = "textures/entity/explosion";
/// The side of one frame in the 4 x 4 sheet, a hair under a quarter, MCP
/// `EntityLargeExplodeFX.java:44`.
pub const LARGE_EXPLODE_FRAME_SPAN: f32 = 0.24975;
/// The frames of the sheet minus one, MCP `EntityLargeExplodeFX.java:38`.
const LARGE_EXPLODE_LAST_FRAME: f32 = 15.0;

/// `EntityExplodeFX.Factory`: a grey puff that grows and thins out (MCP
/// `EntityExplodeFX.java:11-22`).
///
/// It is born with the packet speed plus a small scatter, a random grey from
/// 0.7 to 1, a scale from 1 to 7 (biased to small) and a life of 18 to 82
/// ticks.
#[must_use]
pub(super) fn puff(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    particle.motion = DVec3::new(
        speed.x + random_offset(rng, PUFF_SPREAD),
        speed.y + random_offset(rng, PUFF_SPREAD),
        speed.z + random_offset(rng, PUFF_SPREAD),
    );
    let grey = rng.next_float() * 0.3 + 0.7;
    particle.color = [grey; 3];
    particle.scale = rng.next_float() * rng.next_float() * 6.0 + 1.0;
    particle.max_age = (16.0 / (f64::from(rng.next_float()) * 0.8 + 0.2)) as u32 + 2;
    particle.behaviour = Behaviour::Explode;
    particle
}

/// `EntityExplodeFX.onUpdate`: ages, walks the eight smoke pictures from
/// dense to faint, drifts up and slows down.
///
/// It dies when the age *before* the increment reached the life, and keeps
/// moving that last tick like the reference.
pub(super) fn update_puff<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    if particle.age >= particle.max_age {
        particle.dead = true;
    }
    particle.age += 1;
    let frame = particle.age * 8 / particle.max_age;
    particle.texture = Texture::Cell {
        index: 7u32.saturating_sub(frame) as u8,
    };
    particle.motion.y += PUFF_LIFT;
    particle.move_with_collision(context.world);
    particle.motion *= PUFF_DRAG;
    if particle.on_ground {
        particle.motion.x *= PUFF_GROUND_FRICTION;
        particle.motion.z *= PUFF_GROUND_FRICTION;
    }
}

/// `EntityLargeExplodeFX.Factory`: a still, fully lit flash (MCP
/// `EntityLargeExplodeFX.java:25-33`).
///
/// `speed.x` carries the size: the quad is `2 * (1 - 0.5 * speed.x)` blocks
/// from its centre to an edge. The life (6 to 9 ticks) is stored in
/// `max_age`, which this particle never uses otherwise.
#[must_use]
pub(super) fn large_flash(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.max_age = 6 + rng.next_int(4);
    let grey = rng.next_float() * 0.6 + 0.4;
    particle.color = [grey; 3];
    particle.layer = Layer::Lit;
    particle.lighting = Lighting::FullBright;
    particle.texture = Texture::Sprite {
        name: LARGE_EXPLODE_TEXTURE,
    };
    particle.behaviour = Behaviour::LargeExplode {
        size: 1.0 - speed.x as f32 * 0.5,
    };
    particle
}

/// `EntityLargeExplodeFX.onUpdate`: only ages, and dies when the age reaches
/// the life. It never moves.
pub(super) fn update_flash(particle: &mut Particle) {
    particle.previous_position = particle.position;
    particle.age += 1;
    if particle.age == particle.max_age {
        particle.dead = true;
    }
}

/// Where and how big the large explosion is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LargeExplodeQuad {
    /// The left edge of the frame in the 4 x 4 sheet, 0 to 0.75.
    pub u: f32,
    /// The top edge of the frame in the sheet, 0 to 0.75.
    pub v: f32,
    /// The distance from the centre to an edge of the quad, in blocks.
    pub half_extent: f32,
}

/// The frame and size `EntityLargeExplodeFX.renderParticle` draws at
/// `partial_ticks`, or `None` once the animation passed its 16th frame (or
/// for another particle).
///
/// The frame advances `15 / life` per tick, so the whole sheet plays over the
/// life. The reference draws it white-tinted by the particle colour, unlit,
/// with the lightmap forced to full sky and block light.
#[must_use]
pub fn large_explode_quad(particle: &Particle, partial_ticks: f32) -> Option<LargeExplodeQuad> {
    let Behaviour::LargeExplode { size } = particle.behaviour else {
        return None;
    };
    let progress =
        (particle.age as f32 + partial_ticks) * LARGE_EXPLODE_LAST_FRAME / particle.max_age as f32;
    let frame = progress as u32;
    (frame <= 15).then(|| LargeExplodeQuad {
        u: (frame % 4) as f32 / 4.0,
        v: (frame / 4) as f32 / 4.0,
        half_extent: 2.0 * size,
    })
}

#[cfg(test)]
mod tests {
    use super::super::shared::Layered;
    use super::*;
    use umicraft_world::BlockState;

    fn air() -> Layered {
        Layered {
            top: 0,
            state: BlockState::AIR,
        }
    }

    fn tick<S: BlockSource>(particle: &mut Particle, world: &S, rng: &mut JavaRandom) {
        let mut spawned = Vec::new();
        let mut context = TickContext {
            world,
            rng,
            spawned: &mut spawned,
        };
        let behaviour = particle.behaviour;
        behaviour.update(particle, &mut context);
    }

    #[test]
    fn a_puff_is_born_in_the_reference_ranges() {
        let mut rng = JavaRandom::new(8);
        for _ in 0..300 {
            let puff = puff(DVec3::ZERO, DVec3::ZERO, &mut rng);
            assert!((0.7..=1.0).contains(&puff.color[0]));
            assert_eq!(puff.color[0], puff.color[2]);
            assert!((1.0..=7.0).contains(&puff.scale));
            assert!((18..=82).contains(&puff.max_age));
            assert!(puff.motion.abs().max_element() <= PUFF_SPREAD);
        }
    }

    #[test]
    fn a_puff_follows_the_packet_speed() {
        let mut rng = JavaRandom::new(8);
        let puff = puff(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0), &mut rng);
        assert!((puff.motion.x - 1.0).abs() <= PUFF_SPREAD);
    }

    #[test]
    fn a_puff_fades_through_the_eight_pictures() {
        let mut rng = JavaRandom::new(8);
        let mut puff = puff(DVec3::new(0.5, 100.0, 0.5), DVec3::ZERO, &mut rng);
        puff.max_age = 16;
        puff.motion = DVec3::ZERO;
        tick(&mut puff, &air(), &mut rng);
        // age 1: 7 - 1 * 8 / 16 = 7.
        assert_eq!(puff.texture, Texture::Cell { index: 7 });
        for _ in 0..7 {
            tick(&mut puff, &air(), &mut rng);
        }
        // age 8: 7 - 8 * 8 / 16 = 3.
        assert_eq!(puff.texture, Texture::Cell { index: 3 });
        assert!(puff.motion.y > 0.0);
    }

    #[test]
    fn a_puff_dies_one_tick_after_reaching_its_life() {
        let mut rng = JavaRandom::new(8);
        let mut puff = puff(DVec3::new(0.5, 100.0, 0.5), DVec3::ZERO, &mut rng);
        puff.max_age = 3;
        for _ in 0..3 {
            tick(&mut puff, &air(), &mut rng);
            assert!(!puff.dead);
        }
        tick(&mut puff, &air(), &mut rng);
        assert!(puff.dead);
        assert_eq!(puff.texture, Texture::Cell { index: 0 });
    }

    #[test]
    fn a_flash_is_lit_and_sized_by_the_packet() {
        let mut rng = JavaRandom::new(8);
        for _ in 0..100 {
            let flash = large_flash(DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), &mut rng);
            assert!((6..=9).contains(&flash.max_age));
            assert!((0.4..=1.0).contains(&flash.color[1]));
            assert_eq!(flash.layer, Layer::Lit);
            assert_eq!(flash.lighting, Lighting::FullBright);
            assert_eq!(flash.behaviour, Behaviour::LargeExplode { size: 0.0 });
        }
    }

    #[test]
    fn a_flash_stays_put_and_dies_at_its_life() {
        let mut rng = JavaRandom::new(8);
        let mut flash = large_flash(DVec3::new(0.5, 100.0, 0.5), DVec3::ZERO, &mut rng);
        flash.max_age = 6;
        for _ in 0..5 {
            tick(&mut flash, &air(), &mut rng);
            assert!(!flash.dead);
        }
        tick(&mut flash, &air(), &mut rng);
        assert!(flash.dead);
        assert_eq!(flash.position, DVec3::new(0.5, 100.0, 0.5));
    }

    #[test]
    fn the_flash_plays_the_whole_sheet_over_its_life() {
        let mut rng = JavaRandom::new(8);
        let mut flash = large_flash(DVec3::ZERO, DVec3::ZERO, &mut rng);
        flash.max_age = 6;
        let first = large_explode_quad(&flash, 0.0).expect("frame 0");
        assert_eq!((first.u, first.v, first.half_extent), (0.0, 0.0, 2.0));
        flash.age = 3;
        // 3 * 15 / 6 = 7.5 -> frame 7: column 3, row 1.
        let middle = large_explode_quad(&flash, 0.0).expect("frame 7");
        assert_eq!((middle.u, middle.v), (0.75, 0.25));
        flash.age = 6;
        // 6 * 15 / 6 = 15 is still drawn; one tick more is not.
        assert!(large_explode_quad(&flash, 0.0).is_some());
        assert!(large_explode_quad(&flash, 0.5).is_none());
    }

    #[test]
    fn other_particles_have_no_flash_quad() {
        let particle = Particle::new(DVec3::ZERO, &mut JavaRandom::new(1));
        assert_eq!(large_explode_quad(&particle, 0.0), None);
    }
}
