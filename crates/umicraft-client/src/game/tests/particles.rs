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

/// A rocket entity (object type 76) carrying one large-ball explosion.
fn game_with_rocket(explosions: Vec<umicraft_game::nbt::Nbt>) -> Game {
    use umicraft_game::entity::EntityItem;
    use umicraft_game::nbt::Nbt;
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entities.spawn_object(
        EntityId(9),
        76,
        DVec3::new(2.5, 66.0, 2.5),
        0.0,
        0.0,
        glam::Vec3::ZERO,
        0,
    );
    let tag = Nbt::Compound(vec![(
        "Fireworks".into(),
        Nbt::Compound(vec![("Explosions".into(), Nbt::List(explosions))]),
    )]);
    let item = EntityItem {
        item_id: 401,
        damage: 0,
        count: 1,
        nbt: tag.to_root_bytes().map(Into::into),
    };
    game.entities.set_item(EntityId(9), Some(item));
    game
}

#[test]
fn an_exploding_rocket_throws_the_sparks_of_its_tag_and_blasts() {
    use umicraft_game::nbt::Nbt;
    let burst = Nbt::Compound(vec![
        ("Type".into(), Nbt::Byte(4)),
        ("Colors".into(), Nbt::IntArray(vec![0xFF0000])),
    ]);
    let mut game = game_with_rocket(vec![burst]);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, hurt_status(9, 17), &mut queue, &frames);
    // The shell is not a particle: nothing is drawn until it ticks.
    assert_eq!(game.particles.len(), 0);
    let world = game.world.as_ref().expect("a world");
    game.particles.tick(world);
    // 70 sparks of a burst and the flash.
    assert_eq!(game.particles.len(), 71);
    let sounds = game.particles.drain_sounds();
    assert_eq!(sounds.len(), 1);
    assert!(sounds[0].name.starts_with("fireworks.blast"));
}

#[test]
fn another_status_of_a_rocket_launches_nothing() {
    let mut game = game_with_rocket(Vec::new());
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, hurt_status(9, 2), &mut queue, &frames);
    let world = game.world.as_ref().expect("a world");
    game.particles.tick(world);
    assert_eq!(game.particles.len(), 0);
}

#[test]
fn a_snowball_landing_on_the_floor_puffs() {
    // Regression: the impact particles of thrown objects were never made.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entities.spawn_object(
        EntityId(11),
        61,
        DVec3::new(2.5, 64.6, 2.5),
        0.0,
        0.0,
        glam::Vec3::new(0.0, -1.0, 0.0),
        0,
    );
    let world = game.world.as_ref().expect("a world");
    game.entities.tick_objects(world);
    spawn_impact_particles(&mut game);
    assert_eq!(game.particles.len(), 8);
    // The impact is reported once.
    spawn_impact_particles(&mut game);
    assert_eq!(game.particles.len(), 8);
}
