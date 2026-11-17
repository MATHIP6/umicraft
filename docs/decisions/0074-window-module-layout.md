# ADR-0074 — Layout of the window module

Status: accepted (2026-11-16)

## Context

`umicraft-client/src/window/mod.rs` was 2 259 lines. Its `App` struct had 97
fields, `redraw` was 389 lines and `build_ui_frame` 320. The module already
had a pattern for this, files that each add an `impl App` block
(`chat_control`, `session_control`, ...), but most of the code had stayed in
`mod.rs`.

## Decision

The window module follows its own pattern throughout, as ADR-0072 and
ADR-0073 did for the game thread and the renderer.

- `mod.rs` keeps `run`, the `App` struct and what its files share.
- One `impl App` block per concern: `startup` (`App::new`, the background
  assets), `keyboard`, `player_input`, `screen_state`, `death`, `redraw`,
  `ui_frame` and `handler` (the winit `ApplicationHandler`), next to the
  existing `*_control` files.
- The fields of `App` that belong together are groups in `window/state/`:
  `input`, `presses`, `chat`, `clock`, `assets`, `options_menu` and `frame`.
  `App` keeps 45 fields; `options` and `lang`, read everywhere, stay on it.
  The frame timing is a method of its group (`FrameClock::tick` and
  `schedule_next`), with unit tests.
- `redraw` and `build_ui_frame` keep the order of a frame and call named
  steps: `send_scene`, `upload_entities`, `draw_portrait`,
  `record_input_latency`; `build_hud` and `draw_screen`.

Two borrow patterns make the steps possible, and new steps should use them
rather than grow the long functions again:

- The renderer leaves `self` while the scene is handed to it
  (`self.renderer.take()`, then `insert`), so a step can take
  `&mut Renderer` and `&mut self` together. A step called this way must not
  read `self.renderer`.
- The UI font atlas is an `Arc`, so a frame holds its own handle instead of
  a borrow of `self` for its whole length.

## Consequences

- No test covers `redraw` or `build_ui_frame`, which need a window. The
  change was checked in a real window: the title screen draws about 610
  frames in 12 seconds before and after, with no error in the boot log.
- One measurement showed 15 frames instead of 600; alternating runs of the
  two builds showed that it came from the window being hidden at the time
  (Wayland throttles the frame callbacks of a hidden window), not from the
  code.
