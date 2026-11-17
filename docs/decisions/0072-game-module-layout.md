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
| `state/` | the `Game` struct and the questions asked of it; its field groups (`vitals`, `hotbar`, `controls`, `chat`, `social`, `metrics`) |
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
- `Game` had 80 fields that every function saw in full. A second pass moved
  36 of them into groups under `state/`: `Vitals`, `Hotbar`, `Controls`,
  `ChatState`, `Social` and `TickMetrics`, each in its own file with its
  `Default`. `Game` keeps 44 fields. A rule that only touches one group is a
  method of that group with its own unit tests (the chat history limit, the
  death animation, the digit keys and slot sync, the title fade, the
  timings); the free functions keep what spans several parts of the state
  (the windows, the HUD flag, the command queue). New fields go into the
  group they belong to rather than onto `Game`.
- Line references to `game.rs` in older documents (roadmap review, ADR-0016,
  ADR-0029, perf log) point at the file as it was then.
