# M6-W1 — light sampling, smooth lighting and ambient occlusion

Owner worktree: `/home/mathip/dev/rust/mc-project/.worktrees/m6-light`
Branch: `feat/m6-light`
Depends on: M6 setup on `main` (ADR-0008)

## Goal

Make the world light nibbles readable through `BlockSource` and turn them into
per-vertex smooth light plus corner ambient occlusion for both the legacy cube
path and the baked-model path. Split transparent faces into the second mesh
pass. Keep the no-assets fallback full-bright and usable.

## Owned files

- `crates/umicraft-world/src/light.rs`
- `crates/umicraft-world/src/section.rs`
- `crates/umicraft-world/src/chunk.rs`
- `crates/umicraft-world/src/world.rs`
- `crates/umicraft-world/src/lib.rs` (exports only)
- `crates/umicraft-mesh/src/lib.rs`
- `crates/umicraft-mesh/src/models.rs`
- `crates/umicraft-mesh/benches/**`
- `crates/umicraft-mesh/tests/**`

Do **not** edit render, client, docs or `Cargo.toml`. Report any needed
dependency or `docs/perf-log.md` row to the lead.

## Required behaviour

1. Keep the frozen types and signatures from ADR-0008. Implement the missing
   bodies; do not rename `LightLevel`, `VertexLight`, `Vertex::light`,
   `Mesh::transparent_vertices`, `Mesh::transparent_indices` or
   `BlockSource::light`.
2. `ChunkColumn::light` and setters:
   - nibble reads at section-local coordinates;
   - out-of-range reads `DARK`, writes are ignored;
   - elided sections read `FULL_SKY`;
   - a present section with no sky array reads `sky = 0`;
   - `set_sky_light` allocates a missing sky array;
   - do not drop a section that still carries non-zero light when a block
     update makes its block storage empty. Add a regression test. If a
     light-only section is not retained, a synthetic cave loses its light.
3. `World::light` respects `has_sky_light`: Nether/End sky is zero. Missing
   chunks and out-of-range Y read `DARK`. `ChunkSnapshot::light` reads the
   same nine columns as `block`; decide and document its missing-column
   default (ADR-0008 suggests daylight at the snapshot edge).
4. `BlockSource::light` defaults to `FULL_SKY`, so `mesh_chunk` with a
   synthetic source that has no light remains byte-identical in geometry and
   full-bright. `World` and `ChunkSnapshot` override it.
5. Smooth light per emitted quad:
   - derive the face axis from the face normal (legacy `FaceDef::normal` or
     the model quad normal);
   - for each of the four quad corners, sample the four cells on the outward
     side of the face in the tangent plane and average block and sky light;
   - round the average to the nearest integer nibble and pack it into
     `VertexLight`;
   - do not allocate per corner; use fixed arrays and stack values;
   - the same lookup helper is shared by the legacy and model paths rather
     than duplicated.
6. Ambient occlusion per corner:
   - sample the two edge neighbours and the diagonal neighbour on the
     outward side of the face;
   - `BlockState::is_opaque()` is the occluder predicate (M5 uses the same
     coarse class);
   - `occlusion = 3` when both edges occlude, otherwise the sum of the three
     booleans;
   - `ao = 15 - 5 * occlusion` (15, 10, 5, 0);
   - `ambient_occlusion == false` or `ModelQuad::shade == false` leaves AO
     open at 15;
   - the `shade` flag is carried from `ModelQuad::shade` (legacy cubes use
     `true`). Do not drop the existing normal/AO rule when a quad is a model
     cross or a plant.
   - cite `BlockModelRenderer.AmbientOcclusionFace` and
     `WorldProvider.generateLightBrightnessTable` in module comments; the
     lead adds the `docs/sources.md` row.
7. Transparent routing:
   - `BlockState::kind()` `Transparent` or `Liquid` appends to the transparent
     vertex/index vectors; all other rendered states append to opaque;
   - `Mesh::face_count`/`triangle_count`/`is_empty` include both passes;
   - the culling rule is unchanged from ADR-0007;
   - the no-assets path routes water/glass/leaves the same way.
8. Keep the hot path allocation-free apart from the final mesh vectors. Do not
   build a temporary `Vec` per block/face. Avoid hashing more than the M5
   path already does.

## Tests

Add unit tests and at least one integration test:

- `LightLevel` clamping and `combined` (already present; extend if needed);
- section/chunk getters and setters, out-of-range access, missing sky array;
- elided section reads full sky; present section without sky reads zero;
- Nether `World::light` forces sky zero and missing chunks read dark;
- a light-only section survives an air block update;
- `ChunkSnapshot::light` reads a neighbour and the snapshot edge default;
- a single lit block at a corner produces the expected `ao` values 15/10/5/0
  on a face;
- a sky-lit vs. cave vertex gets different `VertexLight.sky`;
- a smooth light corner averages across the four cells, not just the block
  under the face;
- `ModelQuad::shade == false` and `ambient_occlusion == false` set open AO;
- stone is opaque, water/glass/leaves are transparent, and the two counts add
  up to the previous single-pass face count;
- the no-assets `mesh_chunk` geometry/indices are unchanged when the source has
  no light data (full-bright geometry is acceptable; positions/indices must
  match).
- The existing model-mesh tests must be updated for the new fields; do not
  weaken them.

## Benchmark

Add a Criterion benchmark (or extend `benches/mesh.rs`) for the flat chunk and
the dense 16³ section through `mesh_chunk`, plus the model path if it does not
make the suite too slow. Use release, `--quick` is acceptable for the handoff.
Record the raw numbers in the handoff and send them to the lead for
`docs/perf-log.md`. Do not edit the perf log yourself.

## Gates

```sh
cargo fmt -p umicraft-world -p umicraft-mesh -- --check
cargo clippy -p umicraft-world -p umicraft-mesh --all-targets -- -D warnings
cargo test -p umicraft-world -p umicraft-mesh
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-world -p umicraft-mesh --no-deps
cargo bench -p umicraft-mesh --bench mesh -- --quick
```

Report the commit SHA, test count, benchmark numbers and any frozen-API
concern. Do not merge.
