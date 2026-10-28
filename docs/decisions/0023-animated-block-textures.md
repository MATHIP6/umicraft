# ADR-0023 — Animated block textures

**Status:** accepted
**Date:** 2026-10-28
**Milestone:** P7 (rendering), pulled forward because water and lava looked frozen

## Context

The atlas kept the first square frame of every texture, so water, lava, fire
and the portal showed one still picture. The liquids already sampled the
right layers (ADR-0017 mesher), so only the pixels of those layers were wrong.

A second cause hid behind the first: commit `05f0cc1` had also unplugged the
liquid mesher (`umicraft-mesh::fluid`, ADR-0017 era). Water and lava went back
through the legacy cube path, so they were drawn as full blocks with their
inner faces and a stepped surface, whatever their texture. The wiring of
`fluid::emit_fluid` in `mesh_chunk` and `mesh_chunk_with_models`, its layer
lookup and the tests that pin it (11 faces for a lone source, surface tint) were
restored from `05f0cc1^`.

## Decision

1. **The atlas keeps the animation.** `build_atlas` still stores frame 0 in the
   layer. A block texture with a valid `animation` section in its `.mcmeta` and
   more than one frame also gets a `TextureAnimation` holding all its frames,
   scaled to the atlas edge. A strip without metadata stays on frame 0, like
   the reference. `BlockAtlas::animations()` exposes them.
2. **The timeline copies `TextureAtlasSprite`.** One tick advances a counter;
   the entry changes when the counter reaches its time; `interpolate` blends
   the colour bytes (alpha of the current frame kept) in between. Invalid
   metadata is ignored instead of failing the whole atlas.
3. **One layer upload per change.** `Renderer::write_block_layer` rewrites a
   single layer of the bound array with `queue.write_texture`. It refuses to
   write when the procedural array is bound, so a stale animation cannot
   corrupt it.
4. **The window owns the clock.** `TextureAnimator` turns the frame time into
   20 Hz ticks, independently of the frame rate, and runs at most 4 ticks per
   frame after a stall. A resource reload creates a fresh animator.
5. **Items are excluded.** Only keys under `blocks/` animate. The clock and the
   compass have a `.mcmeta` but the reference drives them by time and heading;
   no item is drawn before P3 (see `docs/backlog.md`).

## Consequences

- The 1.8.8 jar yields nine animated layers: water and lava (still and flow),
  both fire layers, the portal, rough prismarine and the sea lantern.
- Each tick uploads at most one layer per animated texture (32×32×4 bytes),
  and nothing on frames that cross no tick.
- Not verified against vanilla frame by frame; see `docs/parity.md`.
