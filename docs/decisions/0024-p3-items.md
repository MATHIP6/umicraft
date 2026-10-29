# ADR-0024 — P3: Items (registry, models, rendering)

**Status:** accepted (implemented; screenshot diff pending the owner reference set)
**Date:** 2026-10-28
**Milestone:** P3

## Context

Items were bare `ItemStack`s: names, durabilities and tool ids lived as
hand-kept lists in `umicraft-client/game.rs` and `umicraft-game`, the slot
widget drew the id as text and dropped items were generic cubes. P3 needs a
registry, the `models/item` pipeline and item rendering, without growing the
already large `game.rs`, `renderer.rs` and `hud.rs`.

## Decision

One responsibility per crate, one module per concern, no file above ~450 lines:

| Layer | Module | Role |
|---|---|---|
| `umicraft-game` | `items/` (`kind`, generated `table`) | `ItemDef` per id: name, stack, durability, `ItemKind` (block, tool + tier, armour, food) |
| `umicraft-game` | `nbt`, `item_nbt` | bounded NBT tree; `ItemDisplay` (name, lore, enchantments, leather colour, skull owner). `digging::enchantment_level` now reads through it (the former hand-rolled cursor is gone) |
| `umicraft-model` | `display` | `display` transforms per context, with per-context parent inheritance |
| `umicraft-model` | `model/{document,loader,mod}` | `model.rs` split: JSON documents, parent merge and baking shared by blocks and items; model keys are now relative to `models/` (`block/stone`, `item/iron_sword`) |
| `umicraft-model` | `item/` (`names`, `special`, `generated`, `set`) | `ItemModelSet`: `(id, meta)` to a baked `ItemModel`; `builtin/generated` extrusion; `builtin/entity` marker |
| `umicraft-mesh` | `item` | `ItemMesh`: places model quads with a pose into terrain `Vertex`es (item sprites share the block texture array) |
| `umicraft-render` | `items/` (`icon_atlas`, `dynamic_mesh`, glint) | icons rendered once into a 2048 px atlas of 64 px cells through the terrain pipeline without culling; `DrawCommand::Item` becomes an atlas quad in command order, so counts and bars stay above the icon; per-frame world and hand item meshes drawn with the terrain pipeline |
| `umicraft-ui` | `widgets/item_overlay`, `slot` | icon command, stack count, durability bar |
| `umicraft-client` | `item_icons`, `item_world` | icon keys and tints, the `IconProvider`, dropped items, the first-person item, items in a remote player's hand, item frames |

Generated tables (`docs/tools/gen_item_data.py`, `gen_item_model_names.py`)
come from the local MCP sources (ADR-0001); no Mojang asset enters the repo.

The model crate reads no textures. `builtin/generated` needs the sprite
silhouette, so the caller supplies it through `SpriteLookup` (`SpriteMask`),
and an unknown sprite is treated as opaque.

## Consequences

- `ClientResources` now loads all of `assets/minecraft/models/` (block, item
  and the `builtin/*` parents are markers, not files).
- `builtin/compass` and `builtin/clock` bake like `builtin/generated`; their
  animated sprite is a rendering concern.
- Validated against a real 1.8.8 jar: all 512 item models bake.
- Known gaps for the rendering steps: fish food values depend on metadata
  (table has 0), the missing-model fallback is "no model", animated
  compass/clock sprites.

## Why an icon atlas

Rendering icons into an atlas once, instead of drawing 3D item geometry
inside the UI frame, keeps every slot a plain textured rectangle: the UI stays
backend-agnostic, draw order is the command order (background, icon, count,
bar, highlight) and a steady-state frame renders no item geometry. The cost is
a fixed icon resolution (64 px per 16 GUI px) sampled with the nearest-neighbour
GUI sampler.

## Known gaps

Listed in `docs/parity.md` (Items). The largest are `builtin/entity` items
(P7), the use animations of other players (P5) and maps.

## Dials and use animations

The compass and the clock keep every frame of their strip
(`umicraft_assets::DialSprite`); `umicraft_client::dials::DialAnimator` runs the
reference's damped spring once per 20 Hz tick and writes the chosen frame into
the block array layer, so GUI icons, dropped and held items share it. The icon
key of a dial item holds its frame, so the icon cache keeps one picture per
frame. The first-person use animations live in `umicraft_game::hand`
(`HandPose::item_use`, published with the frame); the client only picks the
bow drawing model.
