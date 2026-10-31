# ADR-0033 — Colour arithmetic on the stored values (no sRGB target)

**Status:** accepted
**Date:** 2026-10-31
**Milestone:** P7 (found while comparing grass and leaves with vanilla)

## Context

Screenshots of the same spot showed the grass and the leaves paler and greyer
than in the 1.8 client. The cause was the colour space, not the tints: the
surface and every texture were sRGB, so the GPU decoded the texels to linear
light, the shaders multiplied them by the biome tint, the light and the fog
(all of them 8-bit values meant to be multiplied as stored), and the GPU encoded
the product again. A product of stored values is not the stored value of a
product: grass `0.6 x 0.57` is `0.34` in the reference and came out near `0.46`,
lighter and less saturated.

Local patches had grown around it: the overlay shaders decoded their vertex
colours, the tooltip linearised its colours, the additive entity layers raised
their tint to 2.2, and the sky and the fog were left raw and read too light.

## Decision

The 1.8 client does all its arithmetic on the stored 8-bit values and never uses
an sRGB framebuffer. The renderer does the same:

- the surface format is the adapter's default **without** its sRGB suffix, the
  offscreen target is `Rgba8Unorm`;
- every picture is uploaded as `Rgba8Unorm`, so a texel reaches the shader as
  stored;
- the compensations are removed (overlay shaders, tooltip, additive entity
  layers, the TNT flash).

## Consequences

- Texel x tint x light x fog x blend give the numbers of the reference; the
  sky, the clear colour and the fog are no longer "too light".
- The offscreen PNGs hold the stored values, as before.
- A future pass that wants linear light (none today) must convert explicitly.
- Textures are filtered on the stored values, as the reference does.
