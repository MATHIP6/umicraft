# P5 — Entities: plan and architecture

Status: **P5a to P5e are done** (P5a to P5c: commits `18d3a0e` to `29010d0`;
P5d and P5e are described below). P5f onwards remain. The decision record is
`docs/decisions/0026-p5-entity-modules.md`; the MCP sources used are listed in
`docs/sources.md` (rows dated 2026-10-29). The roadmap entry is `ROADMAP.md`
§P5.

## Why a module split first

Before P5, `entity/types.rs` (524 lines) held every entity type, and
`umicraft-client/src/entity_textures.rs` (918 lines) held the texture
registry, the mesh pipeline and one 330-line `match` over every mob. Each P5
feature would have grown those files. P5a split them without changing
behaviour; every later milestone adds a module instead of extending one.

Rule: a module stays under about 400 lines, holds pure functions where
possible, and tests its maths against the MCP formulas.

## Architecture

### Game layer — `umicraft-game/src/entity/`

| Module | Owns |
|---|---|
| `kind.rs` | `EntityId`, `EntityKind`, the wire type tables, `is_living()` |
| `flags.rs` | `EntityFlags` (metadata index 0 bits, skin parts) |
| `item.rs` | `EntityItem` (held, dropped, framed stacks) |
| `record.rs` | `EntityRecord`: the mutable per-entity state |
| `snapshot.rs` | `EntityFrameData` (render-thread interpolation data), `EntityRenderSnapshot` (+ `new`) |
| `angle.rs` | `wrap_degrees`, `lerp_degrees` (vanilla `wrapAngleTo180`, `interpolateRotation`) |
| `limbs.rs` | `LimbSwing`: phase and amplitude of the walk |
| `body_yaw.rs` | `BodyYaw`: `renderYawOffset`, player and mob algorithms |
| `motion.rs` | 3-tick interpolation of moves and teleports, per-tick sequence |
| `anim.rs` | `Animation`: hurt, death, arm swing, `BedPose` |
| `store.rs` | `EntityStore`: owns the records, applies packets, ticks, snapshots |

`meta/` holds one typed decoder per family (P5d); `profile.rs`, `cape.rs` and
`nametag.rs` serve P5e. Planned: `objects.rs`, `ride.rs`.

### Client — `umicraft-client/src/entity_render/`

| Module | Owns |
|---|---|
| `mod.rs` | `mesh_entities`: snapshots to batched vertices per texture |
| `textures.rs` | texture registry (jar path + procedural fallback), GPU upload |
| `mobs.rs` | one builder per model family (`humanoid`, `animal`, `creature`) |
| `player.rs` | the posed player model, shared with held items |
| `pose.rs` | snapshot to `HumanoidPose` (arm pose from held item and use flag) |
| `transform.rs` | `model_matrix` / `body_matrix` / `x_mirror`: orientation, death fall, bed, player scale, crouch |
| `hand.rs` | first-person arm |
| `animals.rs`, `creatures.rs` | families of mobs: babies, coats, wool, collar, slime size, creeper swell |
| `variants.rs` | texture of a wolf, ocelot, rabbit, villager or horse variant |
| `skins.rs` | downloads of skins and capes, `SkinLookup` |
| `specs.rs`, `horse_coat.rs` | texture table; composed horse coats |

Planned: `layers/` (armour, held item for mobs, fire, horse gear, …),
`objects.rs`, `shadow.rs`, `lighting.rs`. Name tags are drawn by
`umicraft-client/src/nametag_draw.rs` through the UI.

### Model crate — `umicraft-model/src/entity/`

`pose.rs` holds `HumanoidPose` / `ArmPose` and `HumanoidModel::set_pose`, a
port of `ModelBiped` and `ModelZombie` `setRotationAngles`. Each new mob gets
one file.

### Render crate

`entity.wgsl` reads bit 8 of `EntityVertex::light` as the hurt tint (30 % red
before lighting). No vertex layout change.

## Conventions worth knowing (they caused real bugs)

- **Angles are vanilla angles.** Yaw 0 looks towards +Z, positive pitch looks
  down. The game layer never converts; only the renderer does.
- **Models face -Z** and are the vanilla model space with y flipped (y up,
  pivots at `24 - y`), with boxes and UVs of the vanilla layout.
  `entity_render::transform` therefore draws them with
  `rotY(180 - body_yaw)` and an x mirror (`x_mirror`), which is the vanilla
  `scale(-1, -1, 1)`. Without the mirror, head turns, limbs and skins are
  mirrored.
- **Pose maths is written with vanilla angles** and converted in one place
  (`set_pose`): a vanilla rotation `(x, y, z)` becomes `(-x, y, -z)` because
  of the y flip. Other mob models still take their angles directly; their
  head pitch was checked visually, not their other poses.
- **Attachments in the vanilla frame** (held item) hang on `body_matrix` and
  apply the mirror themselves (`item_world::hand_frame`); the item is not
  mirrored back.
- **Snapshots are the render contract.** New visual state goes on
  `EntityRecord`, through `EntityFrameData`, into `EntityRenderSnapshot`;
  the renderer never reads the store.

## Milestones

Done (P5a to P5e):

| | Content |
|---|---|
| P5a | Module split, no behaviour change |
| P5b | Vanilla limb swing, body yaw (`renderYawOffset`), 3-tick teleport, head look applied at once |
| P5c | Hurt flash and death fall (`Entity Status` 2/3), arm swing, sleeping (`Use Bed`), sneak, sword block and bow poses, entity orientation fix (`180 - yaw`), model mirror, player scale 0.9375, held item on the right hand |
| P5d | Typed metadata for every class (merged from partial updates, health 0 starts the death fall) and the variants it changes: babies (proper head and body proportions), sheep colour and shearing, wolf tame, angry and collar, ocelot, rabbit, villager professions, horse type and composed coats, wither skeleton, zombie villager, slime size, creeper swell and white flash |
| P5e | Player profiles from the tab list, skins and capes (trusted download on a worker thread, disk cache, slim arms from the profile, 64x32 conversion), skin layers from the metadata, the cape with its swing, name tags with the team rules (prefix, suffix, visibility) |

## What is left for P5

Order matters: P5f needs nothing new, P5g changes the vertex light that P5f
layers inherit, P5h reuses the snapshot and layer plumbing, P5i reuses
`BodyYaw`, `Animation` and the skin cache.

### P5f — Layers

MCP to read: `LayerBipedArmor`, `ModelBiped` armour models, `LayerArmorBase`,
`LayerHeldItem` (mobs), `LayerCreeperCharge`, `LayerSaddle` (pig),
`LayerHorseArmor`, `LayerEnderman*`, `LayerArrow`, `RenderEntity.renderEntityOnFire`,
`Entity.isInvisible`.

- **Armour (done, unverified against vanilla):** `Entity Equipment` slots 1 to 4 are stored on the record (`armor`), drawn as the two biped shells (0.5 leggings, 1.0 others) with the material textures, leather dyed plus overlay, for players, zombies, skeletons, pigmen and giants. Invisible entities draw no body and keep only their armour. The pig saddle layer (`pig_saddle.png`, model inflated by 0.5) is done. The fire overlay is done (`entity_render::fire`, billboarded quads of `fire_layer_0/1`, the camera yaw reaches it through `EntityTextures::set_view_yaw`, widths from `EntityKind::width`; the dropped item and the arrow still use the default width). Horse gear is done in the resting pose: the armour is the third layer of the composed coat, and the saddle, bridle and chests are boxes of the horse model (`HorseModel::emit_gear`); the reins, the swinging ropes, rearing and grazing are not. The mooshroom mushrooms (`entity_render::mushrooms`) and the long ears of donkeys and mules (`HorseModel::mule`) are done. Blocks on the head (`item_world::head`: the snowman pumpkin and any block item in the helmet slot of a player, zombie, skeleton or pigman), the enderman carried block and scream (`item_world::carried`), the sitting wolf and ocelot (ports of `ModelWolf` and `ModelOcelot`, with the sneak and sprint poses of the cat and the health-driven tail of a tamed wolf) and the ghast (4.5 scale, reference tentacle lengths, shooting texture) are done. The additive pass is done (`EntityRenderer::render_additive` and `render_glint`, `entity_render::glow`): the eyes of spiders, endermen and the dragon, the charged creeper, the wounded wither (now at its scale of 2) and the enchantment glint of armour, drawn only where the armour is. Skulls on heads are done (`entity_render::skull`: the five types on players, zombies, skeletons and pigmen, the owner skin of a player skull is not fetched). Arrows stuck in a player are done (`entity_render::arrows`, drawn with the exact `new Random(entityId)` sequence of `LayerArrow` over the 16 or 17 boxes of `ModelPlayer`). The spectator ghost is done (`EntityRenderer::render_ghosts`, `fs_ghost`): a spectator sees the model of an invisible entity at 15 % opacity, and a player in spectator mode is only a head and a hat. Not done: the team rule (an invisible teammate is a ghost to its team when the team allows it). The wither spawn countdown is done (entry 20: the `wither_invulnerable.png` texture that blinks and the size that grows from 1.5 to 2). The dragon death is not: it needs a client-side `deathTicks` counter, a dissolve by alpha threshold on `dragon_exploding.png`, the 60 coloured rays of `LayerEnderDragonDeath` (an additive pipeline with vertex colours) and the flight history of `rotateCorpse`, which the dragon lacks in flight too. Still to do: the glint of the held item in third person is the item pass (P3), arrows in the body, the creeper charge, world light, and the spectator ghost (alpha 0.15). Original plan: helmet, chestplate, leggings and boots of players and armoured
  mobs (zombie, skeleton, pigman): the two armour models (inner 0.5 and outer
  1.0 dilation), the leather tint from `display.color`, the enchantment glint,
  the equipment of `Entity Equipment` slots 1 to 4 (store them on the record
  next to `held`). Needs the armour textures from the jar
  (`models/armor/<material>_layer_1|2.png`, `_overlay` for leather).
- **Third-person held item for mobs:** zombie and skeleton items
  (`LayerHeldItem` generalised from `item_world::hand_frame`), the skeleton's
  bow.
- **Fire overlay:** the burning entity (`EntityFlags::on_fire`) with the two
  fire sprites of the block atlas, billboarded.
- **Invisibility:** invisible entities draw only their armour, the held item
  and glint (players); mobs vanish. Spectators see invisible players as ghosts
  (alpha 0.15), which needs a translucent entity pass.
- **Stored by P5d, drawn here:** creeper charge swirl (scrolling `creeper_armor.png`,
  additive), pig saddle, horse armour (iron, gold, diamond) and chest, enderman
  carried block and open mouth, arrows stuck in the body (`LayerArrow`).
- **Needs the renderer:** an additive blend pass and scrolling UVs for the
  charge layer; a second entity pass with alpha for ghosts. Today every entity
  vertex is opaque, depth-tested and fullbright.

### P5g — Lighting and shadows

MCP to read: `Render.doRenderShadowAndFire`, `Render.renderShadow`,
`Entity.getBrightnessForRender`, `RendererLivingEntity.doRender`.

- **Light at the entity position:** the block and sky light of the cell at the
  entity's eye height (`getBrightnessForRender`), replacing `FULL_LIGHT`
  (`0x00FF_00FF`; the word is now `0x0000_00FF`) in
  `entity_render::mod`. The mesher already knows `VertexLight` for items;
  the world light must reach the render thread (a lookup in `WorldView`, or
  the light in `EntityFrameData`, filled by the game thread from its `World`).
- **Shadows:** the blob under living entities, radius from the entity width,
  strength from the distance to the ground and the light
  (`shadowSize`, `shadowOpaque`), drawn as ground-projected quads on the
  blocks below.
- **Why second:** every layer of P5f and every object of P5h inherits the
  light word.

### P5h — Objects

MCP to read: `NetHandlerPlayClient.handleSpawnObject` / `handleSpawnPainting` /
`handleSpawnExperienceOrb` / `handleSpawnGlobalEntity` / `handleCollectItem` /
`handleEntityAttach`, `RenderBoat`, `RenderMinecart`, `RenderTntPrimed`,
`RenderFallingBlock`, `RenderArrow`, `RenderSnowball`, `RenderFish`,
`RenderEnderCrystal`, `RenderFireball`, `RenderLeashKnot`, `RenderArmorStand`,
`RenderXPOrb`, `RenderPainting`, `RenderLightningBolt`, `EntityArrow.onUpdate`,
`EntityThrowable.onUpdate`, `EntityFallingBlock`, `EntityItem.onUpdate`.

- **Object table:** every `Spawn Object` type (boat 1, minecart 10 to 12,
  TNT 50, ender crystal 51, arrow 60, snowball 61, egg 62, fireball 63 and
  64, ender pearl 65, wither skull 66, falling block 70, item frame 71, eye of
  ender 72, potion 73, exp bottle 75, firework 76, leash knot 77, armour stand
  78, fishing bobber 90); only 2, 60, 71 and 78 are mapped today.
  `EntityKind::from_object_type` and `meta/` grow with it.
- **Models and renderers:** boat, minecart (and its block), primed TNT with its
  flash, falling block, arrows (stuck or flying, tilted by velocity), thrown
  items and fireballs as sprites, fishing bobber and line, ender crystal with
  its base, leash knot, XP orbs (glow and size by value), paintings (the art
  table and the wall placement), lightning bolt (`Spawn Global Entity`), armour
  stand (body, arms, plate; the pose of `meta::ArmorStandMeta`, already
  decoded).
- **Client-side motion:** projectiles, items and falling blocks are simulated
  between packets (gravity, drag, ground contact), as vanilla does
  (`motion.rs` gets an object stepper next to the living glide).
- **`Collect Item`:** the pick-up animation (item flies to the collector in 3
  ticks) and the XP orb version.
- **Riding and leashes (`Attach Entity`):** place passengers on their vehicle
  (`ride.rs`, per-vehicle seat offsets), the rider's body yaw clamp
  (`RendererLivingEntity` 85 degrees), the leash line to the holder; the
  rider hides its own name tag.
- **Storage:** `EntityRecord` gets an object payload (block id, owner, stuck
  flag); keep it out of the living fields.

### P5i — Local player body

MCP to read: `EntityPlayerSP.onUpdate`, `EntityLivingBase.onUpdate` (body-yaw
block), `RenderPlayer`, `Minecraft.clickMouse`, `PlayerControllerMP`.

- **Own skin and cape:** `follow_body` builds its record without a profile, so
  `game.body` wears the default skin: give it the profile of the local player
  (the name and UUID are known at login; the properties come from the tab list
  entry of the local UUID) and tick the cape chase in `follow`.
- **Body-yaw lag and head:** `EntityRecord::follow` calls the player body-yaw
  algorithm already; check it against the F5 recordings (the local head is the
  camera yaw, the body trails it).
- **Arm swing:** `game.hand` knows the swing of the first-person arm; feed the
  same swing into `body.animation.swing_item()` on attack and block break, and
  mirror `Animation` 0 for the local entity.
- **Held item and poses:** the local held stack for the third-person item,
  the use poses (block, bow) from `item_use`, the sneak pose
  (`EntityFlags` bit 1 is set by `follow_body`), the flying and sprint poses.
- **No name tag** for the local player (the store never holds it, keep it so).

### Not part of any P5 step, but found on the way

- **Remote crouch sink:** `RenderPlayer.doRender` lowers remote crouching
  players by a further 0.125 (needs an "is local" flag on the snapshot).
  Listed in `docs/backlog.md`.
- **Crit particles:** `Animation` 4 and 5 belong to P8.
- **Scoreboard below the name:** the objective in display slot 2 under the
  tag (`RenderPlayer.renderOffsetLivingLabel`).
- **Mob tag under the crosshair:** a custom-named mob shows its tag when
  looked at (needs the entity ray cast of P6).
- **Tag occlusion:** tags are drawn without a depth test; vanilla fades them
  behind terrain. Needs either a depth-aware overlay pass or an occlusion
  ray cast on the game thread.

Work in small commits, one milestone at a time. Before finishing a milestone:
`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and update `docs/sources.md`.

## Done when (from `ROADMAP.md`)

On a live server, side-by-side recordings of a reference scene (players
walking, sneaking, sprinting and fighting; every mob type; projectiles;
vehicles; item frames) match vanilla on the owner's checklist; unit tests check
the swing, yaw and interpolation maths against the MCP formulas. The
recordings cannot be produced by the agent; they stay with the owner. What the
owner has already compared: head turn, item in the right hand and its
orientation (diamond sword only).

Verification still owed to the owner:

- every item display transform in the hand (tools, torch, blocks, bow);
- non-humanoid mob poses after the model mirror (legs, wings, tails);
- the cape and the real download of a skin on a live server;
- the name tag position and size against the vanilla screenshot.

## Known gaps and open questions

- P5e: the download was tested offline only (URL parsing and whitelist, cache,
  decoding); no live server or Mojang texture host was reached. A tag shows
  through terrain (vanilla fades it there); the below-name score and riders
  are not handled; the cape's walking bob is left out (it is zero for remote
  players); the local player still wears the default skin until P5i.

- P5d approximations: a foal is the whole horse at half size (the vanilla model
  shrinks it part by part); slimes ignore the squish; wolves and ocelots do not
  sit or beg yet; the white flash is a constant mix of the texture.

- `docs/backlog.md` (Entities): the extra 0.125 sink of remote crouching
  players (needs an "is local" flag on the snapshot); crit particles
  (`Animation` 4 and 5, need P8).
- Non-humanoid mob poses (legs, wings, tails) were not compared with vanilla
  after the model mirror was added; only head yaw and pitch were checked.
- Item display transforms were checked visually only for the diamond sword
  (`/tmp/tes` screenshots, session of 2026-10-29); torch, blocks and tools
  should be compared too.
- The "Done when" of the roadmap (recordings against vanilla on a live server)
  cannot be produced by the agent; it stays with the owner.
- Handy tools: `umicraft-client --entity-demo --screenshot PATH --demo-view
  mobs|closeup|walk-cycle` renders offscreen; the ignored test
  `item_world::players_hold_their_items` (set `UMICRAFT_PLAYER_DUMP`) renders
  players with held items.
