# 0063. Prioritised mesh queue, upload budget, worker count (F2-4)

Status: accepted

## Context

A burst of chunks (the first load, a teleport) went to the mesh workers in
arrival order through a bounded queue that took everything at once. What stood
next to the player could wait behind hundreds of far columns, the order could not
be corrected afterwards, `waiting` was scanned linearly, only four workers ever
ran, and every finished mesh of a burst was uploaded in one frame.

## Decision

- **Nearest first (F2-4a).** `MeshQueue` keeps the waiting snapshots in a map (the
  newest per chunk) and hands out the one with the lowest priority whenever fewer
  than `2 × workers` jobs are in flight. The priority is the squared distance of
  the chunk centre to the eye, plus 128² for a chunk more than 24 blocks behind the
  view direction. It is recomputed at every pick (a scan of the map, about a
  microsecond per hundred chunks), so it follows the player; a heap with stale keys
  would not. `WorldView::focus_on(camera)` feeds it every frame. A chunk with a job
  in flight still waits for its result, so meshes arrive in snapshot order.
- **Upload budget (F2-4b).** `collect` uploads at most 1 MiB of vertices per call
  (one mesh at least); the rest waits in the result channel. 4 MiB made frames of
  24–40 ms, 2 MiB 14–20 ms, 1 MiB 10–13 ms, like the old FIFO.
- **Worker count (F2-4d).** All cores but one, at least one, replaces the cap of
  four; `mesh_threads` in `options.toml` (`0` = automatic, at most 32) overrides it.
  There is no menu entry.
- **Fast chunk hash (F2-4e).** `ChunkMap`/`ChunkSet` in `umicraft-world` use a
  multiply-and-finalize hasher instead of SipHash, for the world, the renderer and
  the world view. It is not collision resistant: a hostile server could choose
  chunk positions that collide and slow its own client down, nothing more. The
  `rustc-hash` dependency was not needed.
- The liquid atlas layers are looked up once per job instead of once per liquid
  cell.

## Measured and not done

- **GPU buffer arena (F2-4c).** An upload costs about 0.3 ms per chunk including
  the vertex buffer creation; with the budget the render thread spends 0.9 ms per
  frame in `sync` while loading (max 1.7–3.5 ms). Sub-allocating pages would save a
  part of that and is not worth its complexity now. `multi_draw_indexed_indirect`
  is left for when a profile shows draw submission as the bound.
- **Copy-on-write sections (F2-4f).** An edit while a mesh job holds the column
  copies it in 2.7 µs (`world/set_block/column_held_by_a_snapshot`, against 34 ns
  without a holder). Not a cost worth a data-structure change.
- **Recycling the mesh vectors (F2-4d).** The regrowth happens on the workers,
  which are not the bound; not done.
