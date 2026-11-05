# ADR-0049 — P10 F3: connection stages, statistics, achievements, title polish

Status: accepted (2026-11-05)

## Context

ADR-0042 and ADR-0044 left the connecting screen with one text, a general-only
statistics list, an inert Achievements button, an upright splash, no Realms
row and an unblurred panorama (ADR-0045).

## Decisions

- **Typed connection end.** The network thread sends `GameEvent::ConnectStage`
  and ends with a `ConnectionEnd { phase, cause }` (pure mapping from
  `NetError`, tested). `Connection::connect` resolves the name first, so
  `NetError::UnknownHost` is told apart from a refused connect. The window turns
  the end into `connect.failed` or `disconnect.lost` plus a reason in the
  player's language.
- **Statistics.** `stats_view` builds four tabs from the `name -> value` pairs
  (registry names after `stat.<kind>.minecraft.`; entity names after
  `killEntity`). The UI screens know no ids. `WorldView` counts answers
  (`stats_revision`) and tracks `stats_loading`; the menu rebuilds only on a new
  revision, so tab, scroll and sorts survive a refresh.
- **Achievements.** The 34 cells are data in `umicraft_game::achievements`.
  The screen is pure and gets resolved nodes. Eight block textures join the GUI
  atlas as `blocks/<name>` for the map tiles. The toast needs a baseline: only
  an answer after the first one can unlock one (`handleStatistics`).
- **Turned text.** `TextStyle::rotation` turns a run around its origin; the UI
  pass emits turned glyph quads (`Overlay::push_corners`). The splash uses it.
- **Panorama blur.** `panorama::blur` renders 64 jittered cube passes into a
  256x256 `Rgba8Unorm` texture, ping-pongs seven box filters and blits with the
  old crop and quarter turn. The sharp pass stays behind
  `Renderer::set_panorama_blur(false)`.
- **Realms** is not drawn (owner decision); the title layout stays without that row.

## Consequences

- The blur costs about 16 ms per title frame on the dev adapter
  (docs/perf-log.md); it runs on the title screen only.
- Remaining differences are listed in docs/parity.md (locked icons, craft `-`,
  no smooth scroll, UTC date, no logo easter egg).
- Nothing here was run in a real window by the author; the owner validates.
