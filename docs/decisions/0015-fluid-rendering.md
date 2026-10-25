# ADR-0015 — Water and lava are drawn by a fluid mesher, not as cubes

- Status: Implemented
- Date: 2026-10-25
- Deciders: project owner
- Supersedes: the liquid case of the model-less fallback of ADR-0007
- Related: ADR-0008 (transparency), ADR-0009 (biome tints), ADR-0012
  (terrain textures), ADR-0014 (movement media)

## Context

Liquids have no blockstate model in 1.8; the client rendered them as legacy
cubes with the still/flowing textures. Every level looked like a full block,
lakes showed vertical walls between levels, falling water and slopes did
not exist, and lava used the still texture on its sides. The reference draws
liquids with its own `BlockFluidRenderer`.

## Decision

1. **The liquid rules shared by physics and rendering live in
   `umicraft_world::fluid`**: the liquid materials, the empty share of a level
   (`getLiquidHeightPercent`), the flow vector of water and lava
   (`getFlowVector`) and the two material properties it reads, over a block
   accessor so the game thread's world and the mesher's snapshots share one
   implementation. `umicraft_game::media` and `umicraft_mesh::fluid` both use
   it.
2. **`umicraft_mesh::fluid` follows `BlockFluidRenderer.renderFluid`**: corner
   heights from the four columns sharing each corner, the still texture or the
   flowing texture turned along the current on the surface, the flowing
   texture up to the corner heights on the sides, faces only towards blocks
   that are neither the same liquid nor opaque, drawn from both sides, the
   surface's underside where a non-full block borders the space above.
3. **Liquids keep the reference's lighting**: flat light (the brighter of the
   block and the block above it) and the fixed colour factors per face (top 1,
   bottom 0.5, north/south 0.8, west/east 0.6) baked into the vertex tint,
   instead of the smooth lighting and the shader's face shade of other blocks.
4. **Water is translucent, lava opaque** (`BlockLiquid.getBlockLayer`).
5. **The procedural texture set uses the same geometry** with its single
   liquid layer.

## Consequences

- Lakes, slopes, falls and lava flows look like the reference; a lone source
  is 11 quads (both sides of its faces), a source inside a lake only its
  surface.
- Texture animation (the moving water and lava) is still missing; it belongs
  to the animated-texture work of the atlas (AST-6).
- A level change re-meshes the face-adjacent chunks only; a corner height at
  a diagonal chunk corner can lag until that chunk re-meshes.
