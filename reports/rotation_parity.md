# Rotation parity: Umicraft against Java 1.8.9

Status: **initial read-only audit.** No code changed, nothing run against a server. Date: 2026-11-11.
This report supersedes the earlier partial version: the reference formulas are now read in `src`, not quoted
from memory. It makes no claim about any server or anti-cheat sanction. Overview: `vanilla_parity_audit.md`.

## Chain in Umicraft

| Step | Where | What it does |
|---|---|---|
| Raw motion | `window/mod.rs:2094-2120` | winit `MouseMotion` delta (f64) times `0.0022 * look_factor * zoom` added to `self.look` (radians, f32) |
| Apply | `game.rs:3085-3088` | `yaw = (yaw + dyaw).rem_euclid(TAU)`, `pitch = clamp(pitch + dpitch, ±MAX_PITCH)` |
| Pitch limit | `game.rs:192` | 89° |
| Physics | `physics.rs:399,452,860` | uses `intent.yaw` (radians) with exact `sin_cos` |
| Wire | `game.rs:3676-3677` | `yaw.to_degrees()`, `pitch.to_degrees()` of the wrapped value |
| Packet choice | `traffic.rs:229-252` | look when `(yaw, pitch)` differs from the last sent pair |

## Reference in Java 1.8.9 (read in `src`)

- `EntityRenderer.java:1096-1120`: `f1 = f*f*f*8` with `f = sens*0.6+0.2`; `setAngles(deltaX*f1, deltaY*f1*±1)`;
  mouse deltas are **integers** (`MouseHelper`).
- `Entity.java:379-387` `setAngles`: `yaw += dx*0.15`, `pitch -= dy*0.15`, pitch clamped to **[-90, 90]**,
  `prevRotationYaw += delta`. **There is no wrap of the yaw.**
- `EntityPlayerSP.java:228-229,245`: the packet carries `rotationYaw` as is; the change test is
  `rotationYaw - lastReportedYaw != 0`.
- The only `% 360` on `rotationYaw` in the searched client sources is the protected `Entity.setRotation`
  (`Entity.java:358`), not used by the mouse path.

## Divergences

| # | Java 1.8.9 | Umicraft | Status |
|---|---|---|---|
| R1 | Yaw is an unbounded float in degrees; a player turning right sends 359.9, 360.2, 360.5 … | Wrapped to [0, 360): 359.9, 0.2, 0.5 … The wire shows a **-360° step** at each crossing, which vanilla never produces. After a server placement `game.yaw` keeps the server's raw value until the next mouse input wraps it. | **Confirmed** in code on both sides. Effect on the server: **Likely** cause of `AimModulo360` (the alert's check was not read). |
| R2 | Pitch limit ±90 | ±89 | **Confirmed.** Effect on a server: unverified. Visible: cannot look straight up or down. |
| R3 | 0.15° per count at 100 % sensitivity, integer counts | 0.126° per count (`0.0022` rad), float deltas from winit | **Confirmed** constant. About 16 % slower than vanilla at the same slider. Changes feel only. Needs the owner's decision. |
| R4 | Angles are `float` degrees end to end | `f32` radians, converted at the wire, wrapped | **Confirmed** representation. Rounding differences of the last bits are expected; effect unmeasured. A server that estimates the mouse sensitivity from rotation steps sees steps that are no longer exact multiples of `0.15*f1` in degrees. **Unverified.** |
| R5 | No zoom | Zoom mod scales the look speed | Documented mod, no action. |

## Not checked yet

- Camera yaw versus player yaw in the render thread (`LookSlot`, `game.rs:1280`) and the interpolation between
  ticks.
- Head yaw and body yaw for the third-person model (`entity/body_yaw.rs`, uncommitted edits).
- Whether any frame-time based smoothing changes the delta per frame.

## Smallest candidate fix (not applied)

Keep the render and physics representation. For the wire, keep a running yaw in degrees (`f32`) that
accumulates the same delta as the mouse and is never wrapped; send that, and let a server placement replace it
with the server's raw value. Regression test: cross 359° to 0° and 0° to 359° and assert every consecutive
difference between two sent yaws is below 180°. Then R2 (constant 90°, test at exactly 90° including the look
vector at the pole), then R3 only if the owner wants the vanilla feel.

## Tests

Executed: none for rotation. Not executed: the wrap test, a Java comparison, a Grim run.
