# ADR-0007 — M5: the 1.8 model system and the texture atlas

- Status: Accepted
- Date: 2026-10-22
- Deciders: project owner
- Supersedes: none
- Related: ADR-0001 (reference source policy), ADR-0006 (M4 UI)

## Context

M4 is complete. M5 (spec §12.1) turns the coarse M2 cube mesher into a real
1.8 block renderer:

- parse the 340 blockstates (`variants` only; 1.8 has no `multipart`), the
  1080 `models/block` documents, the `parent` chain, `textures`,
  `ambientocclusion` and the `elements` format used by 252 models;
- support the parent templates (`cube_all`, `cube`, `cube_column`,
  `cube_bottom_top`, `cross`, `crop`, `half_slab`, `upper_slab`, `stairs`,
  inner/outer stairs, panes, carpet, fence, fence gate, iron bars, plant,
  torch, rail, ladder, vine, snow layer, cactus, water, leaves) through the
  generic model format, not hard-coded per-template code;
- map block id + metadata to a blockstate document and variant key;
- decode the 382 block and 229 item texture entries (373 + 227 PNGs plus their
  `.mcmeta` animation files) from the verified client jar into one texture
  array;
- make the mesher emit model quads and the renderer sample the real layers.

The M4 font loader already proves the legal boundary: Mojang assets are read
from the user's installation at runtime and never copied into the repository
or a release. The same rule applies here. The block-id table is data derived
from the reference implementation under ADR-0001 and is cited in
`docs/sources.md`; the table is original source code, not an extracted asset.

## Decision

### New crate: `umicraft-model`

Add one pure crate between `umicraft-world` and the renderer:

```text
world ──▶ model ──▶ mesh ──▶ render ──▶ client
             ▲
assets ──────┘ (ResourceSource + atlas pixels)
```

`umicraft-model` performs no I/O, owns no GPU resource and has no dependency on
`winit`, `wgpu`, `reqwest` or `png`. It uses `serde_json`, `thiserror` and
`umicraft-world::BlockState`.

### Frozen public API

```rust
pub trait ResourceSource {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>>;
    fn list(&self, prefix: &str) -> Vec<String>;
}

pub struct BlockModelSet { /* ... */ }
impl BlockModelSet {
    pub fn build(
        source: &dyn ResourceSource,
        definitions: &[BlockDefinition],
    ) -> ModelResult<Self>;
    pub fn model(&self, state: BlockState) -> Option<&BlockModel>;
    pub fn state_count(&self) -> usize;
    pub fn model_count(&self) -> usize;
    pub fn blockstate_count(&self) -> usize;
    pub fn source_model_count(&self) -> usize;
    pub fn texture_keys(&self) -> &BTreeSet<String>;
}

pub struct BlockModel { pub variants: Vec<ModelVariant> }
pub struct ModelVariant { pub model: Arc<ResolvedModel>, pub weight: u32 }
pub struct ResolvedModel {
    pub quads: Vec<ModelQuad>,
    pub ambient_occlusion: bool,
    pub particle: Option<String>,
}

pub struct ModelQuad {
    pub positions: [[f32; 3]; 4], // block-local, CCW from outside
    pub normal: [f32; 3],
    pub uvs: [[f32; 2]; 4],       // normalized, v=0 at texture top
    pub texture: String,          // "blocks/stone"
    pub cullface: Option<Face>,
    pub shade: bool,
    pub tint: Option<u32>,
}

pub enum Face { West, East, Down, Up, North, South }

pub struct AtlasLayout { /* ... */ }
impl AtlasLayout {
    pub fn from_keys<I, S>(keys: I) -> Self; // missing layer 0, rest sorted
    pub fn layer(&self, key: &str) -> Option<u32>;
    pub fn keys(&self) -> &[String];
    pub fn len(&self) -> u32;
}

pub struct BlockDefinition {
    pub id: u16,
    pub name: &'static str,
    pub states: &'static [StateDef],
}
pub struct StateDef {
    pub mask: u8,
    pub value: u8,
    pub blockstate: &'static str,
    pub variant: &'static str,
}
pub fn definitions() -> &'static [BlockDefinition];
pub fn definition(id: u16) -> Option<&'static BlockDefinition>;
```

`BlockStateFile::parse` and `BlockStateFile::select` are public for tests and
tooling. `BlockStateFile` canonicalizes every variant key by sorting its
`name=value` pairs, so `a=1,b=2` and `b=2,a=1` select the same variant.

### Geometry contract

- Element `from`/`to` values are divided by 16 into block-local space. Values
  outside `0..16` are allowed (crop starts at `y = -1`).
- Quad corners use the vanilla `EnumFaceDirection` order, which is
  counter-clockwise seen from outside and matches the existing wgpu front-face
  convention. The mesher emits two triangles
  `(0,1,2)` and `(0,2,3)`.
- UVs are normalized to the texture (`0..1`), with `v = 0` at the top. The
  atlas layer supplies the texture; `uv` is not atlas-wide.
- Element rotation and blockstate variant `x`/`y` rotation are applied around
  the element origin and the block centre respectively. `uvlock` derives UVs
  from the rotated positions using the 1.8 `FaceBakery` formula.
- A face with `cullface` is emitted only when the neighbouring block in that
  direction is a full opaque cube or the same transparent/liquid state. A face
  without `cullface` is always emitted.
- Every `ModelQuad.texture` is concrete; `#` references must be resolved while
  building. Missing references are a hard `ModelError`, not a silent layer 0.
- The texture key in `ModelQuad` is un-prefixed (`blocks/stone`); the atlas
  layer is resolved by `AtlasLayout`.

### Block registry

`registry.rs` contains a static, generated-from-reference table for block ids
`0..=197`. For each id it records the registry name and a small set of
metadata rules:

```rust
StateDef { mask: 0, value: 0, blockstate: "stone", variant: "normal" }
StateDef { mask: 12, value: 4, blockstate: "log_oak", variant: "axis=x" }
StateDef { mask: 3, value: 0, blockstate: "leaves_oak", variant: "normal" }
```

First matching rule wins; metadata bits outside the mask are ignored. A state
whose key is absent from its document falls back to the document's first
variant and increments a reported fallback count, so one unknown property never
opens a hole in the world. Blocks with no blockstate document (technical blocks)
are omitted and use the legacy cube fallback.

The table is derived from `Block.java` registrations and each block's
`getStateFromMeta`; the exact MCP symbols and the acceptance cases are cited in
`docs/sources.md`. No Mojang JSON, PNG or class file is copied into the repo.

### Atlas contract

`umicraft-assets` gains:

```rust
pub struct ClientResources { /* in-memory virtual files */ }
impl ClientResources {
    pub fn load(jar: &ClientJar) -> AssetsResult<Self>;
    pub fn get(&self, path: &str) -> Option<&[u8]>;
    pub fn file_count(&self) -> usize;
    pub fn atlas(&self) -> &BlockAtlas;
}
impl ResourceSource for ClientResources { /* ... */ }

pub struct BlockAtlas { /* ... */ }
impl BlockAtlas {
    pub fn width(&self) -> u32;
    pub fn height(&self) -> u32;
    pub fn layers(&self) -> u32;
    pub fn pixels(&self) -> &[u8];  // layer-major RGBA8
    pub fn layout(&self) -> &AtlasLayout;
}
```

The jar is read once. `ClientResources::load` reads all
`assets/minecraft/blockstates/*.json`, `models/block/*.json`,
`textures/blocks/*.png` and `textures/items/*.png`, decodes each PNG to RGBA8,
and builds the atlas in `AtlasLayout` order. Layer 0 is the original Umicraft
missing-texture checker. The 373 block and 227 item PNGs give 600 layers; the
11 `.mcmeta` animation files are counted as texture entries but do not add a
layer. The atlas edge is the largest frame width in the source (32 for the
1.8 jar, because flowing water/lava use 32×32 frames); every layer is
nearest-neighbour normalized to that edge, and a tall animated strip
(water, lava, fire, portal, clock, compass) contributes its first frame. Item textures are loaded now so M8
does not reopen the jar; no M5 block model references them.

### Mesher and renderer contract

`umicraft-mesh` adds:

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

The legacy `mesh_chunk` and procedural texture layers stay as the no-assets
fallback. `mesh_chunk_with_models` chooses a weighted variant deterministically
from the block position, culls `cullface` quads, and writes the atlas layer.
A state with no model still uses the legacy cube geometry. If the real atlas is
present, water and lava sample their `water_still`/`water_flow` and
`lava_still`/`lava_flow` keys; every other no-model state uses the
missing-texture checker. Without an atlas (the no-assets fallback) the
procedural layer table is used unchanged.

`umicraft-render` adds:

```rust
pub struct BlockAtlasError(/* typed reason */);
impl Renderer {
    pub fn set_block_atlas(
        &mut self,
        width: u32,
        height: u32,
        layers: u32,
        pixels: &[u8],
    ) -> Result<(), BlockAtlasError>;
}
```

The method validates the buffer and the adapter's
`max_texture_array_layers`/`max_texture_dimension_2d`, creates a new
`texture_2d_array`, writes the layers once and replaces the terrain bind group.
On failure the renderer keeps its procedural array. The terrain shader gains
alpha cutout (`discard` below 0.5) so leaves, plants and glass holes render
without introducing M6 transparency sorting. Water remains an opaque cutout for
M5; blending/sorting is M6.

The client keeps a single `Option<ModelResources>`:
`Some` uses `mesh_chunk_with_models` and `set_block_atlas`; `None` keeps the
legacy path. This is also the fallback when the official installation is absent
or the jar is corrupt.

### Acceptance path

Add a server-less `--block-demo` mode next to `--ui-demo`:

```sh
cargo run -p umicraft-client -- --block-demo
cargo run -p umicraft-client -- --block-demo --screenshot /tmp/m5-blocks.png
```

The scene loads the verified 1.8.8 jar, builds the atlas and model set, places
the acceptance blocks on a lit platform, meshes the chunk with the model
mesher, renders offscreen and prints:

```text
blockstates       : 340
source models     : 1080
texture entries   : 611 (373 block PNG + 9 mcmeta, 227 item PNG + 2 mcmeta)
texture layers    : 601 (600 PNG frames + missing)
atlas             : 32x32 RGBA8, 2,461,696 bytes
block states      : ...
quads             : ...
fallback variants : ...
```

The scene must include stone, dirt, grass, planks, cobblestone, sandstone,
bricks, glass, leaves, water, every ore, obsidian, glowstone, all 16 wool
colours, pumpkin, melon, netherrack, quartz, and a shape row with bottom/top
slabs, stairs, fence, pane, torch, carpet, snow layer, cactus, ladder, rail and
vine. A synthetic unit test asserts that a bottom slab is 8/16 high, a top slab
is 8/16 high, stairs contain more than six quads, and a cross contains four
quads. The screenshot and report are the M5 evidence; no live server is
required.

## Ownership map

| Stream | Owns | May not touch |
|---|---|---|
| W1 model core | `umicraft-model/src/{lib,error,blockstate,model,geometry}.rs`, `crates/umicraft-model/tests/**` | `registry.rs`, `atlas.rs`, other crates |
| W2 registry | `umicraft-model/src/registry.rs`, `crates/umicraft-model/tests/registry.rs` | parser/geometry files |
| W3 atlas/resources | `umicraft-assets/src/{atlas,client_resources,client_jar,error,lib}.rs`, its tests | model crate, mesh/render/client |
| W4 mesher | `umicraft-mesh/**`, `docs/perf-log.md` | model/assets/render/client |
| W5 renderer | `umicraft-render/**` | mesh/client/assets |
| W6 client/acceptance | `umicraft-client/**`, `README.md`, `docs/threading.md` | other crates |

The lead owns the workspace `Cargo.toml`/`Cargo.lock`, this ADR, task cards,
`docs/sources.md` and integration fixes. A stream that needs a dependency or a
frozen API change stops and asks; it does not edit another owner's files.

## Integration order and gates

Nominal order is **W1 → W2 → W3 → W4 → W5 → W6**. The setup skeleton lands
first. W1/W2/W3 form wave 1; W4/W5 form wave 2; W6 runs after both. Deviations
must be recorded here before merge.

Every stream runs `cargo fmt`, `cargo clippy -p <crate> --all-targets -- -D
warnings`, `cargo test -p <crate>`, and `cargo doc -p <crate> --no-deps` before
handing back. Model tests use synthetic JSON only. The official 1.8.8 jar is
used only by opt-in acceptance/manual tests outside tracked files.

The lead then runs on `main`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
cargo test -p umicraft-render --test offscreen_ui -- --ignored
```

plus `--block-demo`, `--block-demo --screenshot`, `--ui-demo` and the existing
GPU smoke tests. `git ls-files` must show no Mojang assets.

## Consequences

- One new pure crate and four dependency edges (`assets→model`, `mesh→model`,
  `client→model`, `client→assets` already existed).
- Startup reads and decodes the jar once; windowed play and the demo share the
  immutable model/atlas.
- Memory: about 2.4 MiB of texture pixels (601 layers at 32×32 RGBA8), plus
  parsed JSON/quad data.
- The renderer keeps a 19-layer fallback until `set_block_atlas` succeeds, so a
  missing installation, corrupt jar or low texture-array limit degrades to the
  M2 look instead of failing.
- M5 does not add biome tint, smooth lighting, transparent sorting or fog; those
  remain M6. Alpha cutout is the minimum needed for plants and glass.

## Acceptance checklist

- [ ] 340 blockstates and 1080 source models parsed and counted.
- [ ] 382 block + 229 item texture entries inventoried, 600 PNG frames
      decoded into 601 layers (including the missing layer).
- [ ] Common acceptance blocks use real blockstate/model shapes and textures.
- [ ] Bottom/top slabs are 8/16 high; stairs/fence/pane/torch/rail/cross have
      non-cube geometry.
- [ ] Windowed client and offscreen screenshot share the same model/atlas path.
- [ ] No official installation falls back to the procedural M2 renderer.
- [ ] No Mojang asset is tracked by git.
- [ ] Full workspace gates and GPU smoke tests pass.
