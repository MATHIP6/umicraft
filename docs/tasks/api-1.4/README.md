# API 1.4 — combat facts, teams, HUD shapes, world layers and the entity mask

Status: **in progress** (2026-11-25). Design: ADR-0097. Base: API 1.3 (ADR-0096).

| # | Phase | Acceptance | State |
|---|---|---|---|
| 1.4.0 | ADR-0097 and this plan | ADR accepted | done |
| 1.4.1 | `umicraft-modapi-sys`: types below, `UMI_API_VERSION = 1.4`, host functions appended (stubs answering `UNSUPPORTED` until their phase) | layout tests; a 1.4 mod is refused by a 1.3 host | |
| 1.4.2 | Entity facts: combat, effects of every entity, teams, visibility, local player | facts tests for each field; snapshot equals the query | |
| 1.4.3 | HUD: circle, polygon, clipping, thin lines | draw tests, UI tessellation tests | |
| 1.4.4 | World layers: registration, primitives, caps, render pipelines (depth on and off) | host tests of caps and refusals; renderer pipeline test | |
| 1.4.5 | Entity mask: highlights, mask pass, post binding 4 and `umi.mask` | highlight routing tests; 1.1 shaders still validate | |
| 1.4.6 | `umicraft-modapi`: entity accessors, HUD shapes, `WorldCx`; example `entity_glow`; docs; template on 1.4; header | fmt, clippy `-D warnings`, workspace tests | |

## Frozen interface (spec §14 rule 11)

### Version

`UMI_API_VERSION = UmiVersion { major: 1, minor: 4 }`.

### Entities (`services.rs`)

`UmiEntity`, appended after `render_position` (offset 168, size 280):

```rust
pub hurt_time: u32,           // ticks left of the hurt flash, 10 → 0
pub death_time: u32,          // ticks into the death fall, 0 while alive
pub held_item: i32,           // item id in the main hand, -1 when empty
pub held_damage: i32,         // its damage / metadata value
pub display_name: UmiStr,     // player: team prefix + name + suffix (§ codes kept);
                              // else the name tag; empty when none
pub team_color: u32,          // 0xRRGGBB, or UMI_NO_COLOR
pub ping: i32,                // tab-list latency in ms, -1 when not listed
pub uuid_version: u32,        // version nibble of the UUID, 0 when unknown
pub effect_count: u32,        // used entries of `effects`
pub effects: [UmiEffect; 8],  // potion effects, in id order
```

```rust
#[repr(C)]
pub struct UmiEffect {
    pub id: u8,          // 1.8 potion id (1 speed … 23 saturation)
    pub amplifier: u8,   // 0 for level I
    pub flags: u8,       // UMI_EFFECT_HIDE_PARTICLES = 1
    pub reserved: u8,
    pub duration: i32,   // ticks left as the server sent it (32767: "**:**")
}
pub const UMI_MAX_ENTITY_EFFECTS: usize = 8;
pub const UMI_NO_COLOR: u32 = 0xFFFF_FFFF;
pub const UMI_ENTITY_SPECTATOR: u32 = 64;   // a player in spectator mode
pub const UMI_ENTITY_IN_TAB_LIST: u32 = 128;
pub const UMI_ENTITY_VISIBLE: u32 = 256;    // line of sight from the player's eye
pub const UMI_ENTITY_TEAMMATE: u32 = 512;   // same scoreboard team as the local player
```

### Host functions (appended to `UmiHost`, after `camera_capture`)

```rust
world_local_player: extern "C" fn(me, out: *mut UmiEntity) -> UmiStatus,
hud_circle: extern "C" fn(hud, x: f32, y: f32, radius: f32, color: u32, alpha: f32, thickness: f32) -> UmiStatus,
hud_polygon: extern "C" fn(hud, points: *const f32, count: usize, color: u32, alpha: f32) -> UmiStatus,
hud_clip_push: extern "C" fn(hud, x: f32, y: f32, width: f32, height: f32) -> UmiStatus,
hud_clip_pop: extern "C" fn(hud) -> UmiStatus,
register_world_layer: extern "C" fn(me, key: UmiStr, draw: Option<UmiWorldDrawFn>, user: *mut c_void) -> UmiStatus,
world_line: extern "C" fn(cx: *mut UmiWorldCx, x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64,
                          color: u32, alpha: f32, thickness: f32, flags: u32) -> UmiStatus,
world_box: extern "C" fn(cx, min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64,
                         color: u32, alpha: f32, flags: u32) -> UmiStatus,
world_polygon: extern "C" fn(cx, points: *const f64, count: usize, color: u32, alpha: f32, flags: u32) -> UmiStatus,
world_highlight: extern "C" fn(cx, entity: i32, color: u32, alpha: f32, flags: u32) -> UmiStatus,
```

- `world_local_player`: as `world_entities_snapshot` (render thread, inside one of the
  mod's callbacks, else `UNSUPPORTED`; `UNAVAILABLE` outside a world); strings valid
  until the callback returns.
- `hud_circle`: `thickness <= 0` fills the disc, else a ring that wide (clamped to 8).
  `hud_polygon`: 3 to 64 points as `x0, y0, x1, y1, …`, convex, filled.
  `hud_clip_push`: clips the following free items (cells, labels, rectangles,
  segments, circles, polygons) to the rectangle, intersected with the enclosing clip;
  `DENIED` past 8 nested; `hud_clip_pop` with none open answers `INVALID_ARGUMENT`.
  Clips still open when the draw returns are closed.
- `hud_segment` thickness range becomes `0.05..=8`.
- `register_world_layer`: init only, at most 4 per mod, `CONFLICT` for a used key.
- `world_*`: only inside the layer's draw (`DENIED` otherwise, like `hud_*`).
  Thickness in GUI pixels, clamped `0.05..=8`. `world_polygon`: 3 to 64 points as
  `x0, y0, z0, …`, convex. Caps per mod per frame: 4096 lines (a box's edges count
  12), 4096 triangles (a filled box counts 12, a polygon `count - 2`), 256 highlights;
  past a cap `DENIED`, nothing half added. Non-finite numbers: `INVALID_ARGUMENT`.
- `world_highlight`: unknown entity ids are kept and simply match nothing. The last
  colour given to an entity in a frame wins (mods in load order).

```rust
#[repr(C)] pub struct UmiWorldCx { _private: [u8; 0] }
pub type UmiWorldDrawFn = extern "C" fn(world: *mut UmiWorldCx, user: *mut c_void) -> UmiStatus;
pub const UMI_WORLD_THROUGH_WALLS: u32 = 1;
pub const UMI_WORLD_FILL: u32 = 2;
pub const UMI_ATTACH_WORLD_LAYER: u32 = 3;
```

### Post passes

The prelude gains `@group(0) @binding(4) var umi_entity_mask: texture_2d<f32>;`
(filterable, same size as `umi_scene`; rgb the highlight colour, a its alpha, 0 where
nothing is marked) and `UmiPost` gains `mask: vec4<f32>` at offset 96 (112 bytes):
`x` = 1.0 when at least one entity is marked this frame, `y` = how many.

### Limits

| What | Limit |
|---|---|
| World layers per mod | 4 |
| Lines per mod per frame | 4096 |
| Triangles per mod per frame | 4096 |
| Highlights per mod per frame | 256 |
| Polygon points (HUD and world) | 64 |
| Nested HUD clips | 8 |
| Effects per entity | 8 |
| Visibility ray | 128 blocks |
