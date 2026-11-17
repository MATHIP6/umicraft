// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles the mobs give off.

use super::*;

fn game_with_mob(mob_type: u8) -> Game {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entities.spawn_mob(
        EntityId(7),
        mob_type,
        DVec3::new(2.5, 64.0, 2.5),
        0.0,
        0.0,
        0.0,
        glam::Vec3::ZERO,
        EntityFlags::default(),
    );
    game
}

#[test]
fn a_blaze_near_the_player_smokes_every_tick() {
    // Regression: no mob gave off its own particles.
    let mut game = game_with_mob(61);
    spawn_mob_particles(&mut game);
    assert_eq!(game.particles.len(), 2);
}

#[test]
fn an_animal_in_love_shows_hearts() {
    let mut game = game_with_mob(92);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, hurt_status(7, 18), &mut queue, &frames);
    assert_eq!(game.particles.len(), 7);
}
