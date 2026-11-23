# ADR-0094 — Modding API 1.1: render camera, projection, entity snapshot, depth, flight

Status: accepted (2026-11-23)

## Context

API 1.0 (ADR-0086, ADR-0087, frozen in F3.9) leaves five gaps that external mods
reported:

1. a widget cannot place an annotation over a world point: `FRAME` has no camera
   matrix, and widgets only hang from the nine anchors;
2. a mod cannot move the render camera without moving the player;
3. a HUD widget cannot read entities at all (world queries answer on the game
   thread only), and the entity record has no health, size, velocity or flags;
4. a post-process pass sees colour only, no depth;
5. `UMI_PHYSICS_FLYING` is read-only, so a physics layer cannot toggle flight.

The ABI rule of 1.0 is append-only within major 1 (ADR-0087 amendment).

## Decision

API **1.1**. Everything is appended; no 1.0 offset, id or meaning changes, and the
1.0 layout tests stay untouched. The exact types are frozen in
`docs/tasks/api-1.1/README.md`.

### Render camera: a service, not a setter

`render_camera` is a second service with the same rules as `player_physics`
(ADR-0086): any number of wrapping layers, at most one replacing layer, declared in
`[load]`. The physics chain is made generic (`ServiceChain`) so both services share
one implementation of nesting, budgets, `next()` and the per-thread `cx` guard.

A layer runs on the render thread once per frame, after the built-in camera is
placed (eye, angles, effective FOV, bob) and before the camera uniform is written. It
sees `UmiRenderCamera` (eye in f64, Minecraft yaw/pitch in degrees, effective
vertical FOV, aspect read-only, view-bob matrix) and `UmiCameraInput`, derived from
the aggregated input intent the built-in view already consumes (look deltas,
forward, strafe, up); no raw key state, as in 1.0. The camera never feeds back into
the player: the simulation runs from the input as before.

`PRE_RENDER` (event 13, render thread, read-only, not cancellable) fires right after
the camera layers, with the final camera of the frame.

The smaller alternative (a `camera_set` host call) was rejected: two mods setting the
camera would race, and the service already gives ordering and conflict detection.

### Projection lives in `FRAME`

`FRAME` gains the final camera (eye, render origin, view-projection relative to that
origin, viewport, GUI scale, effective FOV, aspect, view bob). To make it exact, the
client dispatches `FRAME` and draws external widgets **after** the camera of the
frame is final (camera layers included) instead of at the start of the frame. The
event still fires once per frame before the HUD, so 1.0 mods see no difference but
fresher data. `Frame::world_to_screen` projects in f64 relative to the render origin,
exactly as the renderer does.

Widgets gain `Anchor::Screen` (index 9: absolute GUI pixels, not movable or scalable
in the HUD editor) and two primitives, `hud_outline` and `hud_segment`, under the
existing 0.5 ms draw budget and primitive caps.

### Entities: a host-built snapshot

`UmiEntity` gains health, max health, armour points, team, width, height, velocity,
flags, `is_player` and the interpolated `render_position`. The client publishes a
snapshot of at most **256** entities (nearest first) from the game thread; the
render thread reads it through `world_entities_snapshot(out, capacity, count)` from
render-thread callbacks (`FRAME`/`PRE_RENDER` intercepts, widget draws, camera
layers). Tick fields are at most one game tick old; `render_position` is for the frame
being drawn. The game-thread visitor is unchanged and fills the same new fields.

A host call was chosen over a borrowed slice in `FRAME` so that `UmiFrame` stays a
plain `Copy` payload without pointers (it crosses to the observe worker).

### Depth in post passes

The prelude gains `@binding(3) umi_depth: texture_depth_2d` and `umi.depth`
(`near`, `far`, `valid`), plus a `umi_linear_depth(uv)` helper. Shaders written for
1.0 never reference them and keep validating; the resource whitelist grows by one.

### Flight and game mode from a physics layer

`UMI_PHYSICS_FLYING` becomes writable. `UmiPlayerPhysics` gains read-only
`allow_flying` (a flag bit) and `fly_speed`, and a writable `game_mode`. A changed
flying bit goes through the vanilla path (`report_flight`, one `C13 Player
Abilities`). A changed game mode is a local prediction applied like the server's
own change; no packet exists for it and the next server packet wins.

### Owner decisions (2026-11-23)

The project owner chose the permissive options, knowingly, against the narrower ones
proposed:

- the `camera` permission is informational only: camera mods are **not** held off
  by server-compatible mode, and the eye is not clipped against blocks;
- the official entity-snapshot example lists every entity with its name and health,
  as requested; this is a debugging view, not a feature of the client (spec §10
  still governs the core);
- `set_flying` is always applied, even without `allow_flying`; the server stays the
  authority and corrects or kicks;
- `set_game_mode` exists, as a documented prediction.

## Consequences

- 1.0 mods load unchanged on 1.1; a 1.1 mod is refused by a 1.0 client with the
  existing version message.
- `FRAME` and external widgets move later in the frame (same frame, same order
  relative to the HUD).
- The physics chain is refactored; its tests are the regression net (ADR-0076: no
  change of the trajectory with no mod).
- Server-compatible mode still holds `movement` mods, so flight and game-mode writes
  are off there; camera mods are not.
