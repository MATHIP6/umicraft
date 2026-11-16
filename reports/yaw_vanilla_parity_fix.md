# Yaw vanilla parity fix (phase 1)

Date: 2026-11-11. Scope: the one confirmed divergence A1/R1 of `vanilla_parity_audit.md` and
`rotation_parity.md`. Nothing else was changed. **This fix is not claimed to resolve the Minemen ban or any
GrimAC alert**: it removes one confirmed difference from vanilla, and its effect on any server is untested.

## Original Rust behaviour

`crates/umicraft-client/src/game.rs`, `handle_input`:

```rust
game.yaw = (game.yaw + update.look_yaw).rem_euclid(std::f32::consts::TAU);
```

`game.yaw` (radians, `f32`) was wrapped into [0, 2π) at every mouse update. Report time converts it with
`game.yaw.to_degrees()` (`report_movement`, `TickSnapshot.yaw`), so the yaw on the wire was always in [0, 360).
A player turning right through north therefore sent a step of about -360° between two consecutive rotation
packets.

## Java 1.8.9 reference

- `Entity.java:379-387` `setAngles`: `rotationYaw = (float)((double)rotationYaw + (double)yaw * 0.15D)`. No wrap.
- `EntityPlayerSP.java:228-229,237,245`: the packet carries `rotationYaw` as is; the look is "changed" when
  `rotationYaw - lastReportedYaw != 0`.
- The only `% 360.0F` on `rotationYaw` in the searched client sources is the protected `Entity.setRotation`
  (`Entity.java:358`), which the mouse path does not call.
- `NetHandlerPlayClient.handlePlayerPosLook` (`:669-717`) stores the server's yaw unwrapped and acknowledges
  with it, which Umicraft already did (`apply_placement` keeps the raw value).

## Modified lines

Only `crates/umicraft-client/src/game.rs`, `handle_input` (about line 3087 of the working tree):

```diff
-    game.yaw = (game.yaw + update.look_yaw).rem_euclid(std::f32::consts::TAU);
+    // Continuous, never wrapped: MCP `Entity.setAngles` adds to `rotationYaw`
+    // and the packet carries it as is, so 359.9 turns into 360.2, not 0.2.
+    // The consumers of the yaw only use its sine and cosine or wrap the
+    // difference of two angles.
+    game.yaw += update.look_yaw;
```

Plus 8 tests appended after `the_input_update_turns_the_view` in the same file (`tests` module). No other file
was modified by this phase; `git status` still lists the same 113 paths. No destructive Git command was used.

## The four representations

| Representation | Where | After the fix |
|---|---|---|
| Internal player yaw / network yaw | `game.yaw` → physics intent, `TickSnapshot` | continuous radians, converted to degrees at the wire, like vanilla's float degrees |
| Rendered camera yaw | render thread `WorldView.player_yaw`, `apply_look_delta` | **unchanged**, still wrapped locally (`world_view.rs:1212`); the camera pose is never interpolated between two yaws (`current.yaw` is used as is), so a wrapped copy and a continuous one never meet in a lerp |
| Head and body rotation | `EntityRecord::follow` → `BodyYaw`, `angle::wrap_degrees` | unchanged; differences are already wrapped with `wrap_degrees` (`body_yaw.rs:76-80,114`); the entity lerp is `lerp_degrees` (`angle.rs:25`) |
| Hand sway | `hand.rs:106` | unchanged; `view_yaw += wrap_degrees(yaw - view_yaw)` |

Other readers of `game.yaw` (`look_direction`, `camera_pose` for F5, physics `sin_cos`, the leash frame) use only
the sine and cosine of the angle. Nothing else was normalised or un-normalised.

## Numerical examples

Measured with the new tests on the old line and on the new one (wire value = `PlayerLook` yaw bytes decoded).
Only the last row is a printed measurement of the old code; the others are the consequence of the old `rem_euclid`
and are asserted by the tests that failed on the old code.

| Start | Turn | Before (old code) | After |
|---|---|---|---|
| 359.9° | +0.3° | about 0.2° (a -359.7° step) | 360.2° |
| 359.0° | +1.0° | about 0.0° | 360.0° |
| 0.0° | -0.2° | 359.8° | -0.2° |
| 0.0° | 8 × +100° | wraps at 360 | 100 … 800° |
| 350.0° | 50 updates of 7.3° in one tick | 355.0° | 715.0° |
| 359.998° | +0.5° | `359.99835 -> 0.4983306` (printed) | `359.99835 -> 360.49835` |

Before the fix five of the new wire tests fail (checked by putting the old line back for one run, then removing
it again); after it all pass.

## Tests

Added (all in `crates/umicraft-client/src/game.rs`):

- `the_wire_yaw_crosses_360_without_wrapping`: 359.9 → 360.2 and 359 → 360.
- `the_wire_yaw_goes_below_zero_without_wrapping`: 0 → -0.2, then -720.
- `several_turns_in_a_row_keep_adding_up`: eight 100° turns, beyond 720°.
- `rapid_mouse_movements_between_ticks_add_up`: 50 updates between two ticks.
- `consecutive_wire_yaws_never_jump_by_a_turn`: 1440 steps of +0.5° (two circles), then 1440 of -0.5° back
  through zero; every wire step stays 0.5°.
- Network serialization: every wire test reads the yaw back from the encoded `PlayerLook` bytes
  (`EncodablePacket::encode_body`) after a full `tick` → `report_movement` → `CommandQueue`.
- `a_continuous_yaw_looks_the_same_as_the_wrapped_one`: `look_direction` (raycast, F5 orbit) for ±k turns.
- `the_local_body_and_the_head_follow_a_continuous_yaw`: head and body yaw of the local body equal modulo 360 for
  the wrapped and the continuous input.

Executed:

- `cargo check -p umicraft-client -p umicraft-game`: finished, no error.
- `cargo test -p umicraft-game -p umicraft-client -p umicraft-net -p umicraft-protocol --no-fail-fast`: every
  suite passes, 0 failed (client lib 1085 passed, 27 ignored; 1078 before, plus the new tests). The baseline
  before the change was also 0 failed.

Not executed: `cargo check`/`test` of the other workspace crates (render, ui, model, mesh, assets, audio, auth,
testkit), `cargo clippy` at all, any visual check of the renderer, F5, third person and head in a running
window (the reasoning above is from the code and the unit tests), any private PandaSpigot/GrimAC session, any
vanilla capture comparison.

## Remaining limitations

- Floating-point range: the yaw is no longer reduced, so a very long session of turning in one direction grows
  the `f32`. Vanilla has the same property in degrees. The resolution stays far below one mouse step for
  hundreds of turns (about 6e-5 rad at 100 turns against 2.2e-3 rad per mouse count); not wrapped on purpose.
- Pitch (±89 against ±90), the mouse speed (0.126° against 0.15° per count), the sine table and the `f32`
  radians representation (R2, R3, R4, A5) are **not** changed and remain open.
- A server placement (`apply_placement`) already installed the server's raw yaw; after it the mouse continues
  from that value, which is the vanilla behaviour.
- The render thread's own copy is still wrapped locally; the two copies agree modulo 2π, not numerically.
- Whether this removes any alert on a server is unknown until the 60 s script is run against a private server
  and compared with a vanilla recording (`docs/parity-harness.md`).

Stopped here as requested. Movement packet scheduling (A2) and the inventory packet order (A3) are untouched.
