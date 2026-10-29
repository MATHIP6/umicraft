# ADR-0028 — P3: approved deviations in item randomness

**Status:** accepted
**Date:** 2026-10-29
**Milestone:** P3

## Context

Two P3 details draw random numbers in the reference:

- `EntityItem.<init>` sets `hoverStart = (float)(Math.random() * Math.PI * 2.0)`
  once per dropped item (it drives the bob and the spin in `RenderEntityItem`).
- `TextureCompass.updateCompass` and `TextureClock.updateClock` use
  `Math.random()` for their target outside the surface world (Nether, End).

`Math.random()` is seeded from the clock: no seed, entity id or server data
reproduces a given draw, so there is nothing to match bit for bit. (The scatter
of stacked blocks, by contrast, is `new Random(187)` in `RenderEntityItem` and
is reproduced exactly by `item_world::JavaRandom`.)

## Decision

Keep the deterministic generators already in place and accept the deviation:

- the hover phase is a hash of the entity id mapped to `0..2π` (same
  distribution, stable across frames, no state to store per entity);
- the dial targets outside the overworld come from the fixed-seed xorshift of
  `dials.rs` (same distribution, reproducible in tests).

## Consequences

Two clients never agree with the vanilla client on the phase of a given item or
on the needle of a Nether compass, and they cannot, since vanilla clients do
not agree with each other either. Screenshot comparisons must not depend on
either value.
