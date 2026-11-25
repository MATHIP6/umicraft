# ADR-0097 — Modding API 1.4: combat facts, teams, world drawing and the entity mask

Status: accepted (2026-11-25)

## Context

The owner listed what API 1.3 cannot do for an entity-overlay mod:

- **occlusion**: there is no visibility test, no depth per entity and no control of
  the depth test, so everything a mod draws is "through blocks";
- **combat state**: no hurt time, death time, spectator, tab-list presence, potion
  effects or held item; a "hurt" colour can only be guessed from health;
- **teams**: the local player's team and the formatted name (with its colour codes)
  are missing, so team mates cannot be told apart reliably;
- **HUD primitives**: no 3D polygon, circle, clipping, nor lines thinner than 0.5 GUI
  pixel; glow, outline and blur of entities need an entity render the full-screen
  post passes cannot see.

The owner chose (2026-11-25): the entity effects are a **mask** the client renders and
the mod's own post pass turns into an outline, a glow or a blur (no built-in effect),
and all of it ships as **one** version, 1.4.

## Decision

API **1.4**, append-only as in ADR-0087. The exact types are frozen in
`docs/tasks/api-1.4/README.md`.

### Entity facts

`UmiEntity` grows at its end: hurt time, death time, held item and its damage, the
formatted display name, the team colour, the tab-list ping, the UUID version and up
to 8 potion effects. New flag bits: spectator, in the tab list, visible, team mate.
Existing bits and fields keep their meaning; a 1.1 mod reads a shorter struct.

- The client does not decide what a **bot** is. It gives the facts anti-bot checks
  use (not in the tab list, UUID version 2, ping) and the mod decides.
- **Visible** is a line-of-sight test the game thread runs each tick from the
  player's eye to three points of the entity's box (eye height, centre, feet), against
  the blocks a ray stops at (the targeting rules of `raycast`), up to 128 blocks. It is
  the player's eye, not a mod camera: the game thread does not know where a camera
  layer put the eye, and a free camera is not where the player sees from.
- **Team colour** is the team's colour index when the server set one, else the last
  colour code of the team prefix (what 1.8 draws name tags with), else none.
- Potion effects of other entities are now kept: a 1.8 server sends `Entity Effect`
  for every tracked living entity, the client used to drop all but its own.
- `world_local_player` fills a `UmiEntity` for the local player (team, display name,
  effects…), from the same render-thread snapshot as `world_entities_snapshot`.

### HUD primitives

`hud_circle` (filled or a ring), `hud_polygon` (filled convex, up to 64 points) and
`hud_clip_push`/`hud_clip_pop` (up to 8 nested; free items only, the stacked lines are
not clipped). `hud_segment` accepts 0.05 to 8 GUI pixels; a band thinner than one
physical pixel is drawn one pixel wide with its opacity scaled by its width, the usual
coverage approximation, since the UI has no anti-aliasing.

### World layers

`register_world_layer(key, draw, user)` declares a callback run on the render thread
once per frame in a world, after the camera layers, like a widget's draw. During it,
`world_line`, `world_box` and `world_polygon` add geometry in world coordinates, and
`world_highlight` marks an entity for the mask. Each primitive chooses with
`UMI_WORLD_THROUGH_WALLS` whether it is depth-tested against the world (it never writes
depth); `UMI_WORLD_FILL` fills a box's faces instead of drawing its edges. The
geometry is drawn after the world and before the hand, translucent, and capped per
mod and per frame. A layer can be attached to a module (`UMI_ATTACH_WORLD_LAYER`).

### Entity mask

Every highlighted entity is drawn once more, after the world, into a colour texture
the size of the frame: its model with a flat colour (the texture's alpha cutoff is
kept, so the shape is the skin's), depth-tested against the world or not
(`UMI_WORLD_THROUGH_WALLS`). Post passes get it as binding 4, `umi_entity_mask`
(filterable, so a pass may blur it), and `umi.mask` tells whether anything was marked.
Outline, glow and Gaussian blur are then a few lines of WGSL in the mod's pass, which
`mods/examples/entity_glow` shows. A mask nobody reads costs one extra draw of the
marked entities; with no mark the pass is skipped.

### No new permission

Drawing and highlighting change what the player sees, not what the server receives,
like the HUD widgets and the camera (ADR-0094). Server-compatible mode does not hold
them off.

## Consequences

- A 1.4 mod is refused by a 1.3 host; 1.0 to 1.3 mods load unchanged.
- Shaders written for 1.0 to 1.3 still validate: the prelude only gains binding 4 and
  a field at the end of `UmiPost` (96 to 112 bytes).
- Everything a mod can draw through walls it could already infer from the snapshot
  positions; the mask makes such overlays look better, it does not reveal more. The
  official example only marks entities with the depth test on (spec §10).
- The visibility flag is one tick old and from the player's eye; a mod that wants the
  exact pixels uses the mask with the depth test on.
