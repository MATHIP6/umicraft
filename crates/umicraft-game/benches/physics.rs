// SPDX-License-Identifier: MIT OR Apache-2.0

//! The local player's tick (ADR-0076): `physics::step` is run every tick on
//! the game thread, and ADR-0076 moved it to `float` arithmetic and the
//! `MathHelper` table.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use glam::DVec3;
use umicraft_game::input::RawInput;
use umicraft_game::physics::{step, Player};
use umicraft_game::shapes::BlockSource;
use umicraft_world::BlockState;

/// A superflat world: solid up to y = 4.
struct Flat;

impl BlockSource for Flat {
    fn block(&self, _x: i32, y: i32, _z: i32) -> BlockState {
        if (0..4).contains(&y) {
            BlockState::new(1, 0)
        } else {
            BlockState::AIR
        }
    }
}

/// One hundred ticks of sprint-jumping while turning.
fn sprint_jump_turning(criterion: &mut Criterion) {
    criterion.bench_function("physics/step_100_sprint_jump_turning", |bench| {
        bench.iter(|| {
            let mut player = Player::new(DVec3::new(0.5, 4.0, 0.5));
            player.on_ground = true;
            for tick in 0..100_u16 {
                let intent = RawInput {
                    forward: 1.0,
                    strafe: if tick % 7 == 3 { 0.0 } else { 1.0 },
                    jump: true,
                    sprint: true,
                    yaw: f32::from(tick) * 1.35,
                    ..RawInput::default()
                }
                .to_intent();
                step(&mut player, &intent, &Flat);
            }
            black_box(player.position)
        });
    });
}

criterion_group!(benches, sprint_jump_turning);
criterion_main!(benches);
