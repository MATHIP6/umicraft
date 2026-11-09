# Perf 2 — Analysis of the client and plan

Status: **analysis done; phases A, B (except B3, B5), C1, D1, E1, E3 and E4 implemented** (see "Status of the steps" below). Follows F2 (`docs/tasks/f2/README.md`,
ADR-0058), whose phases 0 to 5 are done. Numbers are in `docs/perf-log.md`
(2026-11-09 rows). Written 2026-11-09.

## Why a second pass

F2 attacked the terrain on a GPU-bound machine (an Intel iGPU, 4 ms per frame, 2.5 ms of
GPU). The owner's machine has a strong dedicated GPU and plays at 700 to 1 100 fps. There
the scene is **CPU-bound**, and what costs most is not the terrain.

## What the measurements say

Owner's machine, release build, `--bench-scene` (terrain only, no HUD, no entities):

- Frame 0.52 to 0.55 ms (about 1 900 fps), GPU only 0.16 ms.
- The render path itself is about 0.18 ms: uniforms 25 us, culling 30 us, encoding all passes
  20 us, `queue.submit` 100 us. The rest is waiting for the GPU.
- 130 to 177 chunk draws per frame. Encoding a draw costs about 0.13 us, so **fewer draw
  calls is not the first lever** (F2-4c stays deferred).

The real game takes 1.37 ms at 730 fps, so about 1.2 ms are outside the terrain path.
Micro-measurements of what runs every frame in the window thread:

| What | Cost per frame | Share of 1.37 ms |
|---|---|---|
| HUD: build the draw list and tessellate, chat of 100 lines + sidebar | 270 + 50 us | 23 % |
| HUD with the tab list of 80 shown | 340 + 130 us | 34 % |
| of which the chat alone: 100 lines against 10 lines | 229 us against 69 us | 12 % |
| Entities: 50 players / 50 zombies / 100 cows (re-meshed every frame) | 147 / 357 / 615 us | 11 / 26 / 45 % |
| `HudState::clone` (chat, sidebar, tab list) | 17 to 30 us | 1 to 2 % |
| `world.entities()` computed three times (tags, tab list, render) | not measured | — |
| Uniform writes (sky, clouds, weather, particles, border...) | 25 us | 2 % |
| `prepare_visible_chunks` (walks every loaded column) | 30 us | 2 % |

Not CPU, for the record: DX12 against Vulkan makes no difference here; startup of the
offscreen demo is 0.7 s; flate2 already uses `zlib-rs`; the F2 terrain work holds.

## Findings (with the code that causes them)

1. **The HUD is rebuilt from scratch every frame.** `draw_hud_gui` then `UiPass::rebuild`
   run at 700+ Hz on text that changes at 20 Hz. ADR-0064 measured only the tessellation of
   a small list (37 us) and left the cache undone; the *build* is 5 to 7 times bigger.
2. **The chat processes its whole history every frame.** `chat_rows`
   (`umicraft-render/src/hud/gui/chat.rs:49`) wraps and measures all 100 retained lines, then
   `visible_window` picks the few that show. 160 us wasted, no cache needed to fix it.
3. **Every entity is meshed on the CPU every frame** (`entity_render::mesh_entities_with`):
   five `HashMap`s created per call, one `Vec` per layer, a `transform_vertex` pass per
   vertex, for entities that mostly stand still between two ticks. There is no entity
   frustum or distance culling in that path (none found by reading).
4. `window/mod.rs` clones the whole `HudState` (chat, tab list, sidebar) each frame, calls
   `world.entities()` up to three times (three `Vec`s and three interpolations), and clones
   `window_view()` each frame while a container is open. The inventory portrait re-renders
   and allocates two GPU buffers every frame while the inventory is open.
5. The `release` profile is the default one. Fat LTO with `codegen-units = 1` gives
   **-6 to -8 %** on the CPU-bound scene (4 interleaved pairs, all in favour). The F2-0d
   verdict "no gain" came from a GPU-bound machine. Cold build 4 min.
6. **"Fast graphics" does nothing for performance.** `fancy_graphics` only draws the
   vignette and shows in F3. Vanilla fast graphics makes leaves opaque with internal faces
   culled, which is a large cut in forests, and it is the main lever of a weak machine.
7. The window keeps redrawing at full speed while it is **unfocused or minimized**
   (`Focused(false)` only releases the mouse), and in menus.
8. 15 mesh workers on 16 threads at normal priority: good for load time (F2-4a) but they
   compete with the render thread in bursts. The game runs 40 threads.
9. Memory is not observable on Windows: `process_memory.rs` reads `/proc` only, so the F3
   line shows 0. The running process held 338 MB.
10. The tools are too coarse: only seven profiling scopes, none inside `redraw`.

## Status of the steps (2026-11-09)

Done: **A1** (profiling scopes in `redraw`, the HUD, the entity meshing, the render
window), **A2** (a `CPU` line in F3: world, hud, scene, render, smoothed), **A3** for the
HUD (`umicraft-render/tests/hud_cpu_costs.rs`; the entity meshing bench is not kept),
**A4** (Windows memory in F3 through `memory-stats`; the machine total is not read, so
F3 shows `Mem: 338MB`), **B1** (chat: 229 to 69 us), **B2** for the HUD *build*
(`hud_cache.rs`: kept while the state is equal, its clock held for one tick; the mod
widgets are drawn after it every frame), **B4** (one `entities()` per frame), **C1**
(`entity_render/cull.rs`: far, or well behind the camera, not meshed; the list stays
whole), **D1** (the visible lists are kept for a still camera over unchanged chunks),
**E1** (a `dist` profile, not the default), **E3** (at most 8 automatic mesh workers),
**E4** (30 fps without the focus).

Not done: **B2 tessellation** (the draw list is still turned into triangles every
frame, 35 to 130 us), **B3**, **B5**, **C2**, **C3**, **C4**, **D2**, **D3**, **E2**,
**E5**, all of **F** (fast graphics, presets, render scale, half-resolution blur) and
**G**. The changed paths are in the HUD, the entities and a still camera, so the
bench-scene does not move; read the effect in game with the F3 `CPU` line.

## Plan

Rule kept from ADR-0058: no optimisation lands without a before/after row in
`docs/perf-log.md`, and the pixels do not change (screenshot tests and `docs/parity.md`).
The "expected" figures are estimates from the table above, to be replaced by measured ones.

### Phase A — see the real game (prerequisite, no gain)

| Step | Content | Done when |
|---|---|---|
| A1 | Profiling scopes inside `redraw`: world sync, HUD clone, HUD build, tessellation, entity meshing, render encode, submit, present wait. | The puffin viewer shows each. |
| A2 | An F3 "CPU" block: the same split in microseconds, smoothed, so the owner can read it in game with no tool. | F3 prints it. |
| A3 | Keep the three throwaway measurements as permanent `criterion` rows: HUD build, entity meshing, `HudState::clone`. | They run with the other benches. |
| A4 | Process memory on Windows for F3 (a crate such as `sysinfo`; the workspace denies `unsafe`). | F3 shows MB used. |

### Phase B — HUD (the largest measured gain)

| Step | Content | Expected |
|---|---|---|
| B1 | The chat wraps only the lines that can show (newest first, stop when the window is full; the log keeps its history). | -160 us with 100 lines |
| B2 | Cache the HUD draw list and its triangles while its inputs are unchanged. Inputs change at tick rate (chat fade is counted in ticks in the reference), so the cache key is a version bumped by the game thread, the viewport, the GUI scale and the dynamic parts (F3, mod widgets, crosshair). Split static layers (chat, sidebar, tab list, titles) from per-frame ones. | -300 us (-500 with the tab list) |
| B3 | Stop cloning `HudState`: share the chat, tab list and sidebar by `Arc`, or draw from a reference. | -17 to -30 us |
| B4 | `world.entities()` once per frame, shared by the tags, the tab list hats and the render. | a `Vec` and an interpolation pass x2 |
| B5 | Inventory portrait only when the pointer or the model changed; reuse its buffers. | one frame in many |

### Phase C — entities (scales with the crowd)

| Step | Content | Expected |
|---|---|---|
| C1 | Frustum and distance culling before meshing (the reference has `shouldRender` and a render distance weight). | proportional to what is behind |
| C2 | Cache the meshed vertices of an entity while its interpolated snapshot is unchanged (idle mobs, players standing, armor stands). | up to -90 % in a crowd that stands |
| C3 | Reuse the maps and vectors between frames (no per-frame `HashMap` or `Vec` creation). | fewer allocations |
| C4 | Longer term, own ADR: pose on the GPU (static part meshes, one matrix per part in a storage buffer), so the CPU cost per entity is a few matrices. | about 0.2 us per entity |

### Phase D — render path (small, after A to C)

| Step | Content | Expected |
|---|---|---|
| D1 | Skip `prepare_visible_chunks` when the camera, the loaded set and the options are unchanged; walk a ring around the eye, not the whole map. | -30 us on still frames |
| D2 | Write the sky, clouds, weather and border uniforms only when they changed; one write for the sky. | -15 us |
| D3 | Re-measure `queue.submit` (100 us) with fewer, larger passes and render bundles for static draws. Re-open F2-4c (arena and indirect multi-draw) only if A1 shows draws dominate in real worlds. | to measure |

### Phase E — build and process

| Step | Content | Expected |
|---|---|---|
| E1 | A `dist` profile (`lto = "fat"`, `codegen-units = 1`) used for releases; day-to-day builds keep the default. | -6 to -8 % |
| E2 | `mimalloc` as global allocator, measured on the HUD and entity paths (allocation heavy). Licence check in `deny.toml`. | to measure |
| E3 | Mesh workers: default `min(cores / 2, 8)` and below-normal priority; pause far meshing while a frame is over budget. | fewer hitches in bursts |
| E4 | Background frame cap: when the window is unfocused or minimized, 20 to 30 fps; in menus 60. | power, heat, other apps |
| E5 | Optional `--backend` flag for troubleshooting (no gain measured). | — |

### Phase F — settings and weak machines

| Step | Content |
|---|---|
| F1 | Real **fast graphics**: opaque leaves with internal faces culled (and fast clouds and other vanilla fast paths). Parity-checked against the reference (`src`). |
| F2 | **Presets** Low, Medium, High, Ultra, and a first-run choice from the adapter (integrated or discrete): view distance 6 / 8 / 12, smooth lighting off, clouds off or fast, particles minimal, entity shadows off, mipmaps 2, fast graphics. |
| F3 | A **render scale** option (world at 50 to 100 %, HUD at native) for integrated GPUs. |
| F4 | Motion blur: a quality option at half resolution (the blur costs 0.3 to 0.5 ms at 1440p on a strong GPU, more on a weak one). |
| F5 | Document the settings that cost most in the options screen help text: render distance, smooth lighting, fancy graphics, clouds, particles, mipmaps. |

### Phase G — startup and memory (measure, then decide)

| Step | Content |
|---|---|
| G1 | Time the windowed start with a server (jar read, atlas, mipmaps, first chunks), with spans; parallelise the texture work if it matters. |
| G2 | Memory budget per feature once A4 exists. Re-test a packed 16-byte vertex on a discrete GPU (F2-3c was 70 % slower on an iGPU, so no assumption). |

## Order and expected result

Highest gain per effort first: **B1, B2, C1, C2, E1**, then B3 to B5, D1, D2, E3, E4. On
the owner's scene (HUD with a busy chat, some entities, 1.37 ms) the CPU items B and C
should take about 0.5 ms off, E1 another 6 to 8 %, which points at 1.0 ms or under
(1 000 fps). These are estimates: A1 and A2 give the real split before anything is kept.

For weak machines the order is F1, F2, F3 (GPU and memory bound), then E4.

## Not in scope

- Anything that changes what the player sees. Parity stays green.
- Greedy meshing (breaks smooth lighting parity), still deferred.
- Multi-draw indirect and vertex packing before A1 shows they matter.
