# ADR-0027 — P2: riding, selection box, liquids and the held item in F5

**Status:** accepted
**Date:** 2026-10-29
**Milestone:** P2 (ROADMAP.md)

## Context

P2 still lacked riding. Three defects showed up in play: the local body drawn
in F5 had empty hands, the selection outline was the collision shape (a fence
was 1.5 tall, a flower a full cube) and the ray cast stopped at water and lava,
which made liquids selectable and diggable.

## Decision

1. **Riding.** A rider has no physics: `GameState::ride` puts it on the seat
   computed by `umicraft_game::riding::rider_feet` (MCP
   `Entity.updateRiderPosition`: boat shifted 0.4 along its yaw, `-0.3`;
   minecart `0`; spider half its height; others three quarters; minus 0.35 for
   `EntityPlayer.getYOffset`). The server moves the vehicle from `Steer Vehicle`;
   the client only follows the tracked entity. An untracked vehicle falls back to
   normal movement.
2. **Selection box.** `selection_shape` is the vanilla selected bounding box
   (`umicraft_game::selection`), not the collision shape. Collision shapes used
   by physics are unchanged.
3. **Liquids.** The ray skips liquids like `EntityLivingBase.rayTrace`
   (`stopOnLiquid = false`). A bucket needs a liquid-aware ray when P2/P3 item
   use targets liquids (the use packet is sent without a target today).
4. **Held item in F5.** `follow_body` copies the selected hotbar stack into the
   body record, since `Entity Equipment` is never applied to the local player.

## Consequences

- The horse rearing offset, horse jump and the rider's sitting pose are not done.
- Vehicle and passenger rendering belong to P5.
