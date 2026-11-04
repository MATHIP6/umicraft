# ADR-0043 — Options completeness

Status: accepted (2026-11-04)

## Context

P10 asks that a player can change every vanilla option from the screen and
that the options survive a restart. Before this step the options file had
video, mouse, chat, language and key sections, the pages for skin, sounds,
resource packs and language were inert hub buttons, and most video entries
were drawn but did nothing.

## Decisions

- **New sections in `options.toml`.** `[video]` gains `smooth_lighting`,
  `gui_scale`, `gamma`, `view_bobbing`, `mipmap_levels`, `vsync`,
  `fullscreen`, `block_alternatives` and `entity_shadows`; `[skin]` has the
  seven model parts; `[sound]` has the master and eight category volumes;
  `[resource_packs]` has `enabled`, the file names in the `resourcepacks`
  folder, lowest priority first. Defaults and ranges are those of MCP
  `GameSettings`. Every section is clamped on load; a pack name with a path
  separator or `..` or a repeat is dropped.
- **Clouds are three-state** (off, fast, fancy) and independent of Graphics,
  as in 1.8.8; `[video] clouds` changed from a boolean to a number (a boolean
  in an older file is still read).
- **Skin bitmask.** `SkinOptions::parts_mask` ORs `1 << partId` of the shown
  parts (`EnumPlayerModelParts`). The byte goes in `PlayOptions` for the
  entry packet and in `GameEvent::ClientSettings` / `ClientCommand` when the
  options close with a different `ServerSettings`. Main hand is not offered:
  it is a 1.9 field and the 1.8 packet has none.
- **Pages.** One file each in `options_screen/` (`skin_page`, `sounds_page`,
  `packs_page`, `language_page`), the video page completed. The screen reads
  no file: the window hands it a `Catalog` (languages of the asset index,
  packs of the folder, via `umicraft_assets::pack_discovery`) when the options
  open. `Outcome` gains `language_changed`, `packs_changed`, `skin_changed`
  and `open_packs_folder`; the window reloads the `Lang`, the resources, or
  opens the folder. The pack page edits a draft that Done applies, as vanilla
  does. `--resource-pack` on the command line replaces the saved list.
- **Applying.** `options_effects` holds what the options do outside their
  screen: `ServerSettings`, `pack_paths` and `VideoApplier` (GUI scale,
  vsync, fullscreen, entity shadows). GUI scale is one process-wide value in
  `umicraft_ui::screens` read by `gui_scale`, because every screen derives
  its scale from the window size alone. Brightness feeds the render
  environment, view bobbing gates the camera matrix.
- **Not faked.** Smooth Lighting, Mipmap Levels and Other Block Variants are
  stored and shown, but nothing reads them yet (see `docs/parity.md` for what
  each needs). 3D Anaglyph and Use VBOs are not offered.

## Consequences

- The sound volumes are ready for P11, which only has to read `[sound]`.
- Brightness now starts at Moody like the reference; the renderer used to
  start at 0.5.
- A future mesher switch for smooth lighting needs a flag in
  `umicraft_mesh::quad_light` and a re-mesh (`ReMeshAll` exists).
