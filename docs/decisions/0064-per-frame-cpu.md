# 0064. Per-frame CPU work (F2-5)

Status: accepted

## Context

Several render calls ran every frame and allocated GPU buffers or vectors each
time (F2 audit D1 to D6). A measurement (`crates/umicraft-render/tests/frame_cpu_costs.rs`,
GPU, `--ignored`) gave the size of each.

## Decision

- **Persistent entity buffers (F2-5a).** `BatchBuffer` keeps one growing vertex
  buffer per pass (entities, shadows, glow, ghosts, glint, hand), rewritten each
  frame straight into the staging memory (`Queue::write_buffer_with`) with the
  batches laid end to end; a batch is a slice of it plus its texture. The entity
  pass draws from slices (`wgpu::BufferSlice`) instead of one buffer per batch.
- **Name tags (F2-5b).** The two buffers persist (`GrowBuffer`) and the vertex and
  depth lists are reused. The draws are **not** merged: vanilla draws a tag's
  boxes and text tag by tag, and merging the "always" and "tested" parts of all
  tags would change how overlapping tags blend.
- **Particles (F2-5c).** `WorldView::particle_sprites_into` fills a vector the
  window keeps, instead of returning a new one.
- **Frame latency (F2-5e).** `desired_maximum_frame_latency` is 1 (was the wgpu
  default 2). **Not measured**: there is no display in the test environment, so
  the input latency is unchanged in the numbers; this is a one-constant change
  (`FRAME_LATENCY`), to revert if a GPU-bound scene loses frames.

## Measured and not done

- **UI tessellation cache (F2-5d).** `UiPass::rebuild` of a 180-command list costs
  37 µs and comparing two lists 1 µs, so a cache would save at most that on frames
  whose list did not change, and it needs an invalidation epoch for the font,
  brand, GUI and icon atlases and the viewport. Under 0.5 % of a 60 fps frame for
  a risk of stale HUD pixels: not done.
- **Block entities (F2-5c).** Re-meshing only on change needs the animation
  state (lids, books, signs) and the distance cut in the key; the cost is
  proportional to the block entities in range and zero without any. Not done.
- **One sky uniform write (F2-5e).** A few microseconds; not done.
