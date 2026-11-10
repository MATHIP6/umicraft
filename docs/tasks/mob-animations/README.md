# Mob animations and mob particles: audit and plan

Status: **phases A, B, C and P done** (bar `MobAppearance` and `ItemCrack`) (branch `feat/mob-animations`). Audited against MCP 1.8
(`client/model/Model*.java`, `client/renderer/entity/Render*.java`, `entity/`) on
2026-11-09. Line numbers are the ones at `eba357d`.

## Conventions found during the audit

- MCP `setRotationAngles(limbSwing, limbSwingAmount, ageInTicks, headYaw,
  headPitch, scale, entity)`. `ageInTicks` (`ticksExisted + partialTicks`) is
  `EntityRenderSnapshot::age` and now `Pose::age` in `entity_render/mobs.rs`.
- Models are built with y up (`24 - y_mcp`). This turns every x and z
  rotation around (`pose.rs` `apply`: `(x, y, z) -> (-x, y, -z)`). A model that
  copies an MCP x angle without negating it is a bug unless its boxes were
  rebuilt to compensate.

## Phase A: models without new state (done)

| Step | Commit | Fix |
|---|---|---|
| A1 | `1a92304` | Blaze, bat, silverfish, endermite, guardian and wither were driven by the limb swing instead of `ageInTicks`, so they froze while hovering. Also: blaze formula (precedence, 6 px too low, head in degrees), bat (`cos` phase, head look), guardian pulse `0.01`, made-up wither side-head sway removed. |
| A2 | `e5eb1e0` | The head pitch sign was wrong for cow, pig, mooshroom, sheep, chicken, creeper and snowman, and the sheep legs were inverted. The iron golem now walks on a triangle wave. |
| A3 | `629e4be` | Enderman posed from the biped angles: idle sway, attack swing, carrying arms forward (they pointed backward). |

## Phase B: render transforms (done)

All in `entity_render/transform.rs`; the hanging bat waits for C6.

- B1 Bat: bob `cos(age * 0.3) * 0.1` in flight (`RenderBat.rotateCorpse`).
- B2 Iron golem: walking roll `6.5 * tri(limbSwing - amount * (1 - pt) + 6, 13)`
  when `amount >= 0.01` (`RenderIronGolem.rotateCorpse`), in `transform.rs`.
- B3 Zombie villager curing: body yaw `+= cos(floor(age) * 3.25) * PI * 0.25`
  (`RenderZombie.rotateCorpse`). `ZombieMeta.converting` is already parsed.
- B4 Screaming enderman: x/z jitter, gaussian with σ 0.02 (`RenderEnderman.doRender`).
- B5 Tamed ocelot: scale 0.8 (`RenderOcelot.preRenderCallback`).

## Phase C: new game-side state

The statuses come in through `EntityStore::status` (`umicraft-game/src/entity/store.rs:526`),
which today handles only 2, 3, 9 and 19.

- C1 Rabbit jump, status 1: `jumpTicks` and `jumpDuration`, then
  `func_175521_o` into `RabbitModel::set_angles`. The limbs are fixed today.
- C2 Iron golem, status 4 (attack, 10 ticks) and statuses 11/34 (flower, 400
  ticks), through `ModelIronGolem.setLivingAnimations`.
- C3 Wolf shake, status 8: `getShakeAngle` on the head, mane, body and tail,
  and the darker colour while wet. The begging head tilt should also ease in
  (`headRotationCourse`).
- C4 Sheep grazing, status 10: eat timer of 40 ticks, head pivot and angle
  (`getHeadRotationPointY` and `getHeadRotationAngleX`).
- C5 Chicken wings: port the wing state of `EntityChicken.onLivingUpdate`
  (`wingRotation`, `destPos`) and pass the `handleRotationFloat` value.
- C6 Hanging bat: metadata 16, bit 0, giving the upside-down pose of
  `ModelBat` and a translate of `-0.1`.
- C7 Guardian: spikes (metadata 16, bit 2), tail timer (`func_175471_a`),
  extension (`func_175469_o`), eye tracking its target (metadata 17) or the
  camera, body yaw and pitch from the look. The tail is a placeholder today.
- C8 Wither: target yaw and pitch of each side head (`func_82207_a` and
  `func_82210_r`). Today the side heads follow the centre head.
- C9 Magma cube: segments spread apart with `squish`
  (`ModelMagmaCube.setLivingAnimations`). `ent.squish` already exists.
- C10 Snowman: arms turn with the body (`ModelSnowMan`, pivots `cos/sin(bodyY) * 5`).
- C11 Witch: nose wiggle `sin/cos(ticks * 0.01 * (id % 10))`, and raised nose
  when it holds an item. This needs a `nose` part in `VillagerModel`.

## Phase P: client-side mob particles

Done in `umicraft_game::particles::mobs`. The client calls it once per tick
(`spawn_mob_particles`, between the entity tick and the particle tick), and
from the `Entity Status` arm (`spawn_status_particles`).

- P1 Every tick (done): blaze (2 large smoke), enderman and endermite (2
  portal), wither (smoke per head, spell when armoured or spawning), iron
  golem (block crack under its feet while it moves), magma cube landing
  (flames), potion swirls (`potion_color`, ambient flag), dying dragon (large
  explosions, plus huge ones from tick 180 to 200).
- P2 Statuses (done): 20 spawner puff, 18 animal in love, 12/13/14 villager,
  6/7 taming (wolf, ocelot, horse), 15 witch.
- P3 Death puff (done): `EntityStore::drain_deaths` hands out each body when
  its fall ends.
- P4 (done): slime landing (new `Slime` fragment on the atlas of the blocks,
  `Texture::Item`), rabbit hop dust (status 1), wolf shake spray (status 8)
  and the guardian's bubbles (with C7).
- Still to do: the elder guardian's `MobAppearance` (a whole elder guardian
  model drawn in front of the camera: a render pass of its own) and
  `ItemCrack` (needs an item id to picture lookup in the client).
- Not a bug: the enderman teleport trail comes from the server-side
  `teleportTo`, so a 1.8 client in multiplayer never sees it.
- Related but not particles: the client-side sounds of the blaze (`fire.fire`,
  1 tick in 24) and of the dragon's wings.

## Phase D: outside this task

- D1 The armor stand is never drawn. `ArmorStandMeta` exists, but there is no
  model or rendering.
