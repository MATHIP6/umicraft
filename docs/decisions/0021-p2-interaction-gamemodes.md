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

5. Camera effects: the player state keeps the vanilla inputs of view bobbing
   and the FOV modifier; `RenderState.effects` carries the values of a tick
   and the render thread interpolates two frames. `Camera` applies them.
   F5 perspectives are resolved on the game thread (`view::camera_pose`); the
   local body is published as an entity.
6. The block selection outline is a `LineList` pass fed by `Frame.selection`
   (wgpu has no wider lines, so each edge is a quad widened in the vertex shader).

7. Death and respawn: the window opens the death screen when the published
   health is 0 and sends `GameEvent::Respawn` on the button; the game thread
   sends `Client Status` and blocks the dead player's input. The screen is
   a widget of `umicraft-ui` so that its layout and clicks are unit-tested.

## Consequences

- Adventure mode never starts a break because item NBT (`CanDestroy`) is not
  parsed yet (P3).
- The break follows the vanilla progress (hold-to-break). The block table is generated from the reference by `docs/tools/gen_block_data.py`; tools are a hand-written list of ids until the P3 item registry.
- The cracks (own ones and the 0x25 ones of other players, all in `ServerState`) are drawn over the terrain by a dedicated pipeline; the arm swing is sent but not rendered (no first-person hand yet).
- Not done: first-person hand (needs item models, P3); skin, arm swing and body-yaw lag of the local body (P5).
- The death screen's "Title Screen" button waits for the P10 title screen; hardcore variants are missing.
