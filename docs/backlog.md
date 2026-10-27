# Backlog

Known work that is not part of a milestone in progress: gaps found while
fixing something else, follow-ups of the architecture review of 2026-10-24,
and risks that need a decision. Each entry says what is wrong, where, and the
reference to follow. Move an entry into a milestone or a task card when it is
picked up, and delete it when it is done.

## Rendering

### Animated textures (AST-6)
- **Problem:** water, lava, fire, portals and every texture with a `.mcmeta`
  animation show their first frame only.
- **Where:** the atlas build (`crates/umicraft-assets/src/atlas.rs`) and the
  renderer's texture array; liquids already sample the still/flowing layers.
- **Milestone:** M7 (first-run assets and resource packs) or a dedicated task.

### Liquid corners at a diagonal chunk boundary
- **Problem:** a liquid corner height averages the four columns that share the
  corner (`fluid_height` in `crates/umicraft-mesh/src/fluid.rs`). A level change
  re-meshes the chunks that share a face with the block only, so a corner on a
  diagonal chunk boundary can stay stale until that chunk re-meshes.
- **Where:** `dirty_for_block` / `dirty_for_edges` in
  `crates/umicraft-world/src/world.rs`.

## Game logic

### Respawn after death
- **Problem:** the client never sends the respawn request. A dead player stays
  dead, also after a reconnect, and its position reports make the server
  correct it every tick (1 774 resyncs in 90 s were measured).
- **Reference:** `C16PacketClientStatus` (`PERFORM_RESPAWN`) and the death
  screen of the 1.8 client; the server answers with `Respawn` (0x07).

### Fluid, ladder and web movement (Depth Strider)
- **Problem:** `umicraft_game::physics` has no water, lava, ladder, vine or
  cobweb movement, so Depth Strider (enchantment 8 on the boots) has nothing
  to act on. Speed, Slowness, Jump Boost, Blindness and the food rule are done
  (ADR-0021).
- **Reference:** `EntityLivingBase.moveEntityWithHeading` (water and lava
  branches, `EnchantmentHelper.getDepthStriderModifier`), `Entity.moveEntity`
  (web, ladder clamp), `Entity.handleWaterMovement`.
- **Milestone:** P2 follow-up (movement) or P7 with the fluid rendering.

## Performance (spec §7, ahead of M14)

- **No frame-time benchmark:** the M6 acceptance measured the meshing time as
  a proxy. Add a frame benchmark before any rendering optimisation.
- **No frustum culling:** every uploaded chunk is drawn each frame, including
  behind the camera.
- **Mesh queue without priority:** the workers share one FIFO behind a
  `Mutex` (`crates/umicraft-client/src/mesher.rs`); spec §7.1 asks for a
  priority by distance and view direction.
- **Each column is meshed about twice while a world loads,** because a new
  column dirties its loaded neighbours. Delaying the mesh until the four
  neighbours are loaded would roughly halve the work.
- The world benchmarks of 2026-10-24 were taken on a loaded machine
  (`docs/perf-log.md`); measure them again on an idle one.

## Robustness and tests

- **Fuzzing:** the decoder is only exercised by a random-bytes test
  (`crates/umicraft-testkit/tests/integration.rs`). Spec §11 and NET-13 ask for
  `cargo fuzz` targets and `proptest` on the packet decoders and the chunk
  parser.
- **CI coverage:** CI runs on Linux only and does not run the GPU tests,
  `cargo doc -D warnings` or a live Paper server.

## Architecture

- **`umicraft-core` is not used by any crate.** The service registry and the
  event bus wait for a second implementation of a service (ADR-0005 defers
  them to M15); the seams created by ADR-0010 to ADR-0013 are the candidates.

## Decisions needed

### Code derived from the decompiled client (ADR-0001)
- **Risk:** ADR-0001 waived the clean-room rule. `crates/umicraft-model/src/registry.rs`,
  several tables (block shapes, materials, normal cubes) and many comments are
  derived from MCP and cite its method names. The code is published under
  MIT/Apache-2.0 on `origin`.
- **Needed before any public release:** a legal review (spec §3, rule 8), or a
  clean-room re-derivation of the derived parts.
