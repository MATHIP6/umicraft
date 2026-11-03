# ADR-0038 — Lighting parity, local relight and chunk culling

Status: accepted (2026-11-03)

## Context

Side-by-side with the 1.8.9 client, Umicraft showed three differences:

1. Ambient-occlusion creases were much darker, and light near solid blocks was
   pulled toward black.
2. Shadows stayed after a block was broken (and placed blocks left lit cells).
3. Every uploaded chunk was drawn each frame, even beyond the render distance
   and behind the camera.

## Decisions

- **AO factor.** Vanilla averages four cell values (solid 0.2, open 1.0), which
  gives 1.0 / 0.8 / 0.6 / 0.4 for 0–3 occluders (`BlockModelRenderer`,
  `renderModelAmbientOcclusion`). The shader mapped the baked nibble to
  `0.2 + 0.8·n/15` (floor 0.2); it is now `0.4 + 0.6·n/15`.
- **Corner light.** `getAoBrightness` replaces a corner sample with no light
  by the outward cell's light, and a hidden diagonal reuses an edge sample.
  `quad_light` does the same (`or_base`).
- **Local relight.** The 1.8 server sends no light update on a block change;
  the client recomputes it (`World.checkLightFor`). `World::relight_around`
  runs a removal then re-spread flood fill per channel; it is called from
  `apply_block_change(s)`, and the touched columns are returned as dirty.
  Emitters come from a small id table (`relight::emission`); water and ice
  absorb 3 levels. Sections the server elided read as full sky, so they are
  materialised with sky 15 before a write (`Section::open_sky`).
- **Culling.** The renderer draws only columns within `view_distance` chunks of
  the eye (square, like `renderDistanceChunks`) and intersecting the view
  frustum (`Frustum`, Gribb–Hartmann).

## Consequences

- Emission of blocks not in the table is ignored for relight only; the data
  the server sent for them is still displayed until a neighbour changes.
- A lit section emptied of blocks is kept (it carries sky light).
