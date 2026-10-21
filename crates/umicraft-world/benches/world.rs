// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baseline benchmarks for chunk decode and block access (spec §7).

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_protocol::packets::play::clientbound::{BulkChunk, MapChunkBulk};
use umicraft_world::{BlockPos, ChunkColumn, ChunkPos, World};

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

fn flat_world(chunk_count: i32) -> World {
    let mut world = World::new(0);
    let chunks = (0..chunk_count)
        .map(|index| BulkChunk {
            chunk_x: index,
            chunk_z: 0,
            primary_bit_mask: 1,
            data: FLAT_SECTION.to_vec(),
        })
        .collect();
    world
        .apply_map_chunk_bulk(&MapChunkBulk {
            sky_light: true,
            chunks,
        })
        .unwrap();
    world
}

fn apply_bulk_10(criterion: &mut Criterion) {
    criterion.bench_function("world/apply_bulk_10_chunks", |bencher| {
        let chunks = (0..10)
            .map(|index| BulkChunk {
                chunk_x: index,
                chunk_z: 0,
                primary_bit_mask: 1,
                data: FLAT_SECTION.to_vec(),
            })
            .collect::<Vec<_>>();
        bencher.iter(|| {
            let mut world = World::new(0);
            world
                .apply_map_chunk_bulk(&MapChunkBulk {
                    sky_light: true,
                    chunks: chunks.clone(),
                })
                .unwrap();
            black_box(world)
        });
    });
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
    block_access_100k
);
criterion_main!(benches);
