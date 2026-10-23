# ADR-0009 — Biome tints for terrain

- Status: Implemented
- Date: 2026-10-22
- Deciders: project owner
- Supersedes: none (amends ADR-0008 §2)
- Related: ADR-0001 (reference source policy), ADR-0007 (M5 block models),
  ADR-0008 (M6 lighting/sky/transparency)

## Context

The M5/M6 client draws the real 1.8 models and textures, but grass blocks,
leaves, vines and tall grass are still grey. In 1.8 those textures are
deliberately grayscale and the client multiplies them by a colour sampled from
the biome (`ColorizerGrass`, `ColorizerFoliage`, `BiomeColorHelper`). The
model format already identifies the tinted faces (`tintindex`), and the world
already stores one biome byte per column (`ChunkColumn::biomes`), but the
mesher ignores both and `Vertex` has no colour payload.

The fix touches the frozen M6 vertex contract, so this ADR records the new
contract before the code changes. The colormap PNGs are Mojang assets: they
are read from the verified user jar at runtime and never copied into the
repository or a release (spec §3, ADR-0001).

## Decision

### 1. World biome access

```rust
impl ChunkColumn {
    pub fn set_biome(&mut self, x: usize, z: usize, id: u8);
    pub fn fill_biomes(&mut self, id: u8);
}

impl World {
    pub fn biome(&self, pos: BlockPos) -> Option<u8>;
}
```

- The column index is the existing `z << 4 | x`; out-of-range writes are
  ignored.
- `World::biome` returns `None` for an unloaded chunk.
- `BlockSource` gains

  ```rust
  fn biome(&self, x: i32, z: i32) -> Option<u8> {
      None
  }
  ```

  `None` means “no tint data” and renders white, so the no-assets and
  synthetic paths keep their M5 appearance. `World` and `ChunkSnapshot`
  override the method.

### 2. Vertex payload (amends ADR-0008 §2)

`Vertex` grows one `u32` and is now 44 bytes, still `Pod`/`repr(C)`, 4-byte
aligned:

```rust
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub layer: u32,
    pub light: u32, // packed VertexLight (ADR-0008)
    pub tint: u32,  // 0x00RRGGBB, 0x00FF_FFFF = untinted
}
```

`terrain.wgsl` gets the tint as vertex attribute `@location(5)` (`Uint32`),
converts it to `vec3<f32> / 255.0` and multiplies the sampled array texel in
both the opaque and the transparent fragment path. The tint is constant over a
quad, so the interpolant is `@interpolate(flat)`.

The alternative — packing a 6-bit RGB tint into the unused high bits of
`light` — was rejected: 64 levels per channel would band smooth biome
gradients, and the mesh CPU/GPU cost of 4 extra bytes is measured (see
`docs/perf-log.md`).

### 3. Colormaps (`umicraft-assets`)

```rust
pub struct Colormap { /* 256×256 RGB */ }
impl Colormap {
    pub fn sample(&self, temperature: f32, rainfall: f32) -> [u8; 3];
}

pub struct BiomeColormaps {
    pub grass: Colormap,
    pub foliage: Colormap,
}

impl ClientResources {
    pub fn colormaps(&self) -> Option<&BiomeColormaps>;
}
```

- Entries are `assets/minecraft/textures/colormap/grass.png` and
  `foliage.png`, decoded from the verified client jar at load time.
- Sampling follows 1.8 `ColorizerGrass`/`ColorizerFoliage`:
  `rain *= temp`, `x = (1 - temp) * 255`, `y = (1 - rain) * 255`, index
  `y * 256 + x`; `temp`/`rain` are clamped to `0..=1` first.
- Missing entries leave `colormaps()` as `None` (the old white result). A
  malformed size or PNG is a typed `AssetsError`.
- The colormaps never enter the block atlas and are never written to disk.

### 4. Biome colours (`umicraft-mesh::tint`)

`BiomeTints` is an immutable, 40-entry table (plus mutation aliasing) built
once at resource load:

```rust
pub enum TintClass { Untinted, Grass, Foliage, Water }

pub struct BiomeTints { /* [u8; 3] per base biome and class */ }
impl BiomeTints {
    pub fn from_samplers<G, F>(grass: G, foliage: F) -> Self
    where
        G: Fn(f32, f32) -> [u8; 3],
        F: Fn(f32, f32) -> [u8; 3];

    pub fn color(&self, class: TintClass, biome: u8) -> [u8; 3];
}
```

The table uses the 1.8 `BiomeGenBase` climate (temperature/rainfall) for ids
`0..=39`, with these vanilla overrides:

| Biome | Rule |
|---|---|
| swamp (6) | grass and foliage fixed `0x6A7039`; water `0xE0FFAE` |
| roofed forest (29) | grass `((c & 0x00FEFEFE) + 0x0028340A) >> 1` |
| mesa (37–39) | grass `0x90814D`, foliage `0x9E814D` |
| mutated (128–167) | delegate to `id & 127` |
| unknown | plains (id 1) |

Water is white for every other biome, matching `field_76759_H`. The swamp
grass noise split is collapsed to `0x6A7039`; the 1.8 altitude term in
`getFloatTemperature` is not reproduced. Both deviations are listed in
`docs/sources.md`.

### 5. Tint resolution (`umicraft-mesh`)

- A model quad is tinted only when `ModelQuad::tint` is `Some`; the
  `tintindex` value itself is not interpreted (the 1.8 blocks in scope ignore
  it). A face without `tintindex` stays white, which is what keeps the grass
  side base uncoloured while the overlay is green.
- The class comes from the block state:
  grass block 2; tall grass 31 (meta 0 dead bush excluded); double plant 175
  (meta 2/3 grass/fern only) → `Grass`; leaves 18 and vines 106 → `Foliage`;
  water 8/9 → `Water`; lily pad 111 → fixed `0x208030`; everything else
  untinted.
- The colour is the average of the 3×3 biome colours around the block column,
  matching `BiomeColorHelper`; columns without biome data are skipped and an
  all-`None` neighbourhood is white. The average is computed at most once per
  block and reused by all its tinted quads.
- `ModelMeshContext.tints: Option<Arc<BiomeTints>>` carries the table to the
  mesh workers. It is immutable, so workers still share it without a lock.
- The legacy fallback path applies the class without a `tintindex` (water in
  particular). `mesh_chunk` without a context has no table and stays white.

### 6. Cache invalidation

Biomes arrive only with a full chunk payload. `World::apply_chunk_data`
already returns the replaced chunk (and its neighbours when edges change) as
dirty, and the client re-submits those snapshots, so no new invalidation
channel is needed. Block changes cannot change a biome.

## Consequences

- Grass, leaves, vines, tall grass and swamp water now match 1.8 colours; the
  colour blends across biome borders.
- Vertex memory and bandwidth grow by 10 % (40 → 44 bytes). The mesher pays
  one 3×3 biome lookup per tinted block; the benchmark records the cost.
- The no-assets path, synthetic tests without colormaps and `mesh_chunk` keep
  their previous output (white tint).
- A custom jar without colormaps still renders, just without colour.

## Sources

- `mcp: .../world/ColorizerGrass.java`, `ColorizerFoliage.java` (sampling).
- `mcp: .../world/biome/BiomeGenBase.java` (climate, colour methods,
  registration), `BiomeGenForest.java`, `BiomeGenSwamp.java`,
  `BiomeGenMesa.java`, `BiomeGenMutated.java`, `BiomeColorHelper.java`.
- `mcp: .../block/BlockGrass.java`, `BlockLeaves.java`, `BlockTallGrass.java`,
  `BlockDoublePlant.java`, `BlockVine.java`, `BlockLiquid.java`,
  `BlockLilyPad.java` (tint classes).
- The 1.8.8 jar (runtime evidence): all `leaves_*` and grass textures are
  grayscale; `models/block/leaves.json` and the grass model carry
  `"tintindex": 0`; the colormaps are 256×256.
