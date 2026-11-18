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
- `item_position`: a flat sprite in the first-person hand takes the fixed
  placement of 1.7.10 `ItemRenderer.renderItem` (2D branch) instead of the
  `firstperson` pose of its 1.8 model
  (`umicraft_model::item::pose::legacy_flat_held_pose`). With the vanilla
  sword model the two differ little at rest but clearly while blocking: the
  1.7 sword is about 13 % smaller, further left and more upright. The fishing
  rod and the carrot on a stick turn half around first
  (`shouldRotateAroundWhenRendering`), and a drawn bow stretches along the
  depth of its sprite as 1.7 did. It applies to every flat item, not only
  while blocking, so that the sword does not jump when the block starts.
  Blocks are placed alike by both versions and keep their model pose.
- `third_person`: the players seen from outside take the 1.7 pose. A
  blocking arm keeps a yaw of 0 (1.7.10 `ModelBiped.setRotationAngles`, where
  1.8.9 turns it by -30 degrees), and a flat item in a player's hand takes
  the fixed placement of 1.7.10 `RenderPlayer.renderEquippedItems` (bow,
  handle-like items with the turn across the body while blocking, other
  items) before the 1.7 sprite chain, in place of the model's `thirdperson`
  pose. The renderer reads it from `ModsRuntime::biped_style` each frame
  (`EntityTextures::set_biped_style`, `WorldItems::set_biped_style`) and
  passes `umicraft_model::entity::BipedStyle` down to the humanoid pose; the
  game thread does not need it. Mobs, blocks and torches keep the 1.8 look.

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
  - The sneaking body without the 0.2 lowering of 1.8.9 `ModelBiped.render`
    (and the matching lift of the held item): `BipedStyle` now reaches the
    pose, so it is a small follow-up.
  - A fishing player shows a stick in 1.7 (`RenderPlayer`, line 303); the
    1.8 rod is kept.
  - With `item_position` on, a resource pack's own `firstperson` pose of a
    flat item is not used, as 1.7 had no such pose.
- In adventure mode the block hit never swings; 1.7 swung there with the
  right tool for the block.
