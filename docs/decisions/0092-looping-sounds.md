# ADR-0092 — Looping sounds kept in step by a declarative list

**Status:** accepted
**Date:** 2026-11-22
**Milestone:** vehicles follow-up (minecart sounds)

## Context

The 1.8.9 client plays the rolling of minecarts as `ITickableSound`s
(`MovingSoundMinecart`, `MovingSoundMinecartRiding`): started once, looped
(`repeat`, delay 0), and given a new volume and position every tick by
`SoundManager.updateAllSounds` until `isDonePlaying`. The audio engine only had
fire-and-forget voices.

## Decision

1. **Audio crate.** A voice may carry a `loop_key`: the mixer starts it over at
   its end, `AudioBackend::adjust_loop` sets its gains and
   `AudioBackend::stop_loop` ends it (reaped as a normal voice, so its channel
   comes free). The engine keeps the loops it started (`AudioMessage::Loop`,
   `EndLoop`) and recomputes their gains each tick, since the listener moves.
   A loop starts even while silent, as the reference starts a tickable sound
   at its default volume; one that finds no channel is tried again on its next
   update.
2. **Game side, declarative.** Each tick the game thread lists the loops it
   wants (`umicraft_game::sound::looping`), sent with the frame. The window's
   `LoopSync` diffs that list against what it sent: start, update, end. A cart
   that is removed, a dismount or the end of the world (empty list) needs no
   lifecycle message of its own, and a dropped message heals on the next frame.

## Consequences

- `minecart.base` follows `clamp(speed, 0, 0.5) * 0.7`, attenuated over 16
  blocks; `minecart.inside` follows `clamp(speed, 0, 1) * 0.75` with no
  attenuation, while the local player rides that cart. The speed is the last
  velocity the server sent, as the client's `motionX/Z`.
- A streamed sound cannot loop (none of the vanilla loops is streamed).
- The first tick is not loud: the loop starts at its computed volume instead
  of the reference's default 1.0 before its first update.
