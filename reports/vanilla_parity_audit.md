# Vanilla parity audit: Umicraft against Java 1.8.9

Status: **initial read-only audit.** No source file was changed. Date: 2026-11-11.
Reference: the MCP 1.8.9 tree in `src/minecraft` (read line by line where a line number is given).
Evidence labels: **Confirmed** = read in both trees, line numbers quoted. **Likely** = follows from confirmed
facts, no runtime proof. **Unverified** = hypothesis only.
Detail reports: `physics_parity.md`, `rotation_parity.md`, `network_protocol_parity.md`,
`mmc_compatibility_investigation.md`.

## 1. Repository state

- Branch `main`, 629 commits, last `eba357d` (F1 account manager).
- **113 paths are modified or untracked and uncommitted** (94 tracked files, +8418/-2067 lines). They include
  `crates/umicraft-game/src/shapes.rs` (collision maths), `crates/umicraft-client/src/game.rs`,
  `crates/umicraft-client/src/network/thread.rs` and a new `crates/umicraft-render/src/frame_pacing.rs`.
  Every finding below is about the **working tree**, not `HEAD`. Nothing was overwritten, staged or committed.
- A partial `reports/rotation_parity.md` from an earlier pass existed; it is superseded by the new one.

## 2. Architecture map

| Concern | Crate / file | Notes |
|---|---|---|
| Player physics | `umicraft-game/src/physics.rs` (1927 lines, ~960 code) | `step`, `travel`, `move_flying`, `move_player`; f64 maths |
| Collision | `umicraft-game/src/collision.rs`, `shapes.rs` | Port of `Entity.moveEntity` incl. step-up and sneak edge |
| Input to intent | `umicraft-game/src/input.rs` (`RawInput::to_intent`) | sneak scale; 0.98 damping applied in `physics::step` |
| Prediction / setback | `umicraft-game/src/lib.rs` (`GameState`, `teleport`, `apply_correction`), `prediction.rs` | |
| Tick packet rules | `umicraft-game/src/traffic.rs` (`ServerView`) | port of `onUpdateWalkingPlayer` |
| Game loop, 20 TPS | `umicraft-client/src/game.rs` `run` (l.1262), `tick` (l.3454) | own thread; `TICK = 50 ms` (l.120) |
| Mouse | `umicraft-client/src/window/mod.rs:2094` `device_event` → `InputUpdate` → `game.rs:3085` `handle_input` | |
| Command queue | `umicraft-client/src/game.rs:1195` `CommandQueue`; `network/command.rs` (`ClientCommand`) | |
| Socket thread | `umicraft-client/src/network/thread.rs`; `umicraft-net/src/play/drive.rs` | tokio current-thread, `TCP_NODELAY` on |
| Packets | `umicraft-protocol/src/packets/play/serverbound/*` | 142 files in the crate |
| Render | `umicraft-render` (wgpu, DX12/Vulkan), `frame_pacing.rs` untracked | separate thread, not on the 20 TPS path |
| Parity tooling | `umicraft-testkit`: `packet_logger`, `trace_diff`, `trajectory_diff`, `movement_probe`; `docs/parity-harness.md` | needs a recorded **vanilla** session |

Crates (files / lines): client 220 / 67k, game 219 / 59k, protocol 142 / 18k, ui 139 / 28k, render 111 / 29k,
model 52 / 21k, assets 33 / 12k, mesh 38 / 8k, testkit 19 / 5k, net 12 / 3.6k, audio 22 / 3.6k, world 15 / 3.2k,
builtin-mods 28 / 3.2k, auth 21 / 2.9k, core 13 / 2.8k.
Threads: window/render thread, game thread (physics + packets), network thread, chunk/mesh workers.

## 3. Confirmed findings

| ID | Finding | Evidence |
|---|---|---|
| A1 | The yaw sent on the wire is wrapped to [0, 360); vanilla's `rotationYaw` is never wrapped by the mouse path. | `game.rs:3087` `rem_euclid(TAU)`, `game.rs:3676`; `Entity.java:379-387` (no wrap; only the protected `setRotation` at `:358` does `% 360`). See `rotation_parity.md`. |
| A2 | `CommandQueue` keeps **one** slot for movement packets and up to 3 catch-up ticks run before one `flush`; an earlier tick's movement packet is overwritten and never sent, while `ServerView` has already recorded it as sent. | `game.rs:1195-1240` (`position` slot), `:1314-1336` (catch-up loop, one `flush` after), `:3688-3694`. Vanilla sends exactly one movement packet per tick (`EntityPlayerSP.java:233-251`). |
| A3 | Hotbar select (C09), item drop (C07) and pick-block are queued in `handle_input` and flushed in the same loop pass, i.e. at the **event time**, not at the start of the next tick. Attack and use are tick-aligned. | `game.rs:1296-1302`, `:1354` (`flush`), `:3090-3112`, `:3346-3354`; tick-aligned: `:3466-3467`. Vanilla: hotbar `Minecraft.java:2052-2060` and drop `:2080-2084` run inside `runTick`; `syncCurrentPlayItem` (C09) runs from `updateController` at `:1736`. |
| A4 | `ServerView.walking_tick` reproduces `onUpdateWalkingPlayer`: order sprint, sneak, one movement packet; 0.03 block / 20 tick rule; look on any change. | `traffic.rs:211-262` against `EntityPlayerSP.java:189-274`. |
| A5 | Movement trig uses exact `f32::sin_cos`; vanilla uses `MathHelper.SIN_TABLE` (65536 entries, index `(int)(x*10430.378F)&65535`). | `physics.rs:452,860`; `MathHelper.java:30-40,548`. |
| A6 | Pitch is clamped to ±89°, vanilla ±90°. Mouse base speed is 0.0022 rad (0.126°) per count, vanilla 0.15° at 100 %. | `game.rs:192`, `window/mod.rs:103`; `Entity.java:385`, `EntityRenderer.java:1096-1120`. |
| A7 | `Entity.moveEntity` is ported faithfully in structure: Y then X then Z, step-up high and low path, "travels further wins", revert rule, `onGround = collidedVertically && dy < 0`. | `collision.rs:97-255` against `Entity.java:686-810`. |
| A8 | Server placement (S08): per-axis motion zeroing, acknowledgement with `onGround=false`, answered immediately. | `lib.rs:355-369`, `game.rs:3054-3081` against `NetHandlerPlayClient.java:669-717`. |

## 4. Likely causes (not proven)

- **L1: A1 produces the `AimModulo360` alerts.** Vanilla never emits a ±360° jump between two consecutive
  rotation packets; Umicraft does whenever the yaw crosses 0°/360°. The alert name fits, but the check's
  logic was not read.
- **L2: A3 produces the `Post` alert, with the `held item change` packet named in the alert.** A packet that
  vanilla sends in the same instant as the tick's movement packet arrives up to 50 ms away from it.
- **L3: A2 produces part of the `Simulation` flags and the rollbacks.** When the server receives one position
  covering two ticks of motion, or none, its prediction no longer matches. A catch-up tick needs the game
  thread to be late by more than 50 ms.

## 5. Unverified hypotheses

- The reported stair rollbacks: shapes of stairs, inner and outer corners and slabs were **not** compared with
  `BlockStairs.java` / `BlockSlab.java` yet (commit `5edd3f2` fixed corner shapes; `shapes.rs` has further
  uncommitted edits).
- The sine table and float-versus-double intermediates (A5) contribute to the `Simulation` offset. Estimated
  size is small (about 5e-5 relative per tick for the table) and was not measured.
- Timer jitter on Windows (`crossbeam select!` timeout) shifts the tick cadence.
- Whether the Minemen sanction has anything to do with movement.

## 6. Corrections applied

None. Proposed order (one fix at a time, each with a regression test): A1, A2, A3, then A5, A6.

## 7. Tests

Executed (baseline, before any change, on the uncommitted tree):
`cargo test -p umicraft-game -p umicraft-protocol -p umicraft-net -p umicraft-client --no-fail-fast` →
**exit 0, 0 failed**: about 3,100 tests passed (largest suites 1384, 1078 with 27 ignored, 283, 114), plus doc tests.
A green suite does not cover A1-A3: no existing test asserts a yaw continuity, a movement packet per catch-up
tick, or the instant of C09.

Not executed: `cargo check --workspace`, `cargo clippy --workspace`, the other crates' tests (render, ui, model,
mesh, assets, audio, auth, testkit, builtin-mods, core, world), any private PandaSpigot/GrimAC session, any
Java-versus-Rust numerical comparison.

## 8. Remaining limitations

The Java tree has no runnable harness here: `src` is the decompiled MCP source and was not compiled. Same-input
comparison therefore needs either a vanilla recording (`docs/parity-harness.md`) or a Java harness built from
`src` plus the client jar and LWJGL, which is not verified to be possible. No vanilla recording was found in the
repository. The diagnostics ring buffer (phase 7) does not exist yet; `client/src/metrics.rs` (untracked) holds
counters only and was not read in detail.
