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

Done:

| | Content |
|---|---|
| P5a | Module split, no behaviour change |
| P5b | Vanilla limb swing, body yaw (`renderYawOffset`), 3-tick teleport, head look applied at once |
| P5c | Hurt flash and death fall (`Entity Status` 2/3), arm swing, sleeping (`Use Bed`), sneak, sword block and bow poses, entity orientation fix (`180 - yaw`), model mirror, player scale 0.9375, held item on the right hand |

| P5d | Typed metadata for every class (merged from partial updates, health 0 starts the death fall) and the variants it changes: babies (proper head and body proportions), sheep colour and shearing, wolf tame, angry and collar, ocelot, rabbit, villager professions, horse type and composed coats, wither skeleton, zombie villager, slime size, creeper swell and white flash |

| P5e | Player profiles from the tab list, skins and capes (trusted download on a worker thread, disk cache, slim arms from the profile, 64x32 conversion), skin layers from the metadata, the cape with its swing, name tags with the team rules (prefix, suffix, visibility) |

What P5d stores but does not draw yet: custom name (P5e), potion colour (P8
particles), arrows in the body, absorption hearts (P6), creeper charge layer,
pig saddle, horse armour and chest, enderman carried block (all P5f), armour
stand pose (P5h).

Remaining, in order:

| | Content | Notes |
|---|---|---|
| P5f | Layers: armour, third-person held item for mobs, fire overlay, invisibility | `layers/` |
| P5g | World lighting at the entity position, shadows | `lighting.rs`, `shadow.rs`; replaces the fixed `0x00FF_00FF` light |
| P5h | Objects: boats, minecarts, TNT, falling blocks, arrows, projectiles, fishing bobber, ender crystal, fireworks, leash knot, item frames, armour stands, XP orbs, paintings, lightning; client-side motion; `Collect Item`; riding and leashes (`Attach Entity`) | `objects.rs`, `ride.rs` |
| P5i | Local player body: body-yaw lag, arm swing on attack and break, held item, own skin and cape, sneak and fly poses | Reuses `BodyYaw` and `Animation` through `EntityRecord::follow` |

Work in small commits, one milestone at a time. Before finishing a milestone:
`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and update `docs/sources.md`.

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
