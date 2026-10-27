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

### Interaction rules in `umicraft-game` (architecture review, track 2)
- **Problem:** digging, placement, the placement acknowledgement and the
  position report are free functions over a private 27-field `Game` in
  `crates/umicraft-client/src/game.rs`, built from shallow `umicraft-game`
  pieces. None of `start_break`, `place_block`, `finish_break`,
  `acknowledge_block`, `apply_placement` or `apply_correction` is tested.
- **Proposal:** `umicraft-game` takes the inputs and the server events and
  returns the packets to send; the client keeps the channels only. Needed
  before M8 (entities) and M9 (inventory) build on it.

### Respawn after death
- **Problem:** the client never sends the respawn request. A dead player stays
  dead, also after a reconnect, and its position reports make the server
  correct it every tick (1 774 resyncs in 90 s were measured).
- **Reference:** `C16PacketClientStatus` (`PERFORM_RESPAWN`) and the death
  screen of the 1.8 client; the server answers with `Respawn` (0x07).

### Effects and enchantments in the movement
- **Problem:** Depth Strider (`Player.depth_strider`, ADR-0014), Speed,
  Slowness and Jump Boost are not applied.
- **Milestone:** M8 (entity effects) and M9 (inventory).

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

- **Only the config store of `umicraft-core` is used** (by the options of M10). The service registry and the
  event bus wait for a second implementation of a service (ADR-0005 defers
  them to M15); the seams created by ADR-0010 to ADR-0013 are the candidates.

## Chat, HUD and options (left by M10)

- **Clicks and hover in the chat:** `clickEvent` and `show_text` are parsed and
  kept on each run, but the chat has no pointer interaction. `open_url` must
  stay unfollowed or ask for confirmation (ADR-0017 decision 2).
- **Vsync option:** the renderer has no way to change the present mode.
- **Not run against a live server:** the chat, the tab completion, the
  scoreboard, the titles and the options screen are covered by unit tests and
  `--hud-demo` only. Run a session on Paper or PandaSpigot 1.8.8, including a
  `/scoreboard`, a `/title` and a Wither.
- **Head icons in the tab list:** 1.8 draws them on encrypted connections only,
  so they arrive with M13.
- **Two fallback fonts:** the HUD overlay font
  (`crates/umicraft-render/src/font.rs`) and the UI fallback font
  (`crates/umicraft-ui/src/text.rs`) have different glyph sets; the UI one still
  lacks `<` and `>`.
- **Options screen on short windows:** the controls tab needs about 540 pixels
  of height and is not scrollable.
- **`--view-distance` has no "not given" state,** so a value of 8 cannot
  override a saved render distance.

## Decisions needed

### Code derived from the decompiled client (ADR-0001)
- **Risk:** ADR-0001 waived the clean-room rule. `crates/umicraft-model/src/registry.rs`,
  several tables (block shapes, materials, normal cubes) and many comments are
  derived from MCP and cite its method names. The code is published under
  MIT/Apache-2.0 on `origin`.
- **Needed before any public release:** a legal review (spec §3, rule 8), or a
  clean-room re-derivation of the derived parts.
