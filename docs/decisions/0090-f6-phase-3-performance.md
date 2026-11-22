# 0090. F6 phase 3: OptiFine performance knobs

Status: accepted (2026-11-21)

## Context

Phase 3 of `docs/tasks/optifine/README.md` lists the OptiFine optimisations
that could apply to a wgpu renderer. Umicraft's F2 pass (ADR-0058 to 0064)
already did part of that work. The rule of the project is that an
optimisation without a measured gain is not kept.

## Decision

- **Already covered, nothing added:**
  - 3.1, the near and player-edited lane: the mesh queue picks the nearest
    chunk at every pick (ADR-0063).
  - 3.2, skipping the visibility search: `prepare_visible_chunks` returns
    early when its key is unchanged.
  - 3.3, side lists: entities and block entities are not drawn per section.
  - 3.4, the cloud cache: the cloud meshes are built once and scrolled by a
    uniform.
- **Chunk Updates:** the per-frame upload budget is `chunk_updates × 256 KiB`.
  The default 4 keeps the 1 MiB budget of ADR-0063.
- **Dynamic Updates:** the budget doubles once the eye position, yaw and pitch
  have been still for 500 ms (`world_view/stillness.rs`). The 500 ms window
  and the ×2 boost are our choice.
- **Smart Animations:**
  - Each uploaded chunk mesh records its block layers in a bitset, and the
    visible draws join them.
  - An animated layer out of view keeps its phase but skips its texture
    write.
  - It is off while a screen is open, because screens show block icons.
  - It is kept as an owner-requested option. Its cost is measured (18.8 µs at
    most per frame of uploads); its gain, a few skipped 1 KiB texture writes
    per tick, is not measurable with the current scene benches, which have no
    animations.
- **Lagometer and Show FPS:**
  - 512-sample allocation-free rings of frame, sync and upload times.
  - The graph is drawn bottom-left under F3.
  - The compact `fps, C:, E:, U:` line is shown without F3.

## Consequences

- A block drawn outside the terrain (a held block, a dropped item) can freeze
  its animation while no chunk with that texture is in view. OptiFine's
  Smart Animations has the same trade-off.
- The screen overlays (in-wall, fire, portal) count as in view: the flames of a
  burning player use `fire_layer_1`, which no terrain fire draws
  (`fire_floor_main` only uses `fire_layer_0`), so without this they froze
  (fixed 2026-11-21, `ScreenOverlays::draws_layer`).
- Numbers are in `docs/perf-log.md`.
