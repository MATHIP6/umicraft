# 0057. Lighting: cells that use neighbour brightness

Status: accepted

## Context

The mesher read the raw stored light of a cell. Vanilla does not
(`Block.getMixedBrightnessForBlock`, `World.getCombinedLight`,
`World.getLightFromNeighborsFor`):

- a block with `useNeighborBrightness` takes, per channel, the brightest of the
  cells above and to its four sides instead of its own light. `Block.registerBlocks`
  sets it for stairs, slabs, farmland and every block that lets light through
  (a material that does not block light, or a light opacity of 0: glass, torches,
  fences, plants, chests, snow layers, carpets, lava...). Water, ice, web and
  leaves have a light opacity above 0 and keep their own light;
- the block light is at least the block's own emission;
- a slab whose combined light is 0 reads the cell below it.

A half slab or a stair stops light, so the engine stores 0 in its cell. A block
face beside one, or a block seen through one, sampled that 0 as the outward cell
and came out dark where vanilla shows the light of its surroundings: dark patches
on walls next to slabs and stairs, and shadows under torches and glass.

## Decision

- `umicraft-world` exports `uses_neighbor_brightness` and `is_slab`, built from
  the registrations above.
- `BlockSource` gets `combined_light` and `mixed_light`. The smooth and flat
  light of a quad (`lighting.rs`) use `mixed_light`; the liquid mesh and the
  block-entity light use `combined_light` with no emission, as `BlockLiquid` and
  `TileEntityRendererDispatcher` do.

## Consequences

- Faces beside slabs, stairs, farmland, glass and the like are lit like their
  surroundings. Leaves and water are unchanged.
- Each lit cell costs one more block lookup, and five more light lookups for the
  blocks that use their neighbours. Air does not.
- The client relight after a block change (`relight.rs`) still treats slabs and
  stairs as transparent; the stored values for them are then not 0 until the
  chunk is resent. The rule above does not depend on them.
