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

#[test]
fn a_spawner_near_the_player_puffs_smoke_and_a_flame_every_tick() {
    // Regression: spawners gave off no particles.
    let mut game = game_on_floor_with(BlockState::new(52, 0));
    spawn_spawner_particles(&mut game);
    assert_eq!(game.particles.len(), 2);
    // A spawner cart puffs too.
    game.entities.spawn_object(
        EntityId(8),
        10,
        DVec3::new(4.5, 64.0, 4.5),
        0.0,
        0.0,
        glam::Vec3::ZERO,
        4,
    );
    spawn_spawner_particles(&mut game);
    assert_eq!(game.particles.len(), 6);
}

#[test]
fn a_spectator_wakes_no_spawner() {
    let mut game = game_on_floor_with(BlockState::new(52, 0));
    game.gamemode = GameMode::Spectator;
    spawn_spawner_particles(&mut game);
    assert_eq!(game.particles.len(), 0);
}
