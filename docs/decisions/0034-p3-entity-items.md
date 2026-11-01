# ADR-0034 — Skull and banner items through the tile-entity models

**Status:** accepted
**Date:** 2026-11-01
**Milestone:** P3 (items), reusing the block-entity renderers of P7

## Context

The skull and banner items have a `builtin/entity` model: no quads, so the
sprite pipeline (block atlas, `ItemMesh`) could not draw them anywhere (GUI,
hand, ground, frames). The remaining list of P3 named them.

The 1.8 client has no separate code for these items. `RenderItem.renderItem`
applies the display pose of the context, `scale(0.5)`, `rotate(180, y)` and
`translate(-0.5)`, then calls `TileEntityItemStackRenderer.renderByItem`,
which draws the tile-entity model at the origin: a floor skull (`UP`, `Rot` 0,
scaled by 2 after `translate(-0.5, 0, -0.5)`) or a standing banner without a
world (no turn, no sway phase, base dye from `BlockEntityTag.Base` or the
damage).

## Decision

- **One mesh per stack** (`block_entity_render::item::entity_item_mesh`): the
  models and textures of the placed skull and banner, built in the cube space
  of the baked item models with the half turn folded in, so that
  `gui_pose`, `held_pose`, `ground_pose` and `context_pose` apply unchanged.
  `BannerDesign::from_item` reads `BlockEntityTag` and the damage.
- **World contexts** (ground, frames, first and third person hands) add their
  vertices to the tile-entity batches of the entity pass
  (`item_world::entity_items`), placed by the matrices of the sprite items.
  The first-person hand is uploaded as two batches, arm and item.
- **GUI icons** are drawn into the icon atlas with the *entity* pipeline
  (`Renderer::render_entity_icon`), because the icon atlas pipeline samples
  the block array. `IconProvider` is unchanged: the client prepares these icons
  before the UI is drawn (`entity_icons::EntityIcons`), since composing a banner
  texture or reading a skin needs the renderer, which is busy while the
  provider is called.
- The icon key identifies a skull or banner by a 24-bit hash of its NBT in the
  colour field; `ItemResources` remembers the stack behind each key.

## Consequences

- No new rendering code for the models: a fix to the placed skull or banner
  fixes the items.
- A player skull whose owner's skin is downloading is drawn with the default
  skin and redrawn (the old cell is forgotten) once the skin is ready.
- Deviations: the GUI icons are lit with the entity shader's face shade at
  full light, not `RenderHelper.enableGUIStandardItemLighting`; the glint of
  enchanted skulls and banners is not drawn; a skull owner given only as a
  name is not looked up (the same as the placed skull); two stacks that differ
  only in unrelated NBT (a custom name) get separate icon cells.
- Chests, ender chests and trapped chests are `builtin/entity` too and can use
  the same path later.
