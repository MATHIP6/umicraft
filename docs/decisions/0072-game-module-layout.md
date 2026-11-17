# ADR-0072 — Layout of the game thread module

Status: accepted (2026-11-16)

## Context

`umicraft-client/src/game.rs` had grown to 9 600 lines: 4 200 of code and
5 400 of tests in one inline module. One `Game` struct with 80 fields, and
`apply_net_event`, a single 640-line `match` over every clientbound packet.
ADR-0029 already noted the size ("about 4 400 lines, so none of this goes
there"), and new work kept adding to the file because there was nowhere else
to put it. A reader looking for the handling of one packet or one key had to
search the whole file, and every change touched the same file.

## Decision

`game` is a directory module, split by concern. Each file holds one step of
what the game thread does:

| Module | Holds |
|---|---|
| `session`, `game_loop` | the thread start and its loop (inputs, frame-paced network events, ticks) |
| `messages`, `look` | what crosses the threads; the lock-free mouse look slot |
| `state` | the `Game` struct and the questions asked of it |
| `net/` | `mod.rs` dispatches each `PlayEvent`; `player`, `world`, `entities`, `inventory`, `social` hold the handlers; `wire` the units of the entity packets |
| `tick` | one simulation step, in the order of MCP `Minecraft.runTick` |
| `actions/` | the input updates and the buttons: `attack`, `use_item`, `hotbar` |
| `abilities`, `movement`, `body`, `particles`, `combat`, `hud` | the parts of the tick |
| `commands`, `publish`, `view`, `chat` | what leaves the thread: serverbound commands, frames, HUD |
| `inventory_events`, `container_events`, `creative_events` | the window's screen events |
| `tests/` | one file per domain, shared helpers in `tests/mod.rs` |

Rules for code added later:

- A packet handler longer than a few lines goes into the `net/` file of its
  domain, and `apply_net_event` only calls it.
- Items shared between files are `pub(super)` (or `pub(in crate::game)` below
  `actions/` and `net/`); nothing is `pub` unless the rest of the crate uses
  it, and the public API is re-exported from `game/mod.rs`.
- A file that passes about 800 lines of code (tests excluded) is split.

## Consequences

- The split was a pure move: the bodies did not change, the 1 234 test names
  did not change, and the arms of `apply_net_event` became handlers verbatim.
- `Game` still has 80 fields that every function sees in full. Grouping them
  into sub-structs (vitals, hotbar, chat, server HUD, tick metrics), so a
  handler takes only the part it changes, is the next step and is not done
  here.
- Line references to `game.rs` in older documents (roadmap review, ADR-0016,
  ADR-0029, perf log) point at the file as it was then.
