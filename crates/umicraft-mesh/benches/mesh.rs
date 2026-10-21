// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baseline meshing benchmarks (spec §7).

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_mesh::{mesh_chunk, ChunkSnapshot};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

const FLAT_SECTION: &[u8] =
    include_bytes!("../../umicraft-world/tests/fixtures/flat-chunk-section.bin");

fn flat_column(pos: ChunkPos) -> ChunkColumn {
    let mut column = ChunkColumn::new(pos);
    column
        .apply_payload(0x0001, true, true, FLAT_SECTION)
        .unwrap();
    column
}

fn mesh_flat_chunk(criterion: &mut Criterion) {
    let centre = ChunkPos::new(0, 0);
    let columns = (-1..=1)
        .flat_map(|dz| (-1..=1).map(move |dx| ChunkPos::new(dx, dz)))
        .map(|pos| Arc::new(flat_column(pos)))
        .collect::<Vec<_>>();
    let snapshot = ChunkSnapshot::from_columns(centre, columns);
    criterion.bench_function("mesh/flat_chunk_with_neighbours", |bencher| {
        bencher.iter(|| black_box(mesh_chunk(centre, black_box(&snapshot))));
    });
}

fn mesh_dense_section(criterion: &mut Criterion) {
    let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                column.set_block(x, y, z, BlockState::new(1, 0));
            }
        }
    }
    let snapshot = ChunkSnapshot::from_columns(ChunkPos::new(0, 0), [Arc::new(column)]);
    criterion.bench_function("mesh/dense_16cube", |bencher| {
        bencher.iter(|| black_box(mesh_chunk(ChunkPos::new(0, 0), black_box(&snapshot))));
    });
}

criterion_group!(benches, mesh_flat_chunk, mesh_dense_section);
criterion_main!(benches);
