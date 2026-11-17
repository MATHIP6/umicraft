# ADR-0076 — Vanilla angles and float math for the local player

Status: proposed (2026-11-17)

## Context

A comparison with another 1.8.9 client written in Rust
(`~/repo/Minecraft-1.8.9-Rust`, which servers' anti-cheats do not flag)
showed that Umicraft's movement logic and packet order match vanilla, but its
arithmetic does not:

- **Angles in radians.** `Game::yaw`/`pitch` and `Player::yaw`/`pitch` are
  radians. The mouse adds `dx * 0.15° in radians * f1` to an `f32` radian
  value and every look packet sends `to_degrees()` of it. Vanilla keeps
  `rotationYaw` in degrees (`Entity.setAngles`:
  `yaw = (float)((double)yaw + (double)(dx * f1) * 0.15D)`). The deltas
  Umicraft sends are three times further from a multiple of the sensitivity
  step than vanilla's (simulation, 1.5e-5 against 5e-6 steps), and a server
  yaw such as `-123.4567` comes back as `-123.45669555`.
- **Exact trigonometry.** `moveFlying` and the sprint jump use
  `f32::sin_cos`. Vanilla uses `MathHelper.sin/cos`, a 65 536-entry `float`
  table indexed by `(int)(x * 10430.378F) & 0xFFFF` (`Entity.java:1240`,
  `EntityLivingBase.jump`). A 40-tick sprint simulation at yaw 37°, 123° and
  -71° drifts by 1e-5 to 3.6e-5 block per tick and up to 7.5e-4 block in
  total (none at yaw 0). That is below the usual flag threshold of
  prediction anti-cheats, but not exact, and it moves the 0.03-block
  threshold that decides whether a position packet is sent.
- **`f64` where vanilla uses `float`.** `0.42` instead of `0.42F`, `0.98`
  instead of `0.9800000190734863`, `0.91`/`0.546` instead of
  `0.91F`/`0.54600006`, the ground acceleration
  `0.16277136F / (f4 * f4 * f4)` and `moveFlying` computed in `f64`, the
  movement speed not cast to `float` (`getAIMoveSpeed`). Each is ~1e-8 a
  tick.
- **Fractional mouse deltas.** `compute_mouse_look_delta` takes winit's `f64`
  as is. LWJGL's `Mouse.getDX()` is an integer, so every vanilla rotation is
  a whole number of sensitivity steps; on Wayland or with high-resolution
  mice winit reports fractions.

## Decision

1. **Degrees on the game side.** `Game::yaw`/`pitch`, `InputUpdate::look_*`,
   `Player::yaw`/`pitch`, `TickSnapshot` and `ServerCorrection` carry
   `f32` degrees, unwrapped, exactly as `rotationYaw`/`rotationPitch`. The
   conversion to radians happens once, where the render thread builds the
   camera (`RenderState`, `world_view`). Packets send the field unchanged.
2. **`umicraft_game::mth`** holds `sin`, `cos` (the `MathHelper` table, built
   once in a `OnceLock<Box<[f32; 65536]>>`), `wrap_degrees` and
   `sqrt_float`. Physics, the sprint jump and every other place where
   vanilla calls `MathHelper` use it; the renderer keeps exact trigonometry.
3. **`float` where vanilla uses `float`.** `physics.rs` follows the
   reference's types: friction, acceleration and `moveFlying` in `f32`,
   widened to `f64` exactly where the reference casts; the constants become
   the `float` literals widened (`0.42_f32 as f64`, `0.98_f32 as f64`).
4. **Whole mouse steps.** The window accumulates the raw deltas and hands the
   game only whole pixels, keeping the fraction for the next event; the
   rotation is then `(float)((double)yaw + (double)(px * f1) * 0.15D)`.
   The zoom factor multiplies `f1`, as OptiFine's zoom changes the
   sensitivity.

## Consequences

- Every consumer of `Game::yaw` changes unit; the grep for `to_degrees()` and
  `to_radians()` on these fields is the checklist. The render thread's
  `RenderState` keeps radians, so the renderer does not change.
- Mods that read the look (`mods_runtime`: `yaw_radians`) keep their API; the
  conversion moves into the bridge.
- The table costs 256 KiB once. `physics::step` is a hot path: a criterion
  benchmark of `step` before and after goes into `docs/perf-log.md`.

## Acceptance

- Golden tests: 40 ticks of walking, sprinting, sprint-jumping and swimming
  at yaws 0°, 37.35°, 123.45°, -71.1° reproduce positions computed with Java
  `float` semantics and the `MathHelper` table, to the bit.
- A server yaw is echoed bit for bit after a teleport.
- Mouse deltas of `0.4`, `0.4`, `0.4` give one step of rotation, not 1.2.
- `cargo fmt`, `clippy -D warnings`, the workspace tests and the benchmark.

## Work split

Phase A (`umicraft-game`): `mth`, degrees in `Player`/`physics`/`traffic`,
golden tests. Phase B (`umicraft-client`): `Game`, input, mouse
accumulation, render conversion. A is merged before B starts.
