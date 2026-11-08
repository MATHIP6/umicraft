// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baseline benchmarks for chunk decode and block access (spec §7).

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_world::{BlockPos, BlockState, ChunkColumn, ChunkPayload, ChunkPos, World};

const FLAT_SECTION: &[u8] = include_bytes!("../tests/fixtures/flat-chunk-section.bin");

fn decode_flat_section(criterion: &mut Criterion) {
    criterion.bench_function("world/decode_flat_section_12544B", |bencher| {
        bencher.iter(|| {
            let mut column = ChunkColumn::new(ChunkPos::new(-107, 6));
            column
                .apply_payload(0x0001, true, true, black_box(FLAT_SECTION))
                .unwrap();
            black_box(column)
        });
    });
}

/// `count` captured flat columns in a row, as a bulk batch borrows them.
fn flat_columns(count: i32) -> Vec<(ChunkPos, ChunkPayload<'static>)> {
    (0..count)
        .map(|index| {
            let payload = ChunkPayload {
                primary_bit_mask: 1,
                ground_up: true,
                data: FLAT_SECTION,
            };
            (ChunkPos::new(index, 0), payload)
        })
        .collect()
}

fn flat_world(chunk_count: i32) -> World {
    let mut world = World::new(0);
    world
        .apply_chunk_bulk(true, &flat_columns(chunk_count))
        .unwrap();
    world
}

fn apply_bulk_10(criterion: &mut Criterion) {
    criterion.bench_function("world/apply_bulk_10_chunks", |bencher| {
        let chunks = flat_columns(10);
        bencher.iter(|| {
            let mut world = World::new(0);
            world.apply_chunk_bulk(true, black_box(&chunks)).unwrap();
            black_box(world)
        });
    });
}

/// A world with one stone column eight sections tall, as a mesh job would hold.
fn stone_world() -> World {
    let mut world = flat_world(1);
    for y in 0..128 {
        for z in 0..16 {
            for x in 0..16 {
                world.set_block(BlockPos::new(x, y, z), BlockState::new(1, 0));
            }
        }
    }
    world
}

/// F2-4f: an edit while a mesh job still holds the column copies it
/// (`Arc::make_mut`); the same edit without a holder does not.
fn edit_with_a_job_in_flight(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("world/set_block");
    let pos = ChunkPos::new(0, 0);
    let mut world = stone_world();
    let mut toggle = false;
    group.bench_function("unshared_column", |bencher| {
        bencher.iter(|| {
            toggle = !toggle;
            let state = BlockState::new(if toggle { 4 } else { 1 }, 0);
            black_box(world.set_block(BlockPos::new(3, 70, 3), state))
        });
    });
    group.bench_function("column_held_by_a_snapshot", |bencher| {
        bencher.iter(|| {
            toggle = !toggle;
            let state = BlockState::new(if toggle { 4 } else { 1 }, 0);
            let snapshot = world.chunk_shared(pos);
            let changed = world.set_block(BlockPos::new(3, 70, 3), state);
            black_box((changed, snapshot))
        });
    });
    group.finish();
}

fn block_access_100k(criterion: &mut Criterion) {
    let world = flat_world(10);
    criterion.bench_function("world/block_get_100k", |bencher| {
        bencher.iter(|| {
            let mut sum = 0u32;
            for index in 0..100_000i32 {
                let x = (index * 31) & 255;
                let z = (index * 17) & 255;
                let y = index & 255;
                sum += u32::from(world.block(BlockPos::new(x, y, z)).wire());
            }
            black_box(sum)
        });
    });
}

criterion_group!(
    benches,
    decode_flat_section,
    apply_bulk_10,
    block_access_100k,
    edit_with_a_job_in_flight
);
criterion_main!(benches);
