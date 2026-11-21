# F6: pack features and OptiFine-style video settings: plan

Status: **phases 0 and 1 done**; next is phase 4 (custom sky). Branch `feat/optifine-features`.
Phases may get their own `feat/<phase>` branches off it. Design: ADR-0088
(reference policy) and ADR-0089 (architecture). Roadmap entry: F6.

## Starting point (main @ 6afb976)

- **Options.**
  - `VideoOptions` (`crates/umicraft-client/src/options/video.rs`) is flat, with
    render distance 2..=16.
  - The video page (`screens/options/video_page.rs`) is the vanilla 1.8 list
    with no sub-pages and no tooltips.
  - Options that affect meshing go through the atomics in `meshing/settings.rs`
    and `GameEvent::ReMeshAll` (ADR-0048).
- **Packs.**
  - `ClientResources::apply_packs` keeps only block, item and GUI textures,
    colormaps, models and blockstates.
  - `window/startup.rs::upload_sky_textures` reads the jar only.
  - Entity textures (`scene/entity/specs.rs`) are jar paths.
- **Render.**
  - The sky lives in `render/sky/` (dome, celestial, stars, End) and the clouds
    in `render/clouds/`.
  - Culling is per-section frustum plus a cave graph (ADR-0062).
  - `MeshQueue` uses nearest-first ordering with a 1 MiB upload budget
    (ADR-0063).
  - Texture animations come from `TextureAnimator` (ADR-0023).
- **Models.** Entity models are hard-coded Rust in `umicraft-model/src/entity/`
  (`ModelPart`, `ModelBox`, `emit_triangles`), posed in code (ADR-0069).
- **The OptiFine reference tree is partial** (ADR-0088): it has no CEM loader,
  part names or entity variables, no Smart Animations and no Render Regions.

## Plan

### Phase 0: foundations

| Step | Content | Done when |
|---|---|---|
| 0.1 | ADR-0088, ADR-0089, this plan, ROADMAP F6 (**done**) | |
| 0.2 | `PackFiles` view over the resource stack for `optifine/**`, `mcpatcher/**`, `textures/environment/**` and `textures/entity/**`, trying `optifine/` before `mcpatcher/` (**done**) | Unit tests on the stack order and the folder precedence |
| 0.3 | Sky, cloud and weather textures read through the packs (bug: jar only) (**done**) | Regression test: a pack `sun.png` wins over the jar |
| 0.4 | Entity, armour, banner, horse and map textures read through the packs (they were jar only) (**done**) | Regression test with a pack armour texture |
| 0.5 | `umicraft-packext` crate: `properties` (typed errors), `ranges` (`0-3 5`), `filters` (biomes, heights, weather) (**done**) | Unit tests and a fuzz target for `properties` |

### Phase 1: video settings menu

| Step | Content | Done when |
|---|---|---|
| 1.1 | Render distance 2..=32 (fog range, mesh queue and cloud cutoff follow) (**done**; the `VideoOptions` subsections and `GraphicsChoice` move to phase 2, with the first options that need them) | Round trip of `options.toml`; an old file still loads |
| 1.2 | `OptionSpec` table, with `Effect` derived from the changed options, replacing `VideoEffects` (**done**) | Unit tests: each effect is triggered by its options only |
| 1.3 | Root page in the OptiFine order. Sub-page buttons: Quality, Details, Performance, Animations, Other. Shaders is not shown (**done**) | Screen tests |
| 1.4 | Tooltips after 700 ms of a still mouse (a move of more than 5 px resets the timer); lines from `<key>.tooltip.N`; a line ending in `!` is red (**done**) | Unit test of the hover timer |
| 1.5 | Animations "All ON" and "All OFF" buttons; Other "Reset video settings" with confirmation (**done**) | Screen tests |
| 1.6 | Existing knobs placed on their pages: max fps, vsync, mipmaps, clouds, mesh threads, cave culling ("Advanced occlusion"), entity shadows (**done**) | Language keys exist in `en_US` and `fr_FR` (existing test) |

### Phase 4: custom sky (after phase 1, by owner order)

| Step | Content | Done when |
|---|---|---|
| 4.1 | `packext::sky` parsing. Files: `sky/world<N>/sky<K>.properties`; the scan stops at the first missing K. Keys: `HH:MM` fades; `startFadeOut` derived when absent; the 24000-tick check; `blend`, `rotate`, `speed`, `axis`, `days`, `daysLoop`, `weather`, `biomes`, `heights`, `transition` | Table tests on fades, day loops and invalid layers |
| 4.2 | Per-frame `SkyFrame` evaluation, with no allocation: brightness = position × weather × fade; transition smoothing | Bench and unit tests |
| 4.3 | Render: a cube with half-size 100 and a 3×2 texture layout; 9 blend modes as pre-built wgpu pipelines; rotation about the permuted axis | Offscreen screenshot with an original test pack |
| 4.4 | Option "Custom Sky" (default on, `Effect::ReloadSky`) | |

### Phase 5: custom entity models (CEM)

| Step | Content | Done when |
|---|---|---|
| 5.1 | `.jem` / `.jpm` parsing: `models`, `baseId`, `model`, `part`, `attach`, `translate`, `rotate`, `scale`, `invertAxis`, `mirrorTexture`, `boxes` (`coordinates`, `textureOffset`, `sizeAdd`, per-face UVs), `sprites`, `submodel(s)`; the path rules `./` and `~/` | Unit tests; fuzz target |
| 5.2 | Per-mob part-name table mapping CEM names to the `ModelPart` fields of the hard-coded models (source: public CEM documentation) | One test per covered mob |
| 5.3 | Expression engine: lexer → parser → compilation to bytecode with slot-indexed variables, evaluated in `f32` with no allocation. Covers the precedence table, the functions, `if`, `between`, `in`, `smooth` and `random` | Table tests; criterion bench of the evaluation |
| 5.4 | Animation variables (`limb_swing`, `head_yaw`, ...), mapped from `EntityRenderSnapshot` (source: public documentation) | Unit tests |
| 5.5 | Hook in `scene/entity/mobs.rs`, falling back to the hard-coded model. Option "Custom Entity Models" (default on) | Offscreen screenshot of an original CEM creeper |

### Phase 2: details and animations

| Step | Content | Done when |
|---|---|---|
| 2.1 | Sky, Stars, Sun & Moon; Fog Fast/Fancy/Off; Fog Start; Cloud Height | Unit tests on `RenderEnvironment` |
| 2.2 | Trees Default/Fast/Smart/Fancy (Smart: baked leaves without inner faces); Rain & Snow; Dropped Items; Vignette; Translucent Blocks | Mesh tests on leaves; screenshots |
| 2.3 | Clear Water, Better Grass/Snow, Swamp Colors, Smooth Biomes | Mesh tests |
| 2.4 | One toggle per animated sprite category (water, lava, fire, portal, redstone, terrain, textures) | Unit tests on `TextureAnimator` |
| 2.5 | Per-type particle filters, rejected before spawn | Unit tests; no allocation on the rejection path |

### Phase 3: performance (every item: a criterion bench and `docs/perf-log.md`)

| Step | Content | Done when |
|---|---|---|
| 3.1 | Priority lane for near and player-edited sections; synchronous fast path for empty sections | Bench of the edit-to-visible latency |
| 3.2 | Skip the visibility search when the camera and the sections are unchanged | Bench; screenshots unchanged |
| 3.3 | Side lists for sections with entities or block entities | Bench |
| 3.4 | Cached cloud mesh (rebuild every 20 ticks, on a colour change > 0.003, or when crossing cloud height; scroll by uniform) | Bench |
| 3.5 | Smart animations: upload only the sprites drawn last frame | Bench |
| 3.6 | "Chunk Updates" and "Dynamic Updates" mapped onto the mesh queue budget | Bench at every setting |
| 3.7 | Lagometer (512-entry ring buffers of section timers) and the compact "Show FPS" line | Unit tests of the ring buffer |

### Phase 6: connected textures (CTM)

| Step | Content | Done when |
|---|---|---|
| 6.1 | `packext::ctm` parsing. Keys: `matchBlocks`, `matchTiles`, `metadata`, `method`, `tiles` (ranges and paths), `connect`, `faces`, `biomes`, `heights`, `width` / `height`, `weights`, `symmetry`, `linked` | Unit tests |
| 6.2 | Methods `ctm` (the 47-tile table), `horizontal`, `vertical`, `h+v`, `v+h`, `top`, `random`, `repeat`, `fixed` | One table test per method |
| 6.3 | Tiles added to the block atlas or `hires_blocks`; tile chosen per quad in the mesher through `BlockSource` | Mesh bench before and after; screenshot |
| 6.4 | Option Off/Fast/Fancy (default Fast) | |

### Phase 7: remaining pack features (one sub-phase each)

Random entities (UUID-based, then rules and weights), Custom Colors
(`color.properties`, colormaps, lightmaps), Natural Textures, Custom Items
(CIT), Custom Fonts, Custom GUIs, custom texture animations, and Dynamic Lights.

## Order

P0 → P1 → P4 → P5 → P2 → P3 → P6 → P7.

## Defaults

OptiFine defaults, except where a faster choice costs nothing or almost nothing
on screen. The owner delegated this choice on 2026-11-21. Options not listed
here take the OptiFine default.

| Option | OptiFine | Umicraft | Why |
|---|---|---|---|
| Chunk Updates | 1 | follows the current mesh budget | One section per frame would be slower than today's multithreaded queue |
| Dynamic Updates | Off | On | More sections are meshed while the player stands still; no visual cost |
| Trees | Default (Fancy) | Smart | Same look as Fancy without the hidden inner faces of leaves |
| Smart Animations | n/a in the 1.8.9 build | On | Only sprites drawn last frame are uploaded; no visual change |
| Advanced occlusion (cave culling) | n/a | On | Already the Umicraft behaviour (ADR-0062) |
| Connected Textures | Fast | Fast | Kept: Fancy adds 4 diagonal neighbour tests per face |
| Better Grass, Natural Textures | Off | Off | Kept: extra work in the mesher |
| Fog | Fast | Fast | Kept |

Optimisations with no visible effect (3.1 to 3.4) are always on and have no
option.

## Not in scope

- Integrated-server options: Smooth World, Lazy Chunk Loading, Autosave, Time
  and Weather.
- GL-only options: Fast Math, Smooth FPS, VBO, Fast Render, and Antialiasing
  with a restart.
- Anaglyph (ADR-0043).
- Shaders (spec RND-13, and the post-process of the F3 modding API).
- OptiFine assets: the default CTM tiles are not shipped.

## Acceptance

- Each phase: `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings`
  and `cargo test --workspace` pass, and the parity screenshot tests stay green.
- Every pack format has unit tests and a fuzz target, and a malformed file never
  crashes the client.
- Every optimisation has before/after numbers in `docs/perf-log.md`.
