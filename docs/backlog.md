# Backlog

Known work that is not part of a milestone in progress: gaps found while
fixing something else, follow-ups of the architecture review of 2026-10-24,
and risks that need a decision. Each entry says what is wrong, where, and the
reference to follow. Move an entry into a milestone or a task card when it is
picked up, and delete it when it is done.

## Rendering

### Item animations (clock, compass)
- **Problem:** the block textures animate (ADR-0023) but the clock and the
  compass item textures stay on their first frame. The reference drives them
  by the world time and the heading (`TextureAtlasSpriteClock/Compass`), not by
  the `.mcmeta` timeline, and no item is drawn before P3 anyway.
- **Where:** `build_atlas` in `crates/umicraft-assets/src/client_resources.rs`
  skips every key outside `blocks/`.
- **Milestone:** P3 (item models).

### Liquid corners at a diagonal chunk boundary
- **Problem:** a liquid corner height averages the four columns that share the
  corner (`fluid_height` in `crates/umicraft-mesh/src/fluid.rs`). A level change
  re-meshes the chunks that share a face with the block only, so a corner on a
  diagonal chunk boundary can stay stale until that chunk re-meshes.
- **Where:** `dirty_for_block` / `dirty_for_edges` in
  `crates/umicraft-world/src/world.rs`.

### UI vertex colours are not linearised for the sRGB target
- **Problem:** `gui_overlay.wgsl` returns `texel * vertex_color` and the render
  target is sRGB (`Rgba8UnormSrgb` headless, the adapter default on a window),
  so a colour written for a plain 8-bit framebuffer comes out too light: the
  vanilla tooltip background `0x100010` showed as a bright purple and the
  blue `0x5555FF` text as periwinkle. White and black are unaffected. Only the
  tooltip is fixed, on the CPU (`linear` / `vanilla` in
  `crates/umicraft-client/src/tooltip_draw.rs`, commit f379d02); every other
  coloured UI element (coloured chat and HUD text, button and slot fills, the
  death screen, the debug screen) still has it.
- **Also suspected:** every multiplicative brightness factor applied to a texel in the shaders (world face shade 0.6 / 0.8 / 0.5, entity shade, item lighting) is multiplied in linear space, so it darkens less than the vanilla factor does on 8-bit values. The GUI block icons are fixed by converting the factor (`gui_face_factor` in `crates/umicraft-mesh/src/item.rs`); check the terrain and entity shaders against the vanilla screenshots.
- **Fix to investigate:** convert the vertex colour in the shader
  (`pow(rgb, 2.4)` piecewise sRGB decode); tinting a texel by a colour is exact
  in linear space. Then remove the CPU conversion of the tooltip, and re-tune any
  colour that was adjusted by eye. Alpha blending also happens in linear space
  while vanilla blends the sRGB values, so translucent fills (dimming
  `0.55` black, drag highlight) differ slightly; decide whether to emulate the
  vanilla blend or accept it. Compare with the vanilla screenshots
  (owner's set, outside the repository, spec §3).
- **Where:** `crates/umicraft-render/src/gui_overlay.wgsl`,
  `crates/umicraft-render/src/ui_pass.rs`; the surface format is chosen in
  `Renderer::new_windowed`.
- **Milestone:** P9 / P10 (HUD, screens) or the screenshot comparison of P13.

## Game logic

### Item use and pick block gaps (ADR-0022)
- **Problem:** the use and pick tables are hand-written (item ids for food,
  drinks, swords and the bow; block-to-item redirects and variant masks). Items
  whose `onItemUse` succeeds locally (hoes, seeds, signs, doors, bone meal)
  still send the in-air use packet after the block one, and do not swing.
  Picking an entity (creative) and the `Ctrl` + pick block-entity copy are
  missing; the bow draw animation, its FOV zoom and the item models need P3.
- **Milestone:** P3 (item registry) removes the tables; entity picking is P6.

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

## Entities

### Critical-hit particles
- **Problem:** `Animation` 4 and 5 (crits) need the particle system (P8).
