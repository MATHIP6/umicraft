# ADR-0024 — P3: Items (registry, models, rendering)

**Status:** accepted (steps 1–3 implemented, rendering pending)
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
| next | `umicraft-mesh`, `umicraft-render/items`, `umicraft-ui` slot | meshing, GUI icons, dropped and held items, frames |

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
