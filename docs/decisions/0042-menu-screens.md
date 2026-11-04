# ADR-0042 — Menu screens

Status: accepted (2026-11-04)

## Context

P10 needs the screens around the game: title, server list, add/edit and
direct connect, delete confirmation, connecting, disconnected, statistics.
The windowed client is being reworked in parallel, so the screens must not
depend on it.

## Decisions

- **Pure immediate-mode screens in `umicraft_ui::menus`.** One file per
  screen, each a small state struct with `show(&mut self, ui, atlas, lang,
  ..) -> Option<Outcome>`. They draw through `Ui`, read no file or clock and
  own no GPU resource; animation and double clicks count `tick()` calls. The
  client maps its state to the view models (`ServerRow`, `RowStatus`,
  `StatRow`) and acts on the outcomes. Layouts are public pure functions
  (`title_layout`, `server_list_layout`, ...) so tests and the wiring share
  the MCP coordinates (GUI units times the GUI scale).
- **Texts are keys.** Every label is a vanilla key resolved by `Lang`
  (ADR-0039). The few texts vanilla hard-codes (`Pinging...`,
  `(no connection)`, `Can't connect to server.`) and Umicraft's own credit
  line are `umicraft.*` keys in the offline table.
- **Singleplayer is greyed.** Umicraft only plays on servers; the button
  stays so the screen matches vanilla.
- **Original branding.** The logo is the word "Umicraft" drawn with the UI
  font and the splash comes from an original list (`SPLASHES`); no Mojang logo
  or `splashes.txt`.
- **Panorama.** The UI crate provides the six opaque texture handles
  (`PanoramaTextures`), the face turns, the camera constants and the rotation
  (`yaw = -t * 0.1`, `pitch = sin(t / 400) * 25 + 20`); the renderer draws the
  cube in a later step. The vanilla blur (64 offset passes plus seven blended
  passes) is skipped. Without textures the default background is drawn.
  `umicraft_assets::panorama` decodes `panorama_0..5.png` from the user's
  client jar at runtime. The owner approved using them; spec §3 still holds,
  so they are never committed or released.
- **Server list view model.** `RowStatus::{Pinging, Online, Unreachable,
  VersionMismatch}`; `Online` also carries a player sample (tooltip) and the
  version name. The screen only reports `MoveUp(i)` / `MoveDown(i)`; the
  caller swaps the entries.
- **Deviations.** No LAN entries, no server icons, plain glyph arrows on the
  row icon, upright splash, no resource-pack toggle in Add/Edit, one
  `VersionMismatch` text instead of "Client out of date!" / "Server out of
  date!".

## Consequences

The client step (D) has to own the ping tasks, the server file, the screen
stack and the panorama cube; nothing here changes when it does.
