# Mob animations and mob particles: audit and plan

Status: **phase A done** (branch `feat/mob-animations`). Audited against MCP 1.8
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

## Phase B: render transforms (small, no new state)

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

None are implemented. The engine (`umicraft-game/src/particles`) already has
almost every type. Hook points: a per-tick `entity_particles(game)` pass next
to `spawn_ambient_particles` (`umicraft-client/src/game.rs`), and the
`PlayEvent::EntityStatus` arm for the status-driven effects.

- P1 Every tick: blaze (2 large smoke), enderman and endermite (2 portal),
  wither (smoke per head, spell when armoured or spawning), iron golem
  (block crack under its feet while walking), guardian (bubbles while
  swimming, and along its beam).
- P2 Statuses: 20 spawner (20 explode), 18 animal in love (7 hearts), 12/13/14
  villager, 6/7 taming (wolf, ocelot, horse), 15 witch (witch magic), 1 rabbit
  (running particles), 8 wolf (water splash while shaking, with C3).
- P3 Death poof: 20 explode when `deathTime` reaches 20 (`onDeathUpdate`).
- P4 Potion swirls: `potion_color` is parsed, plus the ambient flag
  (metadata 8).
- P5 Slime and magma cube landing: needs a `Slime` factory (missing) and the
  `wasOnGround` state.
- P6 Dragon: large and huge explosions while it dies.
- Missing factories: `Slime`, `ItemCrack`, `MobAppearance` (the elder guardian
  appearing, game state 10).

## Phase D: outside this task

- D1 The armor stand is never drawn. `ArmorStandMeta` exists, but there is no
  model or rendering.
