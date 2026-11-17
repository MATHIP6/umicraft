# 0079. 1.7 Animations built-in mod

Status: accepted

## Context

The owner wants a built-in mod (ADR-0065) that brings back the animations and
visuals of the 1.7 client. On 2026-11-17 the owner directed that the local MCP
1.7.10 sources (`../1.7.10-mcp`, outside the repository) be read for it, under
the same terms as the 1.8 sources (ADR-0001): reference only, nothing copied
into the repository.

Each candidate was checked by reading both `1.7.10-mcp` and `1.8.9-mcp`. Several
"1.7" effects turned out to be the same in both versions and need no port: the
view bobbing (`EntityRenderer.setupViewBobbing`), the hurt camera
(`hurtCameraEffect`), the field of view (`getFOVMultiplier` /
`getFovModifier`), the eat and drink motion and the arm swing curves of the
first-person hand.

## Decision

A visual mod `legacy_animations` ("1.7 Animations", off by default) with one
toggle per real difference, all on by default:

- `block_hit`: an item in use (sword raised, food, drawn bow) keeps the swing
  rotations (1.7.10 `ItemRenderer.renderItemInFirstPerson` applies them in
  every branch, 1.8.9 passes 0 while the item is in use). Holding attack at a
  block while an item is in use swings the arm (1.7.10
  `Minecraft.func_147115_a` has no `isUsingItem` check).
- `equip`: the hand dips only when the item or its damage changes (1.7.10
  `updateEquippedItem`), not its count or tags (1.8.9 `getIsItemStackEqual`).
- `sneak_camera`: the view glides down and back up when sneaking, from the
  `ySize` of 1.7.10 (`EntityPlayerSP.onLivingUpdate` sets it to 0.2,
  `Entity.moveEntity` multiplies it by 0.4 each move).

The settings flow as plain data: the mod holds the toggles,
`ModsRuntime::legacy_animations` reads them into
`umicraft_game::legacy::LegacyAnimations`, and the window sends it to the game
thread (`GameEvent::LegacyAnimations`) when a session starts and after each
change in the mods menu. The game thread owns the effects: `HandState` (equip
rule, `HandPose::swing_while_using`), the attack tick (block hit swing) and
`SneakCamera` (eye correction of the published view).

**Nothing the server sees changes.** The block hit swing runs the local hand
and body animation only, with no swing or dig packet; the sneak camera moves
the rendered view only, not the eye used for picking. The anti-cheat parity of
ADR-0076 holds with the mod on. Other players do not see the block hit, unlike
on a real 1.7 client, which sent the swing.

## Consequences

- Left for later, with the reason:
  - Third-person blocking without the 30 degree arm yaw of 1.8.9
    `ModelBiped` (`heldItemRight == 3`) and the sneaking body without the 0.2
    lowering of 1.8.9 `ModelBiped.render`: the humanoid pose is built from
    `EntityRenderSnapshot` in about ten places of the entity renderer, which
    have no way to see a client setting yet. That needs a render style
    passed through the entity meshing, a change of its own.
  - The 1.7 position of flat items in the hand (`ItemRenderer.renderItem`
    with its fixed 2D transform): 1.8 places them with the first-person
    display transform of the item model, which comes from the resource pack.
    Replacing it needs a check against the vanilla models in a real window.
- In adventure mode the block hit never swings; 1.7 swung there with the
  right tool for the block.
