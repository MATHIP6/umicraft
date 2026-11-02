# ADR-0036 — HUD, chat and social overlays: pure model in `umicraft-game`, one renderer in `umicraft-render`

**Status:** accepted
**Date:** 2026-11-02
**Milestone:** P9

## Context

M10 left two HUD implementations side by side: `render/hud.rs` (coloured
rectangles on the `Overlay`, built-in font) and `render/hud_gui.rs` (vanilla
textures on the `DrawList`), plus `chat_hud.rs` and `social_hud.rs`. The client
builds one monolithic `HudState` in `game.rs` (5000+ lines) and resends it
whenever `hud_dirty` is set. Several files exceed 1000 lines.

## Decision

- **Logic** (vitals, XP, held-item name, boss bar, tab-list ordering, sidebar,
  team decoration, F3 lines, chat components, history, completion) lives in
  `umicraft-game` (`hud/`, `chat/`, `debug_f3.rs`). It has no GPU dependency and
  is tested against the MCP rules.
- **Drawing** is one renderer, `umicraft_render::hud`, on the `DrawList` with
  the `widgets.png` / `icons.png` sheets. The `Overlay` HUD survives only as
  `hud/legacy.rs` for the demos and is deleted in P9-h.
- **Styled text** has one implementation, `umicraft_ui::styled_text`, used by
  chat, tab list, sidebar, titles and tooltips.
- **GUI scale** has one implementation, `umicraft_ui::ScaledResolution`
  (MCP `ScaledResolution`, including the 1–4 option).
- `umicraft-client` only assembles per-zone views (`hud_view.rs`) for the render
  thread; `game.rs` gets wiring, not logic.
- Modules stay under about 400 lines.
- Language files: `LangTable` is built from the downloaded `.lang` files
  (AST-8); the built-in table remains only as a fallback before first-run setup.
- F3 becomes the vanilla debug screen; the Umicraft debug data moves to another
  key (F4, approved by the owner on 2026-11-02), recorded in `docs/parity.md`
  as an approved deviation.

## Consequences

Steps P9-b to P9-g touched disjoint files and were developed in parallel in
worktrees. P9-h deleted the legacy overlay HUD (`legacy.rs`, `chat_hud.rs`,
`social_hud.rs`, `layout.rs`, `HudLayout`, `draw_hud`, `draw_debug_only`):
without the GUI atlas the client still draws chat, titles, sidebar and F3 with
the built-in font through `draw_hud_gui`, and shows no hotbar or status rows.
The demo captions use the F3 text path (`client::caption`).

`HudState` is not yet replaced zone by zone: it stays one struct that the game
thread rebuilds with the single `hud_dirty` flag, because the zones share one
channel message and splitting the flag would need one message per zone. That
is deferred (see `docs/tasks/p9/README.md`, "Not done"). The F3 world facts
travel in the per-tick `Frame` instead.

## Addendum (P9-g): F3 and F4

F3 toggles the vanilla text debug screen (`umicraft_game::debug_f3`, drawn by
`umicraft_render::hud::gui::debug_f3`; text colour `0xE0E0E0` without shadow,
as in vanilla). The Umicraft debug screen (`Screen::Debug`) moves from F2 to
**F4**: *approved by the owner (2026-11-02)*.
F2 stays free for the vanilla screenshot (not implemented). The former F4
resource reload is dropped; F3+T still reloads the resources.
