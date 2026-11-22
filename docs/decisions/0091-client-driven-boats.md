# ADR-0091 — The client drives the boat the local player boards

**Status:** accepted
**Date:** 2026-11-22
**Milestone:** vehicles fix (follow-up of ADR-0027)

## Context

ADR-0027 left every vehicle to the server: the client only followed the
tracked entity. A boat then answered the keys a round trip late and jerked
between the sparse boat updates. The 1.8.9 client does not work that way
(`EntityBoat.onUpdate`, `setPositionAndRotation2`,
`NetHandlerPlayClient.handleEntityAttach`). Minecarts and empty boats also
glided over the 3 ticks of every other entity, where their classes use 5 and 8.

## Decision

1. **Glides.** A minecart reaches a reported position in 5 ticks, a boat in 8;
   a boat then drifts on with the server velocity, halved on the ground and
   damped by `0.99/0.95/0.99` (`umicraft_game::entity::boat::BoatMotion::drift`).
2. **Driven boats.** Boarding a boat (`Attach Entity` of the local player)
   marks it driven, as `setIsBoatEmpty(false)`. The game tick runs
   `EntityStore::drive_boats` before seating the rider: buoyancy by fifths of
   the height in water, the rider's keys along its yaw times a speed multiplier
   (0.07 to 0.35), a 0.35 top speed, block collisions through `move_entity`
   and a turn of at most 20 degrees a tick towards the course.
3. **Server updates.** A driven boat ignores a relative move or teleport within
   a block of its own position and only takes the server velocity for a
   farther one. A teleport of a ridden boat (by anyone) puts it there at once.
   The boat stays driven after the player leaves it, until another entity
   boards it (`setIsBoatEmpty(true)`), exactly like the reference.

## Consequences

- No serverbound change: the 1.8 client never sends the boat position, only
  `Steer Vehicle` and its look, so the anti-cheat parity is untouched.
- Left out: splash particles, the snow and lily pads the boat breaks locally
  (the server's block change follows), the bounce on slime and beds, cobwebs,
  and two driven boats bumping into each other. The turn uses the exact
  `atan2` instead of `MathHelper`'s table.
