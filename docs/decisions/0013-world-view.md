# ADR-0013 — One world view for the window and the screenshot path

- Status: Implemented
- Date: 2026-10-24
- Deciders: project owner
- Supersedes: none
- Related: ADR-0005 (game thread), ADR-0008 (clock), ADR-0012 (terrain
  textures)

## Context

The window and the screenshot path each re-implemented the render thread's
side of the game: reading the frame and chunk channels, interpolating the
camera and the clock, queueing snapshots for the mesh workers and uploading
the results. The copies had drifted. The screenshot path kept the old mesh
of a chunk that became empty, kept meshing the queued snapshots of a world
that a `Reset` replaced, and counted a re-meshed chunk's faces again (305
chunks and 175 529 faces reported for a 13 × 13 view that has 169 chunks and
91 474 faces). Both copies also drew a mesh of the old world when it came
back from a worker after the reset.

## Decision

1. **`WorldView`** (client) owns the camera/clock interpolation, the chunk
   queue, the per-chunk face counts, the mesh pool and the newest HUD, status
   and end reason. `sync(frames, chunks, &mut impl MeshTarget)` moves it one
   step and never blocks.
2. **`MeshTarget`** (`upload_mesh`, `clear_meshes`) has two adapters: the
   renderer and a GPU-free test target. An empty mesh removes the chunk, as
   `Renderer::upload_mesh` already did.
3. **The window's behaviour is the reference:** an emptied chunk is removed,
   a reset clears the queue, the counts and the meshes, faces are counted per
   chunk.
4. **Mesh jobs carry a generation.** `MeshPool::submit` takes an opaque tag
   that the result echoes; a reset increments the view's generation and
   results of an older one are dropped. `sync` collects results before it
   dispatches new jobs, so a job never comes back in the call that sent it.
5. **The headless session stays out**: it renders nothing and only builds a
   report from the events.

## Consequences

- The window and the screenshot path call `sync` and read the view; neither
  keeps its own copy of the camera, queue or counts.
- The screenshot report now gives the real chunk and face counts.
- The window's pool is created inside its `WorldView`, with the renderer.
