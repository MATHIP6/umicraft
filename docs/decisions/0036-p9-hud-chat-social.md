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
  key (proposed F4), recorded in `docs/parity.md` as an approved deviation once
  the owner confirms.

## Consequences

Steps P9-b to P9-g touch disjoint files and can be developed in parallel in
worktrees. The `HudState` type is replaced zone by zone; `hud_dirty` becomes a
per-zone flag.
