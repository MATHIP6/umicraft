# ADR-0037: P9-d chat module and the render to game dependency

Status: accepted (2026-11-02)

## Context

Chat wrapping, the pointer hit-test and the click rules must agree: vanilla
wraps once with the font (`GuiNewChat.setChatLine`) and tests the same rows in
`getChatComponent`. The font lives in `umicraft-ui`/`umicraft-render`, the
rules in `umicraft-game`.

## Decision

- `umicraft-game/src/chat/` holds all pure chat logic (components, styles,
  `LangTable`, events, wrapping, history, completion, commands, input).
- The wrapping algorithm is generic over a `Piece` trait and a glyph advance
  closure. `umicraft-render` therefore depends on `umicraft-game` (no cycle: the
  game crate does not know the renderer) and implements `Piece` for its
  `ChatRunView`, so the draw code and `chat_run_at` share one implementation.
- `ChatRunView` carries the click, hover and insertion data of its run; the
  window reads them from the last frame's hit-test.
- Clicks follow ADR-0017: only `run_command` and `suggest_command` act, and
  only on a player click; links are never followed.
- `PlayEvent::SystemChat` (position 1) is new so the chat visibility
  "commands only" can tell system messages from player messages.
