# Drawing in the world

API 1.4, ADR-0097. A **world layer** draws lines, boxes and polygons in world
coordinates every frame, and marks entities for the [entity
mask](post.md#the-entity-mask-api-14) of the post passes. No permission is needed: it
changes what the player sees, not what the server receives, like a HUD widget.

## Declaring a layer

`InitCx::world_layer(key, |frame| ...)` (C: `register_world_layer(me, key, draw,
user)`) during `init`; at most 4 per mod. The host calls the closure on the render
thread once per frame in a world, after the camera layers and with the frame's entity
snapshot lent, so it may call `World::snapshot` and `World::local_player`. A layer can
be tied to a module with `Card::attach_world_layer` (C: `module_attach` with
`UMI_ATTACH_WORLD_LAYER`): it draws only while the module is on.

The draw shares the 0.5 ms budget of the HUD widgets. A draw that fails, panics or
overruns too often fails the mod; a mod that fails during a frame leaves nothing of
that frame on screen.

## What a frame takes

Positions are world coordinates in blocks, colours `0xRRGGBB` with an opacity from 0
to 1. `Depth::Tested` (no flag) lets the world hide a shape like it hides the block
outline; `Depth::ThroughWalls` (`UMI_WORLD_THROUGH_WALLS`) draws it over the blocks in
front of it.

| Rust | C | Draws |
|---|---|---|
| `line(from, to, color, alpha, thickness, depth)` | `world_line` | A line `thickness` GUI pixels wide (0.05 to 8; under one screen pixel, one pixel wide and fainter). |
| `outline_box(a, b, color, alpha, depth)` | `world_box`, no `UMI_WORLD_FILL` | The 12 edges of the box between two corners, one GUI pixel wide. |
| `filled_box(a, b, color, alpha, depth)` | `world_box` with `UMI_WORLD_FILL` | Its 6 faces, translucent. |
| `polygon(&points, color, alpha, depth)` | `world_polygon` (`x0, y0, z0, …`) | A filled convex polygon of 3 to 64 points. |
| `highlight(id, color, alpha, depth)` | `world_highlight` | Nothing by itself: marks the entity for the mask a post pass reads. |

The geometry is drawn after the translucent terrain and before the hand, never
writing depth: the hand stays in front, and water or glass in front of a tested shape
tints it. The order is: the tested shapes, the tested lines, then the shapes and the
lines through walls, so what is drawn through walls stays on top.

`WorldCx` is best effort like `Hud`: a refused call skips the rest of that frame's
calls. A call is refused for a non-finite number (`INVALID_ARGUMENT`), a bad point
count, or a full frame (`DENIED`): a mod may add, per frame and over all its layers,
4096 lines (a box's edges count 12), 4096 triangles (a filled box counts 12, a polygon
of `n` points `n - 2`) and 256 marks. Outside its draw, `UmiWorldCx` answers `DENIED`.

```rust
let world = cx.world();
cx.world_layer("boxes", move |frame| {
    let _ = world.snapshot(|entity| {
        let [x, y, z] = entity.render_position();
        let half = f64::from(entity.width()) / 2.0;
        let top = y + f64::from(entity.height());
        frame.outline_box([x - half, y, z - half], [x + half, top, z + half], 0xFFFFFF, 1.0, Depth::Tested);
        true
    });
})?;
```

Use `render_position`: it is where the frame draws the entity, so the box stays on it
between ticks. `mods/examples/entity_glow` marks the entities near the player for its
outline and glow pass.
