# M5-W4 — model mesher

Owner worktree: `.worktrees/m5-mesher`
Branch: `feat/m5-mesher`
Depends on: W1 model core and W2 registry merged into `main`

## Goal

Emit chunk meshes from baked model quads and atlas layers, keeping the legacy
cube mesher as the no-assets fallback.

## Owned files

- `crates/umicraft-mesh/**`
- `docs/perf-log.md` (only if adding a benchmark)

Do not edit model/assets/render/client.

## Required API

```rust
pub struct ModelMeshContext {
    pub models: Arc<BlockModelSet>,
    pub atlas: Arc<AtlasLayout>,
}

pub fn mesh_chunk_with_models<S: BlockSource + ?Sized>(
    pos: ChunkPos,
    source: &S,
    context: &ModelMeshContext,
) -> Mesh;
```

Keep `mesh_chunk`, `texture_layer`, `TEXTURE_LAYERS`, `Vertex`, `Mesh`, `Face`,
`BlockSource`, `ChunkSnapshot` source-compatible.

## Behaviour

- Iterate the same 16×256×16 column.
- For air, skip. For a state with a baked model:
  - choose a weighted variant deterministically from world coordinates;
  - for each `ModelQuad`, if `cullface` is set, query the neighbour and skip
    when it is a full opaque cube or the same transparent/liquid state;
  - add the block position to the local corners;
  - use `quad.normal`, normalized `quad.uvs`, and
    `context.atlas.layer(&quad.texture).unwrap_or(0)`;
  - keep CCW winding and two triangles per quad.
- A state with no model but a known block id uses the old cube path, so
  technical/unknown ids still render.
- Do not emit duplicate interior faces for full cubes; the `cullface` quads
  from the real cube model handle it.
- Keep the hot path allocation-free except the output vectors; no per-block
  JSON/model parsing.

## Tests

- Use a synthetic `BlockModelSet` built from in-memory JSON and a 2-layer
  `AtlasLayout`.
- stone cube: 6 faces, correct layer, chunk-boundary neighbour culling;
- bottom slab: face heights only 0..0.5 and no full-cube side;
- cross: 4 quads, no cull, correct layer;
- glass: same-state neighbours cull; adjacent stone does not;
- modelless id 1 falls back to the old cube;
- weighted variants are deterministic for a fixed position and vary across
  positions;
- mesher output is deterministic across two runs.

Add a criterion benchmark `mesh/models` only if it stays under a reasonable
time; record before/after in `docs/perf-log.md` if the old path changes. The
default `bench` must still compile.

## Gates

```sh
cargo fmt -p umicraft-mesh -- --check
cargo clippy -p umicraft-mesh --all-targets -- -D warnings
cargo test -p umicraft-mesh
cargo bench -p umicraft-mesh --no-run
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-mesh --no-deps
```

Report the commit SHA, test count, quad counts for the slab/cross cases. Do not
merge.
