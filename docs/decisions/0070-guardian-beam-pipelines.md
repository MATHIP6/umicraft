# ADR-0070 — Guardian beam and apparition pipelines

Status: accepted (2026-11-11)

## Context

The entity pass had four colour pipelines: lit (`fs_main`), additive glow,
equal-depth glint and translucent ghost. Two guardian effects fit none of them:

- the elder guardian apparition (`MobAppearance.renderParticle`) is translucent
  but writes depth (`depthMask(true)`); drawn as a spectator ghost (no depth),
  its back showed through its face;
- the guardian beam (`RenderGuardian.doRender`) is unlit, not blended, alpha
  tested, writes depth and is coloured per vertex with an arbitrary RGB that
  changes with the charge. The lit pipeline's light word has no room left for
  an RGB (dye, flash, cutoff and opacity bits are taken), and the additive
  pipeline blends and writes no depth.

## Decisions

- **Two pipeline kinds**, `APPARITION` (the ghost pipeline with depth write)
  and `TINTED` (`vs_additive`, whose light word is already an RGB tint, with a
  new `fs_tinted`: alpha test 0.1, fog, opaque, depth write). Each has its own
  batch buffer in the renderer, drawn after the entities. A `const` assertion
  pins that the apparition writes depth and the ghosts do not.
- **The beam's target is resolved on the game thread**, like a leash holder
  (`entity::guardian_beam::aim_guardians`): the render thread only has the
  snapshots it draws, and the local player is not among them in the first
  person, though it is the usual target. The frame carries the target's middle
  at both ticks; the snapshot interpolates it.
- **Quirks are kept**: the beam overshoots its target by a block, and the colour
  channels wrap through the `(byte)` cast of `WorldRenderer.color` near the end
  of the charge.
- **One difference**: the reference scrolls and turns the beam by the total
  world time, which the client does not track; the guardian's age stands in,
  which only shifts the phase.

## Consequences

A new opaque effect with an arbitrary colour can reuse `TINTED`. Texture slots
whose coordinates scroll but are not additive say so in
`TextureSlot::repeats`.
