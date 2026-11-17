# ADR-0075 — Layout of the world view and of the headless script

Status: accepted (2026-11-16)

## Context

`umicraft-client/src/world_view.rs` was 4 092 lines, two thirds of them
tests, with 64 methods of `WorldView` in one `impl` block. `input.rs` held
two things: the input types that cross into the game thread, used by the
window, and the scripted session of the headless mode, used only by
`headless`.

## Decision

- The script moves to `headless/script.rs`, next to the session that plays
  it; `input.rs` keeps `InputState` and `InputUpdate`, which both input
  sources produce. The public path of the script is now
  `headless::script`.
- `world_view` follows ADR-0072 to ADR-0074: `mod.rs` keeps the struct,
  `new`, `sync`, the events it applies and the session reset; one
  `impl WorldView` block per concern in `meshing`, `camera`, `scene`,
  `session` and `pacing`; the interpolated camera (`View`) has its own file.
  `MeshTarget` and the pacing types keep their public paths through
  re-exports.
- The tests move to `world_view/tests/`: the shared harness in `mod.rs`, one
  file per concern (`meshing`, `session`, `camera`, `scene`, `pacing`,
  `body_yaw`) and the two audits of the camera and of the perspective key.

The fields of `WorldView` are not grouped: most of them are the newest
frame's data that `apply` replaces and `clear_session` resets together, and
a group would only rename them.

## Consequences

- The test names are the same before and after, with their new module
  paths; the client tests and clippy pass.
- `docs/parity-harness.md` points to the new file of the scripts, which it
  must stay in sync with.
