# F2 — Performance pass: audit and plan

Status: **phases 0, 1 and 2 done** (F2-2c deferred); phases 3–5 planned. Decision: `docs/decisions/0058-f2-performance-plan.md`.
Roadmap entry: `ROADMAP.md` §6 (F2).

The owner started F2 ahead of P13 (2026-11-07) for two reasons:

1. the client renders about as many frames per second as vanilla, where the
   spec (§7) expects a clear lead;
2. far from the origin (|x| or |z| around 1 000 000) the game looks less
   smooth.

The audit below is a code reading at `debffee`. **Nothing has been measured
yet**, so Phase 0 adds the instruments first. Every later step must record
before/after numbers in `docs/perf-log.md` (AGENTS.md, spec §7). Line numbers
are the ones at `debffee`.

## Phase 0 results (2026-11-07)

Baseline in `docs/perf-log.md`; run `umicraft-client --release --bench-scene
[--bench-origin 1000000]`. Findings that shape the next steps:

- 289 columns, 518k faces: about 4.0 ms per frame (250 fps), GPU 2.5 ms.
- **Opaque terrain is 82 % of the GPU time** (2.05 of 2.50 ms): phases 2 and 3
  target exactly this.
- The frame time is the same at x = 0 and x = 1 000 000: the far-from-origin
  problem is precision only (phase 1), as audited.
- `lto = "thin"` gives nothing here (not kept).
- CPU spans: `--features profiling` and `puffin_viewer` (ADR-0059).

## Audit

### A. Far from the origin: precision, not CPU

No CPU cost grows with the size of the coordinates. The symptom is visual
jitter: the camera and the entities move in coarse f32 steps. An f32 has a
spacing of 0.0625 blocks between 2^19 and 2^20 (524 288 to 1 048 576) and
0.125 blocks up to 2^21.

| # | Finding | Where |
|---|---|---|
| A1 | The interpolated eye is lerped in f64, then cast to `Vec3` **before** the renderer subtracts its origin. At x ≈ 1M the camera moves in 6–12 cm steps, so the origin trick in `write_camera` does not help. The second lerp in `View::sample` and `retarget` work on the rounded values. | `umicraft-game/src/lib.rs:385`, `entity/store.rs:319` (spectator), `umicraft-render/src/camera.rs:70` (`from_minecraft`), `umicraft-client/src/world_view.rs:230`, `:814`, `:878` |
| A2 | Entity snapshots hold `prev_position`/`position` as `Vec3` and lerp in f32. Name tags compute anchors and distances in absolute f32. Weather, clouds and border widen the rounded f32 eye back to f64 (`camera.position.as_dvec3()`). | `umicraft-game/src/entity/snapshot.rs:38-40, 133`, `umicraft-client/src/nametag_draw.rs:75-101`, `umicraft-render/src/{weather/mod.rs:130, clouds/mod.rs:213, border/mod.rs:148}` |
| A3 | Terrain vertices are absolute world coordinates in f32. Block corners stay exact below 2^24, but 1/16-aligned model detail (torches, slabs, panes, liquid heights) stops being exact above 2^19, so small geometry starts to snap around 524k. | `umicraft-mesh/src/{legacy.rs:209, models.rs, fluid.rs, moving.rs}` |
| A4 | Fog distance is `distance(world_position, camera_pos)` on two absolute f32 values, so the fog edge shimmers slightly. | `terrain.wgsl:129`, `weather.wgsl:71`, `particles.wgsl` |
| A5 | The culling frustum is built from the absolute `view_projection()`, not `view_projection_from(origin)`. Planes are imprecise far out. This is correctness only, not a cost. | `renderer.rs:1281`, `camera.rs:178` |

### B. Terrain on the GPU

| # | Finding | Where |
|---|---|---|
| B1 | One "opaque" pipeline with an alpha-test `discard` draws **every** solid block. A shader that may discard turns off early depth rejection on many GPUs, so hidden fragments are shaded anyway. Vanilla 1.8 splits SOLID / CUTOUT_MIPPED / CUTOUT. | `terrain.wgsl` `fs_main`, `renderer.rs` `RenderState::opaque` |
| B2 | Opaque chunks are drawn in `HashMap` order, not near to far, which adds overdraw on top of B1. | `renderer.rs:2277` |
| B3 | No occlusion culling. Caves and the whole underground are drawn. Vanilla 1.8 has per-section visibility graphs (`VisGraph`/`SetVisibility`, flood fill from the camera section). | — |
| B4 | The unit of meshing, culling and drawing is a whole 16×256×16 column: one draw, and one frustum box 256 high. | `renderer.rs:2783`, `GpuMesh` |
| B5 | Vertex size is 44 B: `position`, `normal` and `uv` as f32, plus `layer`, `light` and `tint` as u32. About 16 B is enough: section-relative u16 position, 3-bit face, u16 uv, u16 layer, packed light, tint or biome index. That would cut vertex fetch and upload by about 2.5×, and fixes A3. | `umicraft-mesh/src/vertex.rs` |
| B6 | Each mesh has its own u32 index buffer, 6 indices per quad. One shared static quad index buffer would do. | `renderer.rs:2740`, `umicraft-mesh/src/mesh.rs` `push_quad` |
| B7 | The lightmap does four dynamically indexed reads per fragment from a 256 × `vec4` uniform array, to filter bilinearly by hand. A 16×16 texture with a linear sampler does it in one fetch. | `lightmap.wgsl` |

### C. Meshing and upload pipeline

| # | Finding | Where |
|---|---|---|
| C1 | Every job meshes the whole column: 65 536 `block()` calls, even for empty sections. One edited block re-meshes up to 5 columns. | `umicraft-mesh/src/models.rs:106` |
| C2 | No priority: dirty columns go out in `BTreeSet` (x, z) order and `WorldView::waiting` is FIFO. An edit next to the player can wait behind hundreds of far columns, and the initial load is not nearest first. | `umicraft-client/src/game.rs:572, 3306`, `world_view.rs:83, 666-720` |
| C3 | `queue` and `dispatch` do a linear `find`/`remove` on `waiting`, which is quadratic during a bulk load. | `world_view.rs` |
| C4 | `collect` drains up to 512 results in one frame. Each result makes up to 4 `create_buffer_init` calls (no pool) and clones `block_entities`. A burst of chunk loads costs a frame spike. | `world_view.rs:673-697`, `renderer.rs:1752` |
| C5 | Mesh workers are capped at 4 whatever the core count. | `umicraft-client/src/mesher.rs:185` |
| C6 | The four `Mesh` vectors start empty and regrow on every job, with no reuse. `fluid_layers` hashes `&str` atlas keys for every liquid cell. | `umicraft-mesh/src/{mesh.rs, models.rs}` |
| C7 | `Arc::make_mut` deep-clones a whole column on the game thread when a mesh job still holds the snapshot. | `umicraft-world/src/world.rs:129, 150, 241, 305` |
| C8 | `ChunkPos` maps use the default SipHash. The cost is constant, not tied to coordinates. | `world.rs:21`, `renderer.rs:331`, `world_view.rs:85-90`, `world/relight.rs:135` |

### D. Per-frame CPU work (outside terrain)

| # | Finding | Where |
|---|---|---|
| D1 | Six `upload_*` entity paths and `set_hand_batches` call `create_buffer_init` once per texture batch, **every frame**. `items/dynamic_mesh.rs` already has the persistent `write_buffer` pattern. | `renderer.rs:1869-1990` |
| D2 | Name tags rebuild two buffers and their vectors every frame, with 1–2 draws per tag. | `umicraft-render/src/nametag.rs:123-190`, `nametag_draw.rs:183, 227` |
| D3 | The HUD and UI draw lists are rebuilt and re-tessellated every frame, with no cache when nothing changed. | `window/mod.rs:1169`, `ui_pass.rs:336`, `renderer.rs:2145` |
| D4 | Block entities (signs, chests, banners, skulls) are re-meshed every frame. `particle_sprites()` allocates a fresh `Vec`. | `window/mod.rs:1186, 1237`, `world_view.rs:276` |
| D5 | Weather quads are rebuilt every frame. The sky makes one `write_buffer` per layer. | `weather/mod.rs:122`, `sky/mod.rs:277` |
| D6 | `desired_maximum_frame_latency` keeps the wgpu default of 2. `ControlFlow::Poll` with no vsync and no cap uses 100 % of a core, which is expected. | `renderer.rs:385, 2461` |
| D7 | No `[profile.release]` tuning: no LTO, 16 codegen units. | `Cargo.toml` |
| D8 | No instruments: no CPU spans, no GPU timestamps, no reproducible scene. | — |

## Plan

Each step is one small commit (or a few) with a unit or regression test.
Optimisation steps also get a criterion benchmark or a scene measurement in
`docs/perf-log.md`.

### Phase 0 — Measure (prerequisite)

| Step | Content | Done when |
|---|---|---|
| F2-0a (**done**, ADR-0059) | CPU spans behind a `profiling` cargo feature (puffin or tracy, ADR choice) on `redraw`, `WorldView::sync`, `prepare_visible_chunks`, uploads, mesh jobs, the game tick. | Spans visible in the viewer. Zero cost with the feature off. |
| F2-0b (**done**, ADR-0059) | GPU timestamp queries per pass group (sky, opaque, entities, translucent, UI), when the adapter has `TIMESTAMP_QUERY`. Shown in F3. | F3 shows the GPU ms per group, or "n/a". |
| F2-0c (**done**) | A reproducible benchmark scene: a fixed capture or world replay, a scripted camera path, and a frame-time report (mean, p95, p99). Run at the origin and at x = 1 000 000. | One command prints the report. The baseline is in `perf-log.md`. |
| F2-0d (**done**: not kept) | Measure `lto = "thin"` and `codegen-units = 1` in `[profile.release]`. | Kept only if the scene shows a gain. |

### Phase 1 — Precision far from the origin (fixes A1–A5)

| Step | Content |
|---|---|
| F2-1a (**done**) | Carry the eye as `DVec3` from `GameState::interpolated` through `RenderState`, `View::sample` and `retarget`. The render origin is `floor(eye)` in f64. `Camera` gets the small `eye - origin` and the integer origin. |
| F2-1b (**done**, ADR-0060) | Entity snapshots, particles, name tags and held items become origin-relative in f64 before the f32 cast. Weather, clouds and border read the f64 eye. |
| F2-1c (**done**) | Fog distance in origin-relative coordinates. The frustum comes from `view_projection_from(origin)`. |
| F2-1d (**done**) | Regression tests: an eye at x = 1 000 000.5 that moves 0.01 yields two different view matrices, and an entity lerp stays smooth at 1M. |

### Phase 2 — Quick GPU wins on terrain (B1, B2, B6, B7)

| Step | Content |
|---|---|
| F2-2a (**done**, ADR-0061) | The renderer tags each quad SOLID or CUTOUT from the texture's alpha. Two pipelines: SOLID without `discard`, drawn first; then CUTOUT. |
| F2-2b (**done**) | Sort the visible opaque list near to far, reusing its buffer as the translucent sort does. |
| F2-2c (**deferred**: at most 0.2 ms, see perf-log) | Lightmap as a 16×16 `Rgba8Unorm` texture with a linear sampler, written per frame (parity check against `lightmap.rs`). |
| F2-2d (**done**) | One shared quad index buffer, grown on demand. Meshes upload vertices only. |

### Phase 3 — Sections (B3, B4, B5, C1); needs its own ADR

| Step | Content |
|---|---|
| F2-3a (**done**, ADR-0062: spans per section in the column job; section jobs not done) | Mesh per 16³ section. Skip absent and all-air sections. An edit re-meshes the touched section, plus a neighbour section only when the block is on its border. |
| F2-3b (**done**) | Per-section frustum culling with a 16³ box. |
| F2-3c | Packed vertex of about 16 B, relative to the section origin, which goes to the shader per draw. Removes absolute f32 positions for good. |
| F2-3d (**done**) | Cave culling: a per-section visibility graph of which faces connect through non-opaque cells, then a flood fill from the camera section each time the camera changes section (behaviour of vanilla 1.8 `VisGraph`; the implementation is original). |

### Phase 4 — Mesh queue and uploads (C2–C8)

| Step | Content |
|---|---|
| F2-4a | Priority dispatch: nearest first, and sections in the frustum before the others. Replace the linear `waiting` scans with a map and a heap. |
| F2-4b | Per-frame upload budget (bytes or ms). The rest waits for the next frame. |
| F2-4c | A GPU buffer arena (sub-allocated vertex pages) instead of a buffer per mesh. Later, `multi_draw_indexed_indirect` when the adapter has the feature, with a fallback. |
| F2-4d | Worker count `available_parallelism - 1`, capped by a setting instead of 4. Mesh vectors recycled through the result channel. Fluid layers resolved once per job. |
| F2-4e | `FxHashMap`/`FxHashSet` for `ChunkPos` keys (licence check in `deny.toml`). |
| F2-4f | Avoid the `Arc::make_mut` column clone. Snapshot only what the mesher needs, or copy-on-write per section. |

### Phase 5 — Per-frame CPU (D1–D6)

| Step | Content |
|---|---|
| F2-5a | One persistent growable vertex arena for entities, hand, shadows, glint and ghosts, with `(offset, count)` per batch. |
| F2-5b | Persistent name-tag buffers. One draw for all tags where depth allows it. |
| F2-5c | Reuse the vectors behind `particle_sprites` and the block-entity meshes. Re-mesh block entities only when their state changes. |
| F2-5d | Cache the UI tessellation while the draw list is unchanged. |
| F2-5e | `desired_maximum_frame_latency = 1` (measure the input latency). One write for the sky uniforms. |

## Order

0 → 1 → 2 → 3 → 4 → 5.

- Phase 1 fixes the far-from-origin complaint at a moderate cost.
- Phase 2 is probably the best fps gain per line of code.
- Phase 3 is the structural change that should give the clear lead over
  vanilla in dense and underground scenes.
- Phases 4 and 5 remove frame spikes and CPU overhead.

## Not in scope

- Changing what the player sees. Every step must keep parity: screenshot tests
  and `docs/parity.md` rows stay green.
- Greedy meshing: it breaks per-vertex smooth lighting and AO parity unless it
  is restricted to uniformly lit faces. It is kept as a later option.
