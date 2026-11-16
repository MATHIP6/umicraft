# ADR-0069 — Mob animation state on the client

Status: accepted (2026-11-09)

## Context

Several 1.8 mob animations are driven by state the client keeps between
packets. The server sends only a trigger (an `Entity Status`: 1 rabbit hop,
4 and 11 golem swing and poppy, 8 wolf shake, 10 sheep grazing) or nothing at
all (a chicken's wings follow its own motion). `EntityStore::status` dropped
those statuses, and the renderer had no way to read such timers.

## Decisions

- **One seam per entity, `entity::mob_state`.** `EntityRecord::mob` is a
  `MobState` enum with one variant per kind that keeps state, created by
  `MobState::for_kind`. It has three operations: `status(i8) -> bool` (called
  first by `EntityStore::status`), `tick(MobTick)` (called from
  `EntityRecord::animate` with the on-ground and begging inputs) and
  `pose(alpha) -> MobPose` (the interpolated values the render snapshot carries
  in `EntityRenderSnapshot::mob`). A new kind adds a variant there, not more
  fields on the record or the snapshot. The horse keeps its existing
  `HorseState`, which predates this.
- **Models stay independent of the game crate.** They take plain values
  through setters called next to `set_angles` (`RabbitModel::set_angles(hop,
  ..)`, `IronGolemModel::set_action(GolemAction)`, `WolfModel::set_motion`,
  `SheepModel::set_graze`, `ChickenModel::set_wings`); the client maps
  `MobPose` to them.
- **Vanilla quirks are kept**: status 11 always starts the 400-tick poppy
  (1.8 sends it for both `setHoldingRose(true)` and `(false)`).

## Not covered

The wet wolf's darker colour (`RenderWolf`, `getShadingWhileWet`) needs a
colour multiplier in the entity shader; the wolf's wetness itself
(`isWet()`, in water or rain) is not tracked.
