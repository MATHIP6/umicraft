// SPDX-License-Identifier: MIT OR Apache-2.0

//! Particle engine benchmarks (P8-g): the tick of a full bucket and the burst
//! of a broken block.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use glam::DVec3;
use umicraft_game::particles::{create, ParticleEngine, ParticleKind, MAX_PER_BUCKET};
use umicraft_world::{BlockPos, BlockState, World};

/// An engine holding `count` smoke puffs high above an empty world.
fn crowded_engine(count: usize) -> ParticleEngine {
    let mut engine = ParticleEngine::new(1);
    for index in 0..count {
        let position = DVec3::new((index % 64) as f64, 200.0, (index / 64) as f64);
        let mut puff = create(ParticleKind::Smoke, position, DVec3::ZERO, engine.rng())
            .expect("smoke is ported");
        // Long-lived so the population stays full across iterations.
        puff.max_age = u32::MAX;
        engine.add(puff);
    }
    engine
}

fn tick_full_bucket(criterion: &mut Criterion) {
    let world = World::new(0);
    let mut engine = crowded_engine(MAX_PER_BUCKET);
    criterion.bench_function("particles/tick_4000_smoke", |bench| {
        bench.iter(|| engine.tick(black_box(&world)));
    });
}

fn break_block(criterion: &mut Criterion) {
    criterion.bench_function("particles/break_block_64_fragments", |bench| {
        bench.iter(|| {
            let mut engine = ParticleEngine::new(1);
            engine.add_block_destroy_effects(
                BlockPos::new(0, 64, 0),
                black_box(BlockState::new(1, 0)),
                [1.0; 3],
            );
            engine.len()
        });
    });
}

criterion_group!(benches, tick_full_bucket, break_block);
criterion_main!(benches);
