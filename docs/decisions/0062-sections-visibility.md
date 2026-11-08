# 0062. Section spans, per-section culling and cave visibility graph (F2-3)

Status: accepted. The packed vertex (F2-3c) was tried and **not kept** (see the
last section).

## Context

A chunk is meshed, culled and drawn as a whole 16×256×16 column: 65 536
`block()` calls even for empty sections, one 256-high frustum box, and the
whole underground drawn (F2 audit B3, B4, C1). The spec expects a clear lead
over vanilla in dense and underground scenes.

## Decision

1. **The unit of a job stays the column; the unit of culling and draw becomes
   the 16³ section.** A column job meshes section by section, in the same order
   as before (so the vertex order, and every existing mesh test, is unchanged),
   skips absent and all-air sections without a single `block()` call, and
   records for each section the span of vertices it added to each pass
   (`SectionSpan`) and its connectivity.
   Section-granular *jobs* (an edit re-meshing only the touched section and its
   border neighbours) are not done here: it needs section-level dirty tracking in
   the game thread and a buffer per section. It is the follow-up if edit latency
   turns out to matter; the measure is the `mesh/*` benches.
2. **One vertex buffer per column and pass, with per-section sub-ranges.** The
   upload keeps the SOLID quads first and the CUTOUT quads after (ADR-0061), each
   group in section order, and stores `(start, len)` per section and group.
   A frame draws only the sections that survive the tests below; consecutive
   surviving sections whose ranges touch are merged into one draw, so the draw
   count stays near one per chunk and group.
3. **Per-section frustum test** with the 16³ box, relative to the render origin
   (ADR-0060), after the existing per-column test.
4. **Cave culling by a visibility graph**, the behaviour of vanilla 1.8
   `VisGraph`/`SetVisibility` (observed in `RenderGlobal.setupTerrain`; the
   implementation is original):
   - *Connectivity* of a section: which pairs of its six faces are joined by a
     path through non-opaque cells (`BlockState::is_opaque` is false). Computed by
     the mesh worker with a flood fill over the 4 096 cells; stored as six 6-bit
     masks (`Connectivity`). An empty section is fully connected, a full one not
     at all.
   - *Traversal*: from the camera section, visit neighbours through a face when
     the section's connectivity joins the face it was entered by to that face; the
     start section may leave through any face; a step may not reverse a direction
     already taken (as vanilla). Missing or empty chunks are fully open. The
     traversal is bounded by the render distance and by y 0..16, and is not limited
     by the frustum, which is more conservative than vanilla.
   - The result is cached until the camera changes section or a mesh is uploaded
     or removed.
   - `Renderer::set_cave_culling(bool)` turns it off. A test renders the bench
     scene with and without it and requires the same pixels.
5. The block entities, the entities and the particles are not culled by the
   graph (they are few); only terrain sections are.

## Consequences

- `Mesh` gains `sections: Vec<SectionSpan>` (16 entries when produced by the
  mesher, empty for hand-built meshes, which are drawn whole).
- The heuristic can hide a section that is visible through a thin gap, as in
  vanilla; the switch exists for that reason.
- Frame cost: a flood fill of at most 17 × 16 × 17 sections when the camera
  changes section, and 16 box tests per visible chunk per frame.

## F2-3c result: packed vertex not kept

A 16-byte chunk vertex (fixed-point position relative to the column corner,
15-bit uv, 10-bit layer, packed light and tint, a per-chunk offset read per
instance) was implemented and measured: 70 % slower on the owner's Intel iGPU
and not pixel-identical because of the uv grid. The vertex fetch is not the
bound on this hardware. The 44-byte vertex stays. See `docs/perf-log.md`
(2026-11-07, F2-3c) for the numbers and when to retry.
