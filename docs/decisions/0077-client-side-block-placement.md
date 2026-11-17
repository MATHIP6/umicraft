# ADR-0077 — Client-side block placement

Status: accepted (2026-11-17)

## Context

On a server with a high ping, a placed block appeared a full round trip
after the click: Umicraft sent `Block Placement` and waited for the
server's `Block Change`. Bridging was impossible at a few hundred
milliseconds, since the next click had nothing to aim at yet.

The vanilla 1.8.9 client places the block itself.
`PlayerControllerMP.onPlayerRightClick` checks
`ItemBlock.canPlaceBlockOnSide`, sends the packet, then runs
`ItemStack.onItemUse` on the client world: `ItemBlock.onItemUse` (or
`ItemSlab.onItemUse`) picks the cell, checks `World.canBlockBePlaced`,
computes the state with `Block.onBlockPlaced`/`onBlockPlacedBy` and calls
`setBlockState`, then takes one item from the stack (restored in creative).
The server sends a `Block Change` for the clicked block and for the cell
beside it after every placement
(`NetHandlerPlayServer.processPlayerBlockPlacement`), so a wrong guess is
undone a round trip later. The place sound is not played locally (the client
`RenderGlobal.playSound` is empty); it comes from the server to every player
near, the placer included.

Breaking was already predicted (the block turns to air when the dig
finishes), and so were the use of an item (eating, the bow draw: `start` in
`item_use`) and the hit effects of an attack (sprint reset, critical stars).
Only the placement waited for the server.

## Decision

`umicraft_game::placement` ports `ItemBlock` / `ItemSlab.onItemUse`,
`canPlaceBlockOnSide`, `World.canBlockBePlaced` (entities in the default
state's collision box, replaceable material, the block's own
`canPlaceBlockAt`/`canPlaceBlockOnSide`) and the `onBlockPlaced` of the
common blocks, through a `Rule` per block item id. The client writes the
predicted blocks and shrinks the held stack in the tick of the click.

Block items whose rules are not ported (`Rule::Unmodelled`: saplings,
flowers, cactus, snow layers, lily pads, vines, rails, plates, buttons,
levers, tripwire hooks, double plants) and the items that place a block
without being an `ItemBlock` (doors, beds, signs, redstone dust, repeaters,
skulls, seeds, sugar cane, buckets) still wait for the server: the check
uses the default rules and nothing is predicted. They can be added rule
by rule.

The `canPlaceBlockOnSide` check now replaces the old test (a full cube
against the player's box in the cell next to the face): the cell is the
clicked block when it is replaceable (tall grass, one snow layer), the box is
the block's own collision box (a torch can be placed where the player
stands) and other entities count.

## Consequences

- At any ping the block appears at once and can be clicked on in the next
  tick, as in vanilla.
- A misprediction (an unmodelled server rule, a protected region) shows
  the block for one round trip before the server's `Block Change` removes
  it, exactly as vanilla does.
- Approximations, all corrected by the server: the entity check uses the
  union of a multi-box block's collision boxes; the anvil-on-circuits
  exception of `canBlockBePlaced` is not ported; adventure mode never
  predicts (no `CanPlaceOn` support); `isBlockNormalCube` treats unloaded
  chunks like loaded ones.
