# P5 — Entities: plan and architecture

Status at the end of the session of 2026-10-29: **P5a, P5b and P5c are done**
(commits `18d3a0e`, `7ad7f68`, `ff2efaf`, `5242b73`, `4cd5647`, `29010d0`).
P5d onwards remain. The decision record is
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

Planned: `meta/` (one typed decoder per family), `objects.rs`, `ride.rs`,
`players.rs`.

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

Planned: `layers/` (armour, held item, cape, fire, wool, horse gear, …),
`objects.rs`, `nametag.rs`, `shadow.rs`, `lighting.rs`, `skin_cache.rs`.

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

Remaining, in order:

| | Content | Notes |
|---|---|---|
| P5d | Typed metadata and mob variants: baby, sheep colour and shear, slime size, creeper fuse and charge, wither skeleton, zombie villager, villager profession, horse type/variant/armour, wolf and ocelot taming and collar, custom name, armour-stand flags and pose, potion colour, arrows in body, absorption | One decoder per family under `meta/`, one agent per family |
| P5e | Players: `Player List Item` profiles, skin and cape fetch and cache (`fetch_mojang_skin` exists, unused), slim model from the profile, skin layers from metadata, name tags with team rules | `players.rs`, `skin_cache.rs`, `nametag.rs` |
| P5f | Layers: armour, third-person held item for mobs, fire overlay, invisibility | `layers/` |
| P5g | World lighting at the entity position, shadows | `lighting.rs`, `shadow.rs`; replaces the fixed `0x00FF_00FF` light |
| P5h | Objects: boats, minecarts, TNT, falling blocks, arrows, projectiles, fishing bobber, ender crystal, fireworks, leash knot, item frames, armour stands, XP orbs, paintings, lightning; client-side motion; `Collect Item`; riding and leashes (`Attach Entity`) | `objects.rs`, `ride.rs` |
| P5i | Local player body: body-yaw lag, arm swing on attack and break, held item, own skin and cape, sneak and fly poses | Reuses `BodyYaw` and `Animation` through `EntityRecord::follow` |

Work in small commits, one milestone at a time. Before finishing a milestone:
`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and update `docs/sources.md`.

## Known gaps and open questions

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
