// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockState;

use super::*;
use crate::input::InputIntent;
use crate::physics;

#[derive(Default)]
struct World(HashMap<(i32, i32, i32), u16>);

impl World {
    /// A floor of `id` at row 63, so a player at y = 64 stands on it.
    fn floor(id: u16) -> Self {
        let mut world = Self::default();
        for x in -20..20 {
            for z in -20..30 {
                world.0.insert((x, 63, z), id);
            }
        }
        world
    }

    fn with(mut self, x: i32, y: i32, z: i32, id: u16) -> Self {
        self.0.insert((x, y, z), id);
        self
    }
}

impl BlockSource for World {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        BlockState::new(self.0.get(&(x, y, z)).copied().unwrap_or(0), 0)
    }
}

fn rng() -> JavaRandom {
    JavaRandom::new(7)
}

/// A player that moved by `delta` and stands at `(0.5, 64, 0.5)`.
fn mover(delta: DVec3) -> Player {
    let mut player = Player::new(DVec3::new(0.5, 64.0, 0.5));
    player.on_ground = true;
    player.step_facts = StepFacts {
        moved: true,
        walks: true,
        delta,
        ..StepFacts::default()
    };
    player
}

fn run(stepper: &mut Stepper, player: &Player, world: &World) -> Vec<SoundCue> {
    let mut out = Vec::new();
    stepper.tick(player, player.position, world, &mut rng(), &mut out);
    out
}

/// Runs `intent` on `world` through the real physics for `ticks` ticks.
fn simulate(world: &World, intent: InputIntent, ticks: usize) -> (Player, Vec<SoundCue>) {
    let mut player = Player::new(DVec3::new(0.5, 64.0, 0.5));
    let mut stepper = Stepper::new();
    let mut rng = rng();
    let mut cues = Vec::new();
    for _ in 0..ticks {
        let before = player.position;
        physics::step(&mut player, &intent, world);
        stepper.tick(&player, before, world, &mut rng, &mut cues);
    }
    (player, cues)
}

#[test]
fn walking_ten_blocks_on_grass_gives_six_quiet_steps() {
    let world = World::floor(2);
    let mut player = Player::new(DVec3::new(0.5, 64.0, 0.5));
    let mut stepper = Stepper::new();
    let mut cues = Vec::new();
    while player.position.z < 10.5 {
        let before = player.position;
        physics::step(&mut player, &InputIntent::walking(0.0), &world);
        stepper.tick(&player, before, &world, &mut rng(), &mut cues);
    }
    // 10.5 blocks are 6.3 step units; the counter passes 1, 2, ... 6.
    assert_eq!(cues.len(), 6);
    for cue in &cues {
        assert_eq!(cue.name, "step.grass");
        assert!((cue.volume - 0.15).abs() < 1e-6);
        assert!((cue.pitch - 1.0).abs() < 1e-6);
    }
}

#[test]
fn standing_still_is_silent() {
    let (_, cues) = simulate(&World::floor(2), InputIntent::still(0.0), 100);
    assert!(cues.is_empty());
}

#[test]
fn sneaking_on_the_ground_is_silent() {
    let intent = InputIntent {
        sneak: true,
        ..InputIntent::walking(0.0)
    };
    let (player, cues) = simulate(&World::floor(2), intent, 100);
    assert!(player.position.z > 5.0);
    assert!(cues.is_empty());
}

#[test]
fn a_flying_player_makes_no_steps() {
    let mut player = mover(DVec3::new(0.0, 0.0, 3.0));
    player.step_facts.walks = false;
    assert!(run(&mut Stepper::new(), &player, &World::floor(2)).is_empty());
}

#[test]
fn a_spectator_makes_no_sound() {
    let mut player = mover(DVec3::new(0.0, 0.0, 3.0));
    player.step_facts.moved = false;
    assert!(run(&mut Stepper::new(), &player, &World::floor(2)).is_empty());
}

#[test]
fn the_step_takes_the_pitch_of_the_block_and_snow_wins() {
    let player = mover(DVec3::new(0.0, 0.0, 3.0));
    let metal = run(&mut Stepper::new(), &player, &World::floor(42));
    assert_eq!(metal[0].name, "step.stone");
    assert!((metal[0].pitch - 1.5).abs() < 1e-6);
    let snowy = World::floor(2).with(0, 64, 0, 78);
    let snow = run(&mut Stepper::new(), &player, &snowy);
    assert_eq!(snow[0].name, "step.snow");
}

#[test]
fn a_liquid_floor_is_silent_but_the_air_never_steps() {
    let player = mover(DVec3::new(0.0, 0.0, 3.0));
    assert!(run(&mut Stepper::new(), &player, &World::floor(9)).is_empty());
    assert!(run(&mut Stepper::new(), &player, &World::default()).is_empty());
}

#[test]
fn a_fence_under_the_feet_counts_as_the_floor() {
    let mut player = mover(DVec3::new(0.0, 0.0, 3.0));
    player.position.y = 64.5;
    let world = World::default().with(0, 63, 0, 85);
    assert_eq!(
        run(&mut Stepper::new(), &player, &world)[0].name,
        "step.wood"
    );
}

#[test]
fn only_a_ladder_counts_vertical_travel() {
    let player = mover(DVec3::new(0.0, 2.0, 0.0));
    let ladder = World::floor(2).with(0, 63, 0, 65);
    assert_eq!(run(&mut Stepper::new(), &player, &ladder).len(), 1);
    assert!(run(&mut Stepper::new(), &player, &World::floor(2)).is_empty());
}

#[test]
fn swimming_adds_the_swim_sound_with_a_volume_from_the_motion() {
    let mut player = mover(DVec3::new(0.0, 0.0, 3.0));
    player.in_water = true;
    player.step_facts.velocity = DVec3::new(0.0, 1.0, 0.0);
    let cues = run(&mut Stepper::new(), &player, &World::floor(2));
    assert_eq!(cues[0].name, "game.player.swim");
    assert!((cues[0].volume - 0.35).abs() < 1e-6);
    assert!((0.6..=1.4).contains(&cues[0].pitch));
    assert_eq!(cues[1].name, "step.grass");
}

#[test]
fn entering_water_splashes_except_on_the_first_tick() {
    let mut player = mover(DVec3::ZERO);
    player.step_facts.splash_velocity = Some(DVec3::new(0.0, -5.0, 0.0));
    let mut stepper = Stepper::new();
    assert!(run(&mut stepper, &player, &World::default()).is_empty());
    let cues = run(&mut stepper, &player, &World::default());
    assert_eq!(cues[0].name, "game.player.swim.splash");
    assert!((cues[0].volume - 1.0).abs() < 1e-6);
}

#[test]
fn a_long_fall_hurts_loudly_and_thuds_on_the_block() {
    let mut player = mover(DVec3::ZERO);
    player.step_facts.landed_fall = 10.0;
    let cues = run(&mut Stepper::new(), &player, &World::floor(2));
    assert_eq!(cues[0].name, "game.player.hurt.fall.big");
    assert_eq!(cues[1].name, "step.grass");
    assert!((cues[1].volume - 0.5).abs() < 1e-6);
    assert!((cues[1].pitch - 0.75).abs() < 1e-6);
}

#[test]
fn the_fall_sound_follows_the_damage_threshold() {
    let world = World::floor(2);
    let mut player = mover(DVec3::ZERO);
    player.step_facts.landed_fall = 3.0;
    assert!(run(&mut Stepper::new(), &player, &world).is_empty());
    player.step_facts.landed_fall = 4.0;
    assert_eq!(
        run(&mut Stepper::new(), &player, &world)[0].name,
        "game.player.hurt.fall.small"
    );
    player.step_facts.landed_fall = 8.5;
    assert_eq!(
        run(&mut Stepper::new(), &player, &world)[0].name,
        "game.player.hurt.fall.big"
    );
    player.jump_boost = 2;
    player.step_facts.landed_fall = 5.0;
    assert!(run(&mut Stepper::new(), &player, &world).is_empty());
}

#[test]
fn a_flyer_and_a_slime_landing_are_silent_unless_sneaking() {
    let mut player = mover(DVec3::ZERO);
    player.step_facts.landed_fall = 10.0;
    player.allow_flying = true;
    assert!(run(&mut Stepper::new(), &player, &World::floor(2)).is_empty());
    player.allow_flying = false;
    let slime = World::floor(165);
    assert!(run(&mut Stepper::new(), &player, &slime).is_empty());
    player.sneaking = true;
    assert!(!run(&mut Stepper::new(), &player, &slime).is_empty());
}

#[test]
fn falling_from_height_in_the_real_physics_lands_with_a_hurt_sound() {
    let world = World::floor(1);
    let mut player = Player::new(DVec3::new(0.5, 74.0, 0.5));
    let mut stepper = Stepper::new();
    let mut cues = Vec::new();
    for _ in 0..40 {
        let before = player.position;
        physics::step(&mut player, &InputIntent::still(0.0), &world);
        stepper.tick(&player, before, &world, &mut rng(), &mut cues);
    }
    let names: Vec<_> = cues.iter().map(|cue| cue.name.as_ref()).collect();
    assert_eq!(names, ["game.player.hurt.fall.big", "step.stone"]);
}
