# P7 — World rendering: plan and architecture

Status: **P7-0 to P7d are done** (unit tests; not yet checked in game). P7e onwards
remain. The decision record is `docs/decisions/0030-p7-actual-state.md`;
MCP sources are in `docs/sources.md` (rows dated 2026-10-31). The roadmap entry
is `ROADMAP.md` §P7.

Rule (as in P5 and P6): a step adds a module, existing big files only get
wiring; modules stay under about 400 lines and test their logic against the
MCP rules.

## Architecture

### Mesh — `umicraft-mesh/src/`

| Module | Owns |
|---|---|
| `vertex.rs`, `mesh.rs`, `face.rs` | GPU vertex, the two-pass CPU mesh, the face table |
| `source.rs` | `BlockSource`, `ChunkSnapshot` |
| `legacy.rs` | procedural cubes, cull rule, pass choice |
| `lighting.rs` | smooth light and ambient occlusion |
| `models.rs` | the model mesher (asks `actual_state` before the wire-state table) |
| `block_entity.rs` | `BlockEntityRef`: chests and signs found by the chunk scan |
| `actual_state/` | `getActualState`: one module per family, `FAMILIES` table in `mod.rs` |

### Model — `umicraft-model`

`BlockModelSet::model_by_key` (all variants of all referenced blockstate
documents are baked).

## Steps

| Step | Content | State |
|---|---|---|
| P7-0 | split `umicraft-mesh/src/lib.rs` | done |
| P7a | fences, panes, iron bars, walls, fence gates in walls, stair shapes | done |
| P7b | doors, double plants, vines, redstone wire, tripwire, snowy grass | done |
| P7c | block-entity references from the mesher, chests with the lid, signs with text | done in code, not checked in game |
| P7d | skulls, banners, beacon, end portal, enchanting book, spawner, flower pots | done in code, not checked in game |
| P7d-pistons | moving pistons: structure rules, animation, push of the player, drawing | done in code, not checked in game |
| P7e | vanilla sky: sun, moon, clouds, stars, Nether and End | not started |
| P7f | weather | not started |
| P7g | world border, fog parity | not started |
| P7h | animated texture check, frame-time benchmark, parity rows | not started |

## Known gaps

- Pane and wall connections use `BlockState::is_opaque()` as `Block.fullBlock`;
  a block the world crate classes differently from 1.8 connects differently.
- The redstone power-provider list is an id table, not per-state.
- Hook support (`doesBlockHaveSolidTopSurface`) ignores upside-down stairs and top slabs.
- Not yet checked in game against the reference screenshots.

## Block entities (P7c)

The mesher emits no quad for chests and signs; it lists them on `Mesh`
(`block_entities`, with light and same-block neighbours). `WorldView` keeps
the list per chunk. Each frame `umicraft-client/src/block_entity_render/`
meshes the ones within 64 blocks into entity vertices (`chest.rs`, `sign.rs`)
that the entity pass draws. Lid angles come from
`umicraft_game::server::chest_lids` (`Block Action`), sign lines from
`BlockEntities::sign_table()`.

## Pistons

`Block Action` for a piston (33, 29) reaches the client before the block
changes of the tick. `umicraft_game::piston::motion::start` plans the move on
that pre-move world (`structure::plan_move`, the port of
`BlockPistonStructureHelper`), clears the source cells, puts block 36 in the
destination cells and records a `MovingBlock` per moved block. `tick` advances
them by 0.5 per tick and `push::launch` shoves the local player (the physics
step runs first, then the pistons, as `World.updateEntities` does). The render
thread draws each moving block interpolated (`moving_blocks_draw`,
`umicraft_render::dynamic_blocks`). Collision boxes of the base and head are in
`piston_shape.rs`; before this the sticky piston and the head had none.

## The other block entities (P7d)

`BlockEntityKind` now also lists skulls, banners, beacons, end portals,
enchanting tables, spawners and flower pots. `replaces_model()` says whether
the renderer draws the whole block (skull, banner, portal: no model) or adds to
the model (table, spawner, beacon base, pot). Their NBT comes from
`BlockEntities::entity_table()` (an `Arc` snapshot like the signs). A renderer
can write to the `solid` or `additive` batches of `BlockEntityMeshes`; the
pot plants go through the dynamic terrain mesh like the moving blocks.
