# ADR-0047 — P10-F1: server list parity, font grid and dynamic GUI images

Status: accepted (2026-11-04)

## Context

The first screenshot comparison of the multiplayer screens with vanilla 1.8.8
showed accented letters as `?` (`Prisme B?ta`), descriptions without their
colours, a dark square instead of the server icon, red latency bars instead
of the red X and no resource pack setting on the Add/Edit screen.

## Decisions

- **Whole font grid.** `umicraft_ui::font_grid::GRID_CHARS` is the `charList`
  of the reference `FontRenderer` (a table of characters, not an asset). A glyph
  is built for every non-`\0` cell of the user's `ascii.png`, with the widths of
  the existing column scan. Characters outside the grid (the euro sign, CJK)
  still show `?`, as in vanilla. This applies to all text, chat included.
- **Colours stay `§` codes.** `ServerStatus::motd_text` converts the JSON
  `color` and style flags of each component (with inheritance) to legacy codes,
  like `getFormattedText`. `umicraft_ui::legacy_text` parses the codes into
  `StyledRun`s and wraps with the active format carried over a break
  (`getFormatFromString`). Tooltip rows accept codes too.
- **Row drawing follows `drawEntry`.** No shadow on the name and the
  description (`drawString`), the population text and version as coloured
  strings, a mismatch keeps the description and shows the server's version.
  The arrows and shade appear on the hovered row (the `isSelected` argument of
  vanilla is the hover); the selection is the grey outline of `GuiSlot`.
- **Dynamic GUI images.** Favicons are decoded to 64x64 RGBA by the client and
  appended below the jar part of the GUI atlas (`GuiAtlas::with_dynamic`,
  regions `dynamic/server<row>`); the window re-uploads the atlas when
  `MenuFlow::icons_revision` changes and only while the list is shown. No new
  GPU texture or pipeline. Calling it again with the current set replaces or
  drops icons, so an entry that leaves the book frees its pixels. The atlas
  keeps its power-of-two padding out of the appended part so a tall atlas stays
  under the GPU texture limit. `misc/unknown_server` joins the atlas from the jar.
- **Keys of `GuiMultiplayer.keyTyped`.** F5, Up/Down (Up on the first row or
  Down on the last clears the selection; arrows do nothing without one),
  Shift+Up/Down reorder, Enter. Home, End, Page keys and Delete do not exist in
  1.8.8 and are not added.
- **Resource pack mode.** `ResourcePackMode` (Prompt, Enabled, Disabled) is part
  of `ServerEntry`, saved as the optional `resource_packs` key of `servers.toml`
  (old files read as Prompt), and cycled by the button of the Add/Edit screen.
  Handling the server's resource pack packet is P12.

## Consequences

- Old `servers.toml` files keep working; a Prompt entry writes no new key.
- The demo `--hud-demo --hud-view servers` shows one row of each look.
- A dynamic image wider than the atlas widens it.
