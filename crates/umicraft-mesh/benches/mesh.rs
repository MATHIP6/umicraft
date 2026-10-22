// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baseline meshing benchmarks (spec §7).

use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_mesh::{mesh_chunk, mesh_chunk_with_models, ChunkSnapshot, ModelMeshContext};
use umicraft_model::{
    AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource, StateDef,
};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

const FLAT_SECTION: &[u8] =
    include_bytes!("../../umicraft-world/tests/fixtures/flat-chunk-section.bin");

/// A full cube with `cullface` on every face, used by the model benchmark.
const CUBE_MODEL: &str = r##"{
  "textures": { "particle": "#all", "all": "#all" },
  "elements": [{
    "from": [0, 0, 0],
    "to": [16, 16, 16],
    "faces": {
      "down":  { "texture": "#all", "cullface": "down" },
      "up":    { "texture": "#all", "cullface": "up" },
      "north": { "texture": "#all", "cullface": "north" },
      "south": { "texture": "#all", "cullface": "south" },
      "west":  { "texture": "#all", "cullface": "west" },
      "east":  { "texture": "#all", "cullface": "east" }
    }
  }]
}"##;

const CUBE_BLOCKSTATE: &str = r#"{ "variants": { "normal": { "model": "cube" } } }"#;

const CUBE_STATES: &[StateDef] = &[StateDef {
    mask: 0,
    value: 0,
    blockstate: "cube",
    variant: "normal",
}];

/// The captured flat chunk uses ids 2 (grass), 3 (dirt) and 7 (bedrock); all
/// three share one synthetic cube so the model path emits the same 512 faces
/// as [`mesh_flat_chunk`].
const CUBE_DEFS: &[BlockDefinition] = &[
    BlockDefinition {
        id: 2,
        name: "grass",
        states: CUBE_STATES,
    },
    BlockDefinition {
        id: 3,
        name: "dirt",
        states: CUBE_STATES,
    },
    BlockDefinition {
        id: 7,
        name: "bedrock",
        states: CUBE_STATES,
    },
];

/// In-memory [`ResourceSource`] for the benchmark assets.
struct Memory(BTreeMap<String, Vec<u8>>);

impl ResourceSource for Memory {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| ModelError::Resource {
                path: path.to_string(),
                message: "benchmark resource is missing".to_string(),
            })
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.0
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }
}

fn model_context() -> ModelMeshContext {
    let mut files = BTreeMap::new();
    files.insert(
        "assets/minecraft/models/block/cube.json".to_string(),
        CUBE_MODEL.replace("#all", "blocks/stone").into_bytes(),
    );
    files.insert(
        "assets/minecraft/blockstates/cube.json".to_string(),
        CUBE_BLOCKSTATE.as_bytes().to_vec(),
    );
    let set = BlockModelSet::build(&Memory(files), CUBE_DEFS)
        .expect("the synthetic benchmark model set must build");
    ModelMeshContext {
        models: Arc::new(set),
        atlas: Arc::new(AtlasLayout::from_keys(["blocks/stone"])),
    }
}

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

fn mesh_flat_chunk_with_models(criterion: &mut Criterion) {
    let centre = ChunkPos::new(0, 0);
    let columns = (-1..=1)
        .flat_map(|dz| (-1..=1).map(move |dx| ChunkPos::new(dx, dz)))
        .map(|pos| Arc::new(flat_column(pos)))
        .collect::<Vec<_>>();
    let snapshot = ChunkSnapshot::from_columns(centre, columns);
    let context = model_context();
    criterion.bench_function("mesh/models", |bencher| {
        bencher.iter(|| {
            black_box(mesh_chunk_with_models(
                centre,
                black_box(&snapshot),
                black_box(&context),
            ))
        });
    });
}

fn mesh_flat_view(criterion: &mut Criterion) {
    const VIEW_CHUNKS: i32 = 13; // PandaSpigot view-distance 6
    let columns = (0..VIEW_CHUNKS)
        .flat_map(|z| {
            (0..VIEW_CHUNKS).map(move |x| {
                (
                    ChunkPos::new(x, z),
                    Arc::new(flat_column(ChunkPos::new(x, z))),
                )
            })
        })
        .collect::<Vec<_>>();
    let by_pos = columns
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    let snapshots = columns
        .iter()
        .map(|(pos, _)| {
            let neighbours = (-1..=1)
                .flat_map(|dz| (-1..=1).map(move |dx| ChunkPos::new(pos.x + dx, pos.z + dz)))
                .filter_map(|neighbour| by_pos.get(&neighbour).cloned())
                .collect::<Vec<_>>();
            (*pos, ChunkSnapshot::from_columns(*pos, neighbours))
        })
        .collect::<Vec<_>>();
    criterion.bench_function("mesh/flat_view_169_chunks", |bencher| {
        bencher.iter(|| {
            let mut faces = 0;
            for (pos, snapshot) in &snapshots {
                faces += black_box(mesh_chunk(*pos, black_box(snapshot))).face_count();
            }
            black_box(faces)
        });
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

/// A 16³ stone section with an 8³ air room and a torch-like block-light
/// gradient. Unlike the flat and dense benches, every emitted face samples
/// non-uniform light, so this isolates the M6 smooth-light/AO cost.
fn mesh_lit_cave(criterion: &mut Criterion) {
    let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                let room = (4..12).contains(&x) && (4..12).contains(&y) && (4..12).contains(&z);
                if !room {
                    column.set_block(x, y, z, BlockState::new(1, 0));
                }
            }
        }
    }
    for y in 4..12 {
        for z in 4..12 {
            for x in 4..12 {
                let distance = (x as i32 - 8).abs() + (y as i32 - 8).abs() + (z as i32 - 8).abs();
                column.set_block_light(x, y, z, 14_i32.saturating_sub(distance) as u8);
            }
        }
    }
    let snapshot = ChunkSnapshot::from_columns(ChunkPos::new(0, 0), [Arc::new(column)]);
    criterion.bench_function("mesh/lit_cave_section", |bencher| {
        bencher.iter(|| black_box(mesh_chunk(ChunkPos::new(0, 0), black_box(&snapshot))));
    });
}

criterion_group!(
    benches,
    mesh_flat_chunk,
    mesh_flat_chunk_with_models,
    mesh_flat_view,
    mesh_dense_section,
    mesh_lit_cave
);
criterion_main!(benches);
