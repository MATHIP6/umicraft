# 0061. SOLID/CUTOUT terrain pipelines, sorted opaque chunks, shared quad indices (F2-2)

Status: accepted

## Context

One opaque pipeline with an alpha-test `discard` drew every solid block, which
disables early depth rejection on many GPUs; opaque chunks were drawn in
`HashMap` order; every mesh owned a vertex and an index buffer, though all
quads share one index pattern (F2 audit B1, B2, B6).

## Decision

- **Classification at upload, in the renderer.** The renderer keeps one flag per
  layer of the bound block array: a layer is *cutout* when it has a texel with
  alpha < 128 (the shader discards below 0.5). `upload_mesh` copies the opaque
  quads into a scratch vector, solid first, and stores the solid quad count.
  This replaces "the mesher tags each quad" of the plan: the result is the same,
  `Mesh` and the mesher are untouched, and it also covers the procedural array.
  An unknown layer is cutout (the safe side).
- Two pipelines: `fs_solid` (no `discard`) draws the solid range first, then
  `fs_main` draws the cutout range.
- **Stale flags.** Changing the block array bumps an epoch; a chunk uploaded
  with an older epoch is drawn whole by the alpha-test pipeline until it is
  uploaded again (always correct, only slower).
- **Near to far.** The visible opaque chunks are sorted by squared distance of
  the chunk centre to the eye, `ChunkPos` as tie-break (the transparent list
  keeps its far-to-near sort).
- **Shared index buffer.** Every quad has the indices `[0,1,2,0,2,3] + 4q`
  (`Mesh::push_quad`, covered by a test). One `u32` index buffer of 4096 quads,
  doubled on demand, serves every chunk pass; meshes upload vertices only.
  `Mesh::indices` is still produced by the mesher and ignored by the chunk
  renderer; removing it is left to phase 4.

## Consequences

- Pixel-identical output (bench scene: 0 differing pixels). −14 % GPU time on
  the opaque pass in the bench scene (`docs/perf-log.md`).
- A chunk makes up to two opaque draws instead of one; phase 3 sections and
  phase 4 arenas will merge them.
