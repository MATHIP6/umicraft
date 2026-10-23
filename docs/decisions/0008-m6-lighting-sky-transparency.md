# ADR-0008 — M6: lighting, sky, fog and transparency

- Status: Implemented (2026-10-22)
- Date: 2026-10-22
- Deciders: project owner
- Supersedes: none
- Related: ADR-0001 (reference source policy), ADR-0007 (M5 block models)

## Context

M5 is complete: the client renders the real 1.8 block models and texture array,
but every face is still drawn with a coarse normal-based shade, the sky is a
constant clear colour, fog ignores the world clock and all geometry is opaque.
M6 (spec §12.2, RND-6/7/8) must use the light nibbles already carried by the
world, add sky/sun/moon/clouds/fog, and split transparent geometry into a
correctly ordered pass.

The milestone is too large for one contributor. This ADR freezes the shared
types and the file ownership before the workstreams start (spec §14.11).

## Decision

### 1. World light access (`umicraft-world`)

The world exposes one plain `Copy` sample type and read/write helpers. Values
are nibbles; constructors and setters clamp to `0..=15`.

```rust
pub struct LightLevel {
    pub block: u8,
    pub sky: u8,
}

impl LightLevel {
    pub const DARK: Self;
    pub const FULL_SKY: Self;
    pub const fn new(block: u8, sky: u8) -> Self;
    pub const fn combined(self) -> u8;
}

impl ChunkColumn {
    pub fn light(&self, x: usize, y: usize, z: usize) -> LightLevel;
    pub fn set_block_light(&mut self, x: usize, y: usize, z: usize, level: u8);
    pub fn set_sky_light(&mut self, x: usize, y: usize, z: usize, level: u8);
}

impl World {
    pub fn light(&self, pos: BlockPos) -> LightLevel;
}
```

Rules:

- out-of-range coordinates read `LightLevel::DARK`;
- an elided all-air section reads full sky light (the server usually omits it
  above the terrain);
- a present section without a sky array reads `sky = 0` (Nether/End);
- `World::light` forces `sky = 0` when `World::has_sky_light()` is false;
- `set_sky_light` may allocate the absent nibble array, so the acceptance demo
  can synthesize a cave independently of the daylight above it;
- light-only sections must not be discarded by the M6 mesher or by block
  updates. If this requires keeping a section that has no non-air blocks, the
  lighting workstream changes the drop rule and covers it with a regression
  test.

### 2. Vertex payload and mesh passes (`umicraft-mesh`)

`Vertex` grows one `u32` and stays `Pod`/`repr(C)` at 40 bytes:

```rust
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub layer: u32,
    pub light: u32, // packed VertexLight
}

pub struct VertexLight {
    pub block: u8, // 0..=15
    pub sky: u8,   // 0..=15
    pub ao: u8,    // 0..=15, 15 = open
    pub shade: bool,
}

impl VertexLight {
    pub const DAY: Self;
    pub const AO_OPEN: u8;
    pub const fn new(block: u8, sky: u8, ao: u8, shade: bool) -> Self;
    pub const fn pack(self) -> u32;
    pub const fn unpack(packed: u32) -> Self;
}
```

Packing, consumed by `terrain.wgsl`:

| bits | field |
|---|---|
| 0..=3 | block light |
| 4..=7 | sky light |
| 8..=11 | AO (`0` dark, `15` open) |
| 12 | `shade` (1 = apply the 1.8 face shade factor) |

`Mesh` gains a second vertex/index pair:

```rust
pub struct Mesh {
    pub vertices: Vec<Vertex>,             // opaque pass
    pub indices: Vec<u32>,
    pub transparent_vertices: Vec<Vertex>,
    pub transparent_indices: Vec<u32>,
}
```

- `Mesh::is_empty`, `face_count` and `triangle_count` cover both passes;
  `opaque_face_count` and `transparent_face_count` expose each pass.
- A face whose block `BlockState::kind()` is `Transparent` or `Liquid` goes to
  the transparent vectors. Water, glass and leaves therefore share one pass;
  the M5 cull rule is unchanged.
- `BlockSource` gains
  `fn light(&self, x: i32, y: i32, z: i32) -> LightLevel` with a default of
  `FULL_SKY`. This keeps the M2 no-assets path and synthetic test sources
  full-bright; `World` and `ChunkSnapshot` override it with real nibbles.
- Light and AO are sampled per vertex. For each face corner, the mesher
  samples the four cells on the outward side of the face and averages their
  block and sky light. AO uses the three occluding cells around that corner:
  two sides and the diagonal. The exact corner mapping follows
  `BlockModelRenderer.AmbientOcclusionFace` (source below); the result is
  quantized to `0/5/10/15` for zero/one/two/three occluders. Model quads with
  `ambient_occlusion == false` or `ModelQuad::shade == false` keep AO open;
  `shade` is still carried to the shader.
- The smooth-lighting sampler is only run for emitted faces, not for every
  air cell. The hot path must not allocate per face.

### 3. Render passes and ordering (`umicraft-render`)

The main render pass is ordered:

1. **Sky pass** — fullscreen sky, sun/moon and procedural clouds, no depth
   writes.
2. **Opaque terrain** — depth test `Less`, depth write on, blend `REPLACE`.
3. **Transparent terrain** — depth test `LessEqual`, depth write off,
   premultiplied or straight alpha blending (the textures use straight alpha;
   `SrcAlpha`/`OneMinusSrcAlpha` is the default), back-face culling on.
4. Overlay/UI as today.

Renderer state is frozen as:

```rust
pub struct RenderEnvironment {
    pub dimension: i8,
    pub world_time: i64,
    pub partial_tick: f32, // clamped 0.0..1.0
    pub view_distance: u8,
}

impl Renderer {
    pub fn set_environment(&mut self, environment: RenderEnvironment);
}
```

The transparent pass sorts chunk meshes by squared distance from the camera to
the chunk centre, farthest first, every frame. Each chunk keeps its opaque and
transparent GPU buffers separate. For a deterministic tie-break, sort by
distance and then by `ChunkPos`; do not rebuild vertex buffers per frame.

The transparent shader samples `texel.a`; it must not use the opaque `a < 0.5`
discard. Glass/leaves have binary alpha and water has real alpha in the 1.8
texture, so blending preserves water translucency. The opaque pass keeps the
existing alpha cutout.

Fog:

- fog target is `SkyState::fog_color`, mixed with the fragment's light so a
  cave does not show a bright blue haze;
- fog distance derives from `view_distance * 16`: start at roughly 75 % and
  end at the full view distance, clamped to the camera far plane;
- at night the fog target follows the night sky colour.

Lighting in `terrain.wgsl`:

- unpack the vertex payload;
- use the 1.8 light-brightness table
  `b(level) = level_normalized / (3 * (1 - level_normalized) + 1)` where
  `level_normalized = level / 15`;
- sky contribution is `b(sky) * sun_brightness`;
- block contribution is `b(block) * torch_factor` (torch factor starts at
  `1.0`; flicker is optional and must be deterministic in tests);
- combine as `min(sky + block, 1.0)` then apply AO and the optional face shade;
- the existing axis shade remains the `shade == true` fallback.

Sky/time model (`umicraft-render/src/sky.rs`, internal except `SkyState`):

```rust
pub struct SkyState {
    pub celestial_angle: f32,
    pub sun_direction: [f32; 3],
    pub moon_direction: [f32; 3],
    pub sky_color: [f32; 3],
    pub fog_color: [f32; 3],
    pub cloud_color: [f32; 4],
    pub star_alpha: f32,
    pub sun_brightness: f32,
    pub moon_phase: u8,
}
impl SkyState {
    pub fn at(environment: RenderEnvironment) -> Self;
}
```

`SkyState` follows the 1.8 formulas in `WorldProvider.calculateCelestialAngle`,
`World.getSunBrightness`, `World.getSkyColor`, `World.getFogColor`,
`World.getCloudColour` and `World.getStarBrightness`, with rain/thunder fixed
to zero in M6 (weather is a later milestone). The sun and moon are original
procedural discs; clouds are an original hash/noise layer at y = 128. No Mojang
texture or shader is read or copied.

### 4. Client clock (`umicraft-client`)

`Time Update` (0x03) is already decoded by `umicraft-protocol`; M6 exposes it
through `NetEvent::TimeUpdate` and stores the latest world time on the game
thread. The render-side frame gains:

```rust
pub struct Frame {
    pub state: RenderState,
    pub tick_time: Instant,
    pub world_time: i64,
    pub dimension: i8,
}
```

The render thread builds `RenderEnvironment` from the newest frame, clamps the
interpolated `partial_tick`, and calls `Renderer::set_environment` once per
frame. The signed `time_of_day` is preserved; negative values mean a frozen
cycle (the server negates the field). `SkyState` uses Euclidean modulo 24 000
so negative values remain well-defined.

### 5. Performance and acceptance contract

The M2 baselines in `docs/perf-log.md` stay the reference:
`mesh/flat_chunk_with_neighbours` 446.7 µs, `mesh/dense_16cube` 685.7 µs,
`mesh/flat_view_169_chunks` 114.3 ms. M6 adds lighting samples and a second
index pass; the frame-time acceptance is **≤ 110 % of the M2 baseline**. The
lighting workstream records fresh Criterion numbers in its handoff; the
acceptance workstream appends the before/after table and a short explanation.
The renderer must not sort or allocate per vertex per frame; transparent
sorting is one `Vec` of chunk handles per frame, bounded by loaded meshes.

Acceptance evidence:

- a server-less `--block-demo` scene with open ground, a lit cave, water, glass
  and leaves, run at day and night;
- screenshots show the AO corners, the night sky and the cave darkness;
- transparent water/glass/leaves are visibly blended and order-independent at
  the chunk level;
- all workspace gates, the opt-in GPU tests and the no-asset audit pass.

## File ownership (frozen)

| Workstream | Owns | Must not edit |
|---|---|---|
| W1 lighting | `crates/umicraft-world/src/{light,section,chunk,world,lib}.rs`; `crates/umicraft-mesh/src/{lib,models}.rs`; `crates/umicraft-mesh/benches/**`; `crates/umicraft-mesh/tests/**` | render, client, docs |
| W2 terrain renderer | `crates/umicraft-render/src/{renderer.rs,terrain.wgsl,transparency.rs}`; `crates/umicraft-render/tests/offscreen_terrain.rs` | world, mesh, client, `sky.rs`, `lib.rs` |
| W3 sky & clock | `crates/umicraft-render/src/{sky.rs,sky.wgsl}`; `crates/umicraft-client/src/{game.rs,network.rs,window.rs}` | world, mesh, `renderer.rs`, `terrain.wgsl`, `block_demo.rs`, `args.rs` |
| W4 acceptance | `crates/umicraft-client/src/{block_demo.rs,args.rs,screenshot.rs}`; `crates/umicraft-client/tests/**`; `docs/perf-log.md` | world, mesh, render, `game.rs`, `network.rs`, `window.rs` |

The setup commit added the shared `LightLevel`, `VertexLight`, `Vertex.light`,
`Mesh` transparent fields, `RenderEnvironment`, `SkyState` and `SkyRenderer`
signatures. Workstreams may change the bodies but not these names or layouts
without a new ADR entry.

## Non-goals

- a full incremental light engine (world light propagation is still server
  driven; M6 only reads/uses the nibbles and allows synthetic setters);
- biome tint, weather/rain/thunder, translucent item rendering;
- per-quad view-dependent transparency sorting; chunk-level ordering plus the
  stable pass split is the M6 contract;
- resource-pack shaders or Mojang sky textures.

## Sources

Per ADR-0001 the reference tree is consulted and cited, not copied:

- smooth light and AO: `mcp: src/minecraft/client/renderer/BlockModelRenderer.java`
  (`AmbientOcclusionFace`, `renderModelAmbientOcclusionQuads`),
  `mcp: src/minecraft/world/WorldProvider.java` (`generateLightBrightnessTable`);
- lightmap combination: `mcp: src/minecraft/client/renderer/EntityRenderer.java`
  (`updateLightmap`, `updateTorchFlicker`);
- time, sky, fog, clouds: `mcp: src/minecraft/world/WorldProvider.java`
  (`calculateCelestialAngle`, `getFogColor`, `getCloudHeight`),
  `mcp: src/minecraft/world/World.java`
  (`getSkyColor`, `getSunBrightness`, `getCloudColour`, `getStarBrightness`);
- sky geometry: `mcp: src/minecraft/client/renderer/RenderGlobal.java`
  (`renderSky`);
- transparency: `mcp: src/minecraft/client/renderer/RenderGlobal.java`
  (`renderBlockLayer`, reverse render-info order for `TRANSLUCENT`),
  `mcp: src/minecraft/client/renderer/chunk/RenderChunk.java`
  (`postRenderBlocks`);
- time wire field and frozen-cycle sign: `umicraft-protocol` `time_update.rs`
  (captured 0x03 shape, protocol docs).

## Consequences

- Every generated vertex is 4 bytes larger; the M2 mesh benchmark will be
  re-recorded.
- Transparent chunks need two GPU buffers and a per-frame distance sort. The
  sort is bounded by the loaded chunk count and does not touch the world.
- W3 can implement the sky pass without editing `renderer.rs`; W2 calls the
  frozen `SkyRenderer` hooks. Any signature change is a lead decision.
- The M6 acceptance run needs the official installation for real textures but
  must keep the procedural fallback usable without it.
