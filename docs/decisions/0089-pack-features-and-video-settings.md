# 0089. Pack features and OptiFine-style video settings

Status: accepted (2026-11-21, project owner)

## Context

The owner wants the OptiFine layout for video settings, its optimisation knobs,
and its resource-pack features. These features change the mesher, the entity
models and the sky pass. Neither `BuiltinMod` (ADR-0065) nor the F3 modding API
(ADR-0086/0087) can reach those parts, so the features are core.

Four gaps exist today:
- `ClientResources::apply_packs` drops every pack file outside its known path
  classes, including `optifine/**`, `mcpatcher/**`, `textures/environment/**`
  and the entity textures.
- `upload_sky_textures` reads only the jar.
- `VideoOptions` is one flat struct.
- The video page is a hard-coded list with its own effect code.

## Decision

### Crates

- **New crate `umicraft-packext`.** It holds pack-feature parsing and
  evaluation, with no GPU or window dependency. Modules:
  - shared parsing: `properties`, `ranges`, `filters` (biomes, heights,
    weather);
  - one module per feature: `sky`, `cem` (with `cem::expr`), `ctm`,
    `random_entities`, `colors`, `natural`, and so on.
- Each feature turns raw pack files into a typed plan (`SkyPlan`, `CemModel`,
  `CtmRules`, ...). Parse errors are typed (`thiserror`) and stay local: a bad
  file drops that one layer, model or rule, and logs a warning.
- `umicraft-render`, `umicraft-mesh` and `umicraft-model` consume these plans.
  They never read `.properties` or `.jem` themselves.
- **Pack file access.** A `PackFiles` view over the resource stack exposes the
  path classes the features need. Lookups try `optifine/` first, then
  `mcpatcher/`. Sky and entity textures are read through the pack stack, with
  the jar as the bottom layer.

### Options

- `VideoOptions` is split into TOML subsections: `[video]`,
  `[video.details]`, `[video.quality]`, `[video.animations]`,
  `[video.performance]` and `[video.other]`.
  - Thanks to `serde(default)`, old files keep loading.
  - Existing keys keep their current place.
- **One declarative table (`OptionSpec`) describes every video option once.**
  Each entry has a key, a label key, a kind (toggle, cycle, slider) and an
  `Effect` (`Immediate`, `ReMesh`, `ReloadResources`, `ReloadSky`,
  `ResetClouds`).
  - The pages, the tooltips (`<label key>.tooltip.N` language keys) and the
    apply step are all derived from this table.
  - On a change, the strongest effect among the changed options wins. This
    replaces the hand-written `VideoEffects` code.
- **Defaults follow OptiFine**, except where a faster choice costs nothing or
  almost nothing on screen (owner, 2026-11-21; the deviations are listed in the
  plan, section "Defaults"). OptiFine defaults include Connected Textures
  Fast, Custom Sky / Custom Colors / Random Entities / Custom Items on, and
  Fog Fast. The deviations include Trees Smart, Dynamic Updates on, and Chunk
  Updates following the current multithreaded mesh budget instead of one
  section per frame.
  - Parity screenshot tests pin vanilla-equivalent settings explicitly, so
    they stay independent of these defaults.
- **"Default" values.** `GraphicsChoice { Default, Fast, Fancy }` resolves
  `Default` against the Graphics setting when the value is used, as OptiFine
  does.
- **Render distance goes up to 32 chunks** (owner, 2026-11-21).
  - Above the server's view distance nothing more is received, so the value
    mostly matters on servers that allow it.
  - The mesh queue, the fog range and the cloud cutoff follow the setting.
- **Options with no effect in Umicraft are not shown.** These cover integrated
  server options (Smooth World, Lazy Chunk Loading, Autosave, Time, Weather),
  GL-only options (Fast Math, Smooth FPS, VBO, Fast Render), Anaglyph
  (ADR-0043) and shaders (RND-13).

## Consequences

- Pack formats are testable and fuzzable without a GPU.
- Adding a video option takes one table entry, one field and two language
  lines. Its page placement, tooltip and reload behaviour come from the table.
- The core grows: `umicraft-packext` becomes a dependency of the client,
  render, mesh and model crates.
- Every claimed optimisation needs a criterion benchmark and before/after
  numbers in `docs/perf-log.md`. An optimisation with no measured gain is not
  kept.

## Amendments

- 2026-11-21 (phase 1): `Effect` has the three variants that have a consumer
  today (`Immediate`, `ReMesh`, `ReloadResources`). `ReloadSky` and
  `ResetClouds` come with the features that need them. VSync is no longer a
  separate button: as in OptiFine, it is the left end of the Max Framerate
  slider (`vsync` and `max_fps` stay two fields in `options.toml`). Cave
  culling (`cave_culling`, new) and the mesh threads, until then file-only,
  are on the Performance page. Option tooltips use the language keys
  `umicraft.options.tooltip.<id>.N`, with original texts.
- 2026-11-21 (phases 4 to 6): custom sky textures are uploaded as their own sky images and drawn by one pre-built pipeline per blend mode; CEM textures use `TextureSlot::Cem(i)` and a replaced part keeps the vanilla motion (rest pose plus the vanilla delta); CTM tiles join the block atlas as `blocks/ctm/<dir>/<n>` and `ModelMeshContext` carries the resolved `CtmTable`.
