# ADR-0017 — Movement through water, lava, ladders, cobwebs and soul sand

- Status: Implemented (code removed by mistake in `05f0cc1`, restored on 2026-10-28, see ADR-0022)
- Date: 2026-10-24
- Deciders: project owner
- Supersedes: none
- Related: ADR-0005 (game thread, client-authoritative movement), spec GAM-1

## Context

The client predicted every tick with the land rules only. In water a player
fell straight to the bottom, a ladder could not be climbed, cobwebs and soul
sand did not slow anyone down. The 1.8 server accepts the client's position
(ADR-0005), so the other players saw the same wrong movement; GAM-1 asks for
the vanilla movement rules.

## Decision

1. **The tick follows the reference order.** `Entity.onEntityUpdate` first:
   the water check of `handleWaterMovement`, which sets `in_water` and pushes
   the player along the current (`BlockLiquid.getFlowVector`, 0.014 per tick).
   Then the living update: sprint, push out of blocks, the motion epsilon, the
   jump key (in water or lava it swims up by 0.04 instead of jumping), and the
   travel branch of `moveEntityWithHeading`.
2. **One travel branch per medium.** Water: acceleration 0.02, friction 0.8,
   gravity 0.02. Lava: 0.02, 0.5, 0.02. Both lift a swimmer blocked by a free,
   dry bank with 0.3. Land: the slipperiness friction (fix `8de3418`),
   plus the ladder rules (speed capped at 0.15, a sneaking player
   holds on, walking into the wall climbs at 0.2).
3. **Blocks act after the move** (`Entity.doBlockCollisions`): a cobweb scales
   the next move by `(0.25, 0.05, 0.25)` and clears the motion; every soul sand
   block overlapped multiplies the horizontal motion by 0.4.
4. **The media live in `umicraft_game::media`**: the queries read the blocks at
   the positions the reference reads them, with its quirks (the water depth
   test compares the scan bound, not the box top; a single tick may apply soul
   sand twice when the box straddles two blocks).
5. **Enchantments and potions are hooks, not features.** `Player.depth_strider`
   is read by the water branch and stays 0 until the inventory (M9) sets it;
   speed and jump potions wait for the entity effects (M8).

## Consequences

- Swimming, climbing and the slow blocks match the reference formulas; each
  medium has unit tests against them, and the `course` headless script
  crosses a built course (water, soul sand, cobwebs, a ladder) on a live
  server without a resync.
- Depth Strider, Speed, Slowness and Jump Boost are known gaps until M8/M9.
- Creative and spectator flight are out of scope (a later item).
