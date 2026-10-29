# ADR-0026 — P5: Entity modules

**Status:** accepted
**Date:** 2026-10-29
**Milestone:** P5 (finishes M8)

## Context

P5 adds animation maths, entity states, typed metadata, players' skins,
layers, lighting and objects. Before it, `entity/types.rs` (524 lines) held
every entity type and `umicraft-client/src/entity_textures.rs` (918 lines)
held the texture registry, the mesh pipeline and one 330-line `match` over
every mob.

## Decision

### Game layer: `umicraft-game/src/entity/`

| File | Owns |
|---|---|
| `kind.rs` | `EntityId`, `EntityKind` and the wire type tables |
| `flags.rs` | `EntityFlags` |
| `item.rs` | `EntityItem` (held, dropped, framed stacks) |
| `record.rs` | `EntityRecord`, the mutable per-entity state |
| `snapshot.rs` | `EntityFrameData`, `EntityRenderSnapshot` and the interpolation |
| `store.rs` | `EntityStore`, which owns the records and applies packets |

P5b/P5c add `limbs.rs` (swing and body yaw), `motion.rs` (3-tick teleport
interpolation) and `anim.rs` (hurt, death, arm swing). Later milestones add
`meta/`, `objects.rs`, `ride.rs` and `players.rs`. Each module stays under
about 400 lines and holds pure functions tested against the MCP formulas.

### Client: `umicraft-client/src/entity_render/`

| File | Owns |
|---|---|
| `mod.rs` | `mesh_entities`: snapshots to batched vertices per texture |
| `textures.rs` | the texture registry and its GPU upload |
| `mobs.rs` | one builder per model family (humanoid, animal, creature) |
| `player.rs` | the posed player model |
| `hand.rs` | the first-person arm |

Later milestones add `layers/` (armour, held item, cape, fire, hurt tint),
`objects.rs`, `nametag.rs`, `shadow.rs` and `lighting.rs`.

### P5b/P5c additions

`angle.rs`, `limbs.rs`, `body_yaw.rs`, `motion.rs`, `anim.rs` (game) and
`entity_render/{pose,transform}.rs`, `umicraft-model/src/entity/pose.rs`.

Two corrections found on the way, both against vanilla: models were drawn at
`-yaw` instead of `180 - yaw` (every entity faced backwards), and the crouch
lifted the player instead of lowering it. The humanoid body now pivots at the
neck like the vanilla torso, so the crouch lean is right.

The entity models are also mirrored along x when drawn
(`entity_render::transform::x_mirror`): their boxes and UVs follow the vanilla
model space, which vanilla draws through `scale(-1, -1, 1)`. Without it the
head turned the wrong way, the right arm was on the left and skins were
mirrored. Attachments given in the vanilla frame (the held item) hang on
`body_matrix` and apply the mirror themselves.

### P5d additions

`entity/meta/` (one decoder per family: `common`, `animals`, `horse`,
`monsters`, `armor_stand`), `entity_render/{animals,creatures,variants,
specs,horse_coat}.rs`, `umicraft-model/src/entity/child.rs`.

Typed metadata lives on `EntityRecord::meta` and travels in the snapshot.
The vertex `light` word of `EntityVertex` carries the per-vertex effects the
shader needs, so the vertex layout is unchanged: bit 8 hurt tint, bits 9-12
white flash level, bit 13 dye tint with its colour in bits 14-17. Horse coats
are composed on first use and cached under `TextureSlot::HorseCoat`.

## Consequences

P5a changes no behaviour (same tests, same vertex counts). `game.rs` only
gets one-line calls into the entity modules.
