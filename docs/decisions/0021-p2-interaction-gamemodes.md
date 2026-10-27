# ADR-0021 — P2: interaction rules, gamemodes and spectator camera

**Status:** accepted
**Date:** 2026-10-27
**Milestone:** P2 (ROADMAP.md)

## Context

The dig / place / acknowledge rules were free functions over the private
`Game` of `umicraft-client`, untested, and the client knew a single gamemode
bit (`1` is creative). P2 asks for the rules in `umicraft-game`, the four
gamemodes and the spectator camera.

## Decision

1. `umicraft_game::interaction::Interaction` owns the dig state, the awaited
   confirmations and the position acknowledgement. Its methods take the
   inputs (gamemode, ray-cast hit, tick, held item) and return an
   `Outgoing` packet; the client maps it onto `ClientCommand` and keeps the
   channels only. The crate still has no networking: `Outgoing` uses the
   protocol types only.
2. `umicraft_game::gamemode::GameMode` carries the per-mode rules (reach,
   instant break, break / place permission, flight, no-clip). The mode is set
   by `Join Game`, `Respawn` and `Change Game State` reason 3.
3. Flight and no-clip live in `physics::Player` (`allow_flying`, `flying`,
   `fly_speed`, `no_clip`) so that prediction replays them. The server's
   `Player Abilities` feed `GameState::set_abilities`; a change of the flying
   state is reported with `Player Abilities` (0x13).
4. `Camera` (0x43): `EntityStore::camera_view` gives the eye of the followed
   entity; the frame uses it and the local movement is suspended.

## Consequences

- Adventure mode never starts a break because item NBT (`CanDestroy`) is not
  parsed yet (P3).
- The break is still the fixed 12-tick stand-in; hold-to-break is the next
  P2 item.
