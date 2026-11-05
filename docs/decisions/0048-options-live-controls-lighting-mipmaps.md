# ADR-0048 — Live options, mouse bindings and the render options

Status: accepted (2026-11-05)

## Context

ADR-0043 stored Smooth Lighting, Mipmap Levels and Other Block Variants
without effect and bound keys only by `KeyCode`, while vanilla allows mouse
buttons and the game input read a few fixed keys.

## Decisions

- **`Binding {Key, Mouse}`** replaces `KeyCode` in `KeyBindings`. `options.toml`
  stores key names as before and `MouseN` for a button; a file without the new
  actions keeps their defaults. The window tracks held keys and buttons and
  runs every action through one `press_binding` / `held` path. Actions added:
  Attack, Use, Pick Block, Drop, Screenshot, Perspective, Fullscreen. Smooth
  camera, spectator outlines and the streaming keys have no backend and are
  not offered.
- **Screenshot.** The window surface is configured with `COPY_SRC` when the
  adapter allows it; the renderer copies the presented frame on request and
  the client writes `screenshots/<UTC time>.png` in the data folder.
- **Smooth Lighting** is `umicraft_mesh::Lighting` (Smooth, Flat). 1.8.8 tests
  `ambientOcclusion != 0`, so levels 1 and 2 mesh alike. Light-emitting
  blocks are flat. Mesh workers read the options from `client::mesh_settings`
  (process-wide, like the GUI scale); a change re-meshes all columns.
- **Mipmaps.** `Renderer::set_mipmap_levels` fixes the chain of the next
  block atlas upload; `umicraft-render::mipmap` reproduces
  `TextureUtil.generateMipmapData` including its first-`levels+1`-texels
  transparency flag. Chunks use a nearest-mipmap-linear sampler; the shared
  block sampler is clamped to level 0 for items, particles and icons. A
  changed option reloads the atlas.
- **Other Block Variants** off selects `WeightedBakedModel`'s base variant
  (`MeshSettings::block_alternatives`).
- **Snooper page** exists with a stored, never used switch. The Client
  Settings locale is the lowercase language code. Resource packs of another
  `pack_format` ask for confirmation.
- **Not done:** pack icons (no arbitrary-texture UI path), Super Secret
  Settings, Force Unicode Font and the unicode glyph renderer.

## Consequences

- Benchmarks of the mesher stayed within noise (`docs/perf-log.md`).
- Moving a movement action to a mouse button works, as does attack on a key.
