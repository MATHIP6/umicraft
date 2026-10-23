# T1 — biome tint implementation card

Branch: `feat/biome-tint` (single workstream, no worktree; merged with
`--no-ff` after the gates)
Depends on: M6 accepted, ADR-0009

## Goal

Make vanilla grayscale block textures pick up the 1.8 biome colour at render
time, without embedding any Mojang asset. The result must match the vanilla
`ColorizerGrass`/`ColorizerFoliage` sampling and the block colour rules for
grass, leaves, vines, tall grass, double plants and water.

## Owned files

- `crates/umicraft-world/src/chunk.rs`, `world.rs`, `lib.rs` (exports)
- `crates/umicraft-mesh/src/tint.rs` (new), `lib.rs`, `models.rs`
- `crates/umicraft-mesh/benches/**`, `crates/umicraft-mesh/tests/**`
- `crates/umicraft-assets/src/colormap.rs` (new), `client_resources.rs`,
  `lib.rs` (exports)
- `crates/umicraft-render/src/renderer.rs`, `terrain.wgsl`,
  `tests/offscreen_terrain.rs`
- `crates/umicraft-client/src/{model_resources.rs,block_demo.rs}`
- `docs/tasks/biome-tint/**`, `docs/decisions/0009-biome-tint.md`,
  `docs/sources.md`, `docs/perf-log.md`, `docs/threading.md`, `README.md`

## Required behaviour

1. **World biome access**
   - `ChunkColumn::set_biome(x, z, id)` writes the `z << 4 | x` cell and
     ignores out-of-range coordinates; `fill_biomes(id)` writes all 256.
   - `World::biome(pos) -> Option<u8>` returns `None` for an unloaded chunk.
   - `BlockSource::biome(x, z) -> Option<u8>` defaults to `None`; `World` and
     `ChunkSnapshot` override it. `None` means “no tint data”, rendered white.
2. **Colormap asset layer**
   - Decode `assets/minecraft/textures/colormap/grass.png` and `foliage.png`
     from the verified jar at runtime into 256×256 RGB maps.
   - `Colormap::sample(temperature, rainfall)` replicates 1.8
     `ColorizerGrass`/`ColorizerFoliage`: `rain *= temp`, then
     `x = (1-temp)*255`, `y = (1-rain)*255`, pixel `y*256 + x`; both inputs
     clamped to `0..=1`.
   - Missing colormaps are not fatal (`Option`); a wrongly sized or undecodable
     colormap is a typed `AssetsError`.
3. **Biome table and classes**
   - Base biome ids `0..=39` with the 1.8 temperature/rainfall table from
     `BiomeGenBase`; mutations `128..=167` delegate to `id & 127`; unknown ids
     fall back to plains (1).
   - Vanilla overrides: swamp grass/foliage `0x6A7039`, swamp water
     `0xE0FFAE`; roofed forest grass `((c & 0x00FEFEFE) + 0x0028340A) >> 1`;
     mesa grass `0x90814D`, foliage `0x9E814D`.
   - Tint classes by block id/metadata: grass block 2, tall grass 31 (meta 0
     dead bush stays white), double plant 175 (meta 2/3 only), leaves 18,
     vines 106 → foliage, water 8/9. Lily pad 111 uses the fixed vanilla
     `0x208030`. Everything else stays white.
   - A quad is tinted only when its model face carries `tintindex`; a face
     without one is white even on a tinted block (the grass side base).
4. **Mesher**
   - `Vertex` gains `pub tint: u32` (`0x00RRGGBB`, white `0x00FF_FFFF`) and
     grows to 44 bytes, still `Pod`/`repr(C)`, 4-byte aligned (ADR-0009).
   - `ModelMeshContext` gains `tints: Option<Arc<BiomeTints>>`.
   - The tint of a tinted model quad is the 3×3 average of the neighbouring
     biome colours at the block column; compute it at most once per block and
     reuse it for every tinted quad of that block. With `tints == None` or
     `biome() == None`, the vertex tint is white.
   - The legacy fallback (water model absent) applies the water class as well;
     `mesh_chunk` without a context stays white.
5. **Renderer**
   - Add the sixth vertex attribute at shader location 5 (`Uint32`) and unpack
     it in `terrain.wgsl`; multiply the sampled texel by `tint / 255` in both
     the opaque and the transparent fragment path.
6. **Client**
   - `ModelResources` builds `BiomeTints` from the optional colormaps and puts
     it in the shared context; the report prints `biome tints : on|off`.
   - The synthetic `--block-demo` scene fills its columns with plains (1) so
     grass/leaves are green.

## Tests

- world: set/fill biome, out-of-range ignored, `World::biome` for loaded and
  unloaded chunks.
- assets: synthetic 256×256 colormap sampling at the four corners and the
  middle; missing colormaps → `None`; wrong size → typed error.
- mesh: vertex size 44; plains grass/foliage colours; swamp/mesa/roofed
  overrides; mutated biome == base; 3×3 average across a biome border;
  untinted faces white; tall-grass/dead-bush/double-plant classes; water
  fallback; model quad without `tintindex` unaffected.
- render (opt-in): a tinted quad is multiplied by the vertex colour; white
  tint keeps the old pixels.
- client: demo columns are plains; tint present in the demo mesh; report
  mentions the colormaps when loaded.

## Gates and handoff

Run the full workspace gates plus the opt-in GPU tests from the task README,
record the Criterion before/after in `docs/perf-log.md`, append the vanilla
sources to `docs/sources.md`, then merge `feat/biome-tint` into `main` with
`--no-ff`. Do not push.
