// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity.createRunningParticles`: the dust a sprinting entity kicks up.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use crate::particles::{ParticleKind, SpawnRequest};
use crate::shapes::BlockSource;

/// How far under the feet the block is read (`posY - 0.20000000298023224D`).
const PROBE_DEPTH: f64 = 0.200_000_002_980_232_24;
/// How high above the feet the dust starts (`minY + 0.1D`).
const DUST_HEIGHT: f64 = 0.1;
/// The block id of the moving piston, the only block besides air that has no
/// model (`getRenderType() == -1`).
const MOVING_PISTON: u16 = 36;

/// The part of an entity the running dust reads.
#[derive(Debug, Clone, Copy)]
pub struct RunningBody {
    /// The feet (`posX`, `posY`, `posZ`).
    pub position: DVec3,
    /// The width of its bounding box.
    pub width: f64,
    /// Its motion per tick.
    pub motion: DVec3,
}

/// The `blockcrack` particle under a sprinting entity's feet, flung backwards
/// (MCP `Entity.createRunningParticles`, line 1163). It is `BLOCK_CRACK`, not
/// `BLOCK_DUST`: 1.8 uses the fragment that keeps the texture jitter.
///
/// The caller checks `isSprinting() && !isInWater()` (`spawnRunningParticles`).
/// The block state id goes through `argument`, which the request borrows;
/// there is nothing to spawn over air or a moving piston.
pub fn running_dust<'a, S: BlockSource + ?Sized>(
    world: &S,
    body: &RunningBody,
    argument: &'a mut [i32; 1],
    rng: &mut JavaRandom,
) -> Option<SpawnRequest<'a>> {
    let ground = BlockPos::new(
        body.position.x.floor() as i32,
        (body.position.y - PROBE_DEPTH).floor() as i32,
        body.position.z.floor() as i32,
    );
    let state = world.block(ground.x, ground.y, ground.z);
    if state.is_air() || state.id() == MOVING_PISTON {
        return None;
    }
    // `Block.getStateId`: the id in the low 12 bits, the metadata above.
    argument[0] = i32::from(state.id()) | (i32::from(state.metadata()) << 12);
    let scatter = |rng: &mut JavaRandom| (f64::from(rng.next_float()) - 0.5) * body.width;
    let (across_x, across_z) = (scatter(rng), scatter(rng));
    Some(SpawnRequest {
        kind: ParticleKind::BlockCrack,
        arguments: argument,
        ignore_range: false,
        position: DVec3::new(
            body.position.x + across_x,
            body.position.y + DUST_HEIGHT,
            body.position.z + across_z,
        ),
        velocity: DVec3::new(-body.motion.x * 4.0, 1.5, -body.motion.z * 4.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::World;
    use umicraft_world::BlockState;

    fn body() -> RunningBody {
        RunningBody {
            position: DVec3::new(10.5, 64.0, -3.5),
            width: 0.6,
            motion: DVec3::new(0.2, -0.0784, -0.1),
        }
    }

    #[test]
    fn dust_comes_off_the_block_under_the_feet_and_flies_backwards() {
        let mut world = World::default();
        world.set(10, 63, -4, BlockState::new(2, 1));
        let mut argument = [0];
        let request = running_dust(&world, &body(), &mut argument, &mut JavaRandom::new(1))
            .expect("grass under the feet");
        assert_eq!(request.kind, ParticleKind::BlockCrack);
        assert_eq!(request.arguments, [2 | (1 << 12)]);
        assert!((request.position.x - 10.5).abs() <= 0.3);
        assert!((request.position.z + 3.5).abs() <= 0.3);
        assert!((request.position.y - 64.1).abs() < 1e-12);
        assert_eq!(request.velocity, DVec3::new(-0.8, 1.5, 0.4));
    }

    #[test]
    fn a_step_just_under_the_feet_still_counts() {
        let mut world = World::default();
        // Feet at 63.9 read row 63 (63.9 - 0.2 floors to 63).
        world.set(10, 63, -4, BlockState::new(1, 0));
        let mut argument = [0];
        let mut standing = body();
        standing.position.y = 63.9;
        assert!(running_dust(&world, &standing, &mut argument, &mut JavaRandom::new(1)).is_some());
    }

    #[test]
    fn nothing_is_kicked_up_over_air() {
        let mut argument = [0];
        assert!(running_dust(
            &World::default(),
            &body(),
            &mut argument,
            &mut JavaRandom::new(1)
        )
        .is_none());
    }
}
