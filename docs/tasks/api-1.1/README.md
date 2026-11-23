# API 1.1 — camera, projection, entity snapshot, depth, flight

Status: **in progress** (started 2026-11-23). Design: ADR-0094. Base: API 1.0
(`docs/tasks/f3/README.md`, ADR-0086, ADR-0087).

| # | Phase | Acceptance | State |
|---|---|---|---|
| 1.1.0 | ADR-0094 and this plan | ADR accepted | done |
| 1.1.1 | `umicraft-modapi-sys`: every type below, `UMI_API_VERSION = 1.1`, host functions appended (stubs answering `UNSUPPORTED` until their phase) | layout tests of the new fields; a 1.0 mod still loads, a 1.1 mod is refused by a 1.0 host | done |
| 1.1.2 | Flight and game mode from a physics layer | a layer toggles flying: the player changes and one `PlayerAbilities` is queued | |
| 1.1.3 | Depth in post passes | example pass fades by distance; undeclared resources still refused | |
| 1.1.4 | Entity metadata and the render-thread snapshot | snapshot equals the game-thread query of the same tick; example lists names and health | |
| 1.1.5 | Camera facts in `FRAME`, `world_to_screen`, `Anchor::Screen`, `hud_outline`, `hud_segment`; `FRAME` and external widgets after the final camera | axis projection test; example box follows a world point | done |
| 1.1.6 | `render_camera` service, `PRE_RENDER`, generic service chain | orbit example; player state bit-identical with a camera layer | |
| 1.1.7 | Docs, template and examples on `api_version = "1.1"`, header | fmt, clippy `-D warnings`, workspace tests | |

## Frozen interface (spec §14 rule 11)

Every addition is appended after the last 1.0 field. Matrices are column-major
`[[f32; 4]; 4]` (`glam::Mat4::to_cols_array_2d`). Angles are Minecraft degrees as in
1.0 (yaw clockwise from south, pitch positive down).

### Version

`UMI_API_VERSION = UmiVersion { major: 1, minor: 1 }`.

### Events (`events.rs`)

- `UmiEventId::PRE_RENDER = 13`, payload `UmiPreRender { header, camera: UmiRenderCamera }`,
  render thread, not cancellable, not mutable. Fires once per drawn frame in a world,
  after the camera layers, before the camera uniform is written.
- `UmiFrame`, appended after `sprinting`:

  ```rust
  pub eye: [f64; 3],              // camera eye, blocks
  pub render_origin: [f64; 3],    // integer origin the GPU positions are relative to
  pub view_proj: [[f32; 4]; 4],   // clip = view_proj * (point - render_origin, 1)
  pub viewport: [u32; 2],         // framebuffer size, physical pixels
  pub gui_scale: f32,             // physical pixels per GUI pixel
  pub fov_y: f32,                 // effective vertical FOV, degrees (zoom, sprint, flight)
  pub aspect: f32,
  pub view_bob: [[f32; 4]; 4],    // camera-space bob, already inside view_proj
  ```

### Services (`services.rs`)

- `UMI_PHYSICS_FLYING` (8) becomes **writable**.
- `UMI_PHYSICS_ALLOW_FLYING: u32 = 32`, read-only.
- `UmiPlayerPhysics`, appended after `fall_distance`: `fly_speed: f32` (read-only),
  `game_mode: u32` (writable; 0 survival, 1 creative, 2 adventure, 3 spectator;
  `UMI_GAME_MODE_*` constants).
- `UmiEntity`, appended after `name`:

  ```rust
  pub health: f32,               // 0 when not living
  pub max_health: f32,           // 0 when not living
  pub armor: f32,                // armour points of the visible equipment
  pub team: UmiStr,              // scoreboard team name, empty when none
  pub width: f32,
  pub height: f32,
  pub velocity: [f64; 3],        // position change over the last tick, blocks/tick
  pub flags: u32,                // UMI_ENTITY_*
  pub is_player: u8,
  pub render_position: [f64; 3], // interpolated for the frame; = position on the game thread
  ```

  `UMI_ENTITY_ON_FIRE = 1`, `UMI_ENTITY_SNEAKING = 2`, `UMI_ENTITY_SPRINTING = 4`,
  `UMI_ENTITY_USING_ITEM = 8`, `UMI_ENTITY_INVISIBLE = 16`, `UMI_ENTITY_LIVING = 32`
  (health fields are meaningful). Strings are borrowed for the call.
- Camera (`camera.rs`, new module):

  ```rust
  #[repr(C)] pub struct UmiRenderCamera {
      pub size: u32,
      pub flags: u32,                // reserved, 0
      pub eye: [f64; 3],             // writable
      pub yaw: f32,                  // writable
      pub pitch: f32,                // writable, clamped to ±90 by the host
      pub fov_y: f32,                // writable, effective, degrees, clamped 1..=170
      pub aspect: f32,               // read-only
      pub view_bob: [[f32; 4]; 4],   // writable
  }
  #[repr(C)] pub struct UmiCameraInput {
      pub size: u32,
      pub flags: u32,                // UMI_INPUT_JUMP / UMI_INPUT_SNEAK as for physics
      pub look_dx: f32,              // degrees turned this frame, + right
      pub look_dy: f32,              // degrees, + down
      pub forward: f32,              // -1..=1
      pub strafe: f32,               // -1 (right)..=1 (left), as physics
      pub up: f32,                   // -1..=1 (jump +, sneak -)
      pub frame_seconds: f32,
  }
  #[repr(C)] pub struct UmiCameraCx { _private: [u8; 0] }
  pub type UmiCameraFn = extern "C" fn(cx: *mut UmiCameraCx, camera: *mut UmiRenderCamera,
      input: *const UmiCameraInput, user: *mut c_void) -> UmiStatus;
  ```

### UI (`ui.rs`)

`UMI_HUD_ANCHOR_SCREEN: u32 = 9`: offsets are absolute GUI pixels from the top-left
corner, scale is forced to 1, the widget is not shown in the HUD editor.

### Host functions, appended to `UmiHost` in this order

```rust
pub register_camera: extern "C" fn(me, mode: UmiServiceMode, step: Option<UmiCameraFn>, user) -> UmiStatus,
pub camera_next: extern "C" fn(cx: *mut UmiCameraCx) -> UmiStatus,
pub world_entities_snapshot: extern "C" fn(me, out: *mut UmiEntity, capacity: usize, count: *mut usize) -> UmiStatus,
pub hud_outline: extern "C" fn(hud, x, y, width, height, color: u32, alpha: f32) -> UmiStatus,
pub hud_segment: extern "C" fn(hud, x1, y1, x2, y2, color: u32, alpha: f32, thickness: f32) -> UmiStatus,
```

- `register_camera`: init only; `DENIED` without `camera = true` or without
  `render_camera` in `wraps`/`replaces`; `CONFLICT` for a second layer.
- `camera_next`: as `physics_next`.
- `world_entities_snapshot`: render thread inside a mod callback, else
  `UNSUPPORTED`; writes `min(capacity, n)` entries, `*count = n` (total available,
  ≤ 256); `UNAVAILABLE` outside a world. Strings valid until the callback returns.
- `hud_outline` / `hud_segment`: count against the existing primitive cap; thickness
  clamped to 0.5..=8 GUI pixels.

### Manifest (`umicraft-modhost`)

- Service id `render_camera` added to `KNOWN_SERVICES`.
- `[permissions] camera = true`, shown as `camera` in the mods menu, **not** part of
  `alters_game` (owner decision, ADR-0094).

### `umicraft-modapi` (safe side)

- `Frame`: `eye()`, `render_origin()`, `view_proj()`, `viewport()`, `gui_scale()`,
  `fov_y()`, `aspect()`, `view_bob()`, `world_to_screen([f64; 3]) -> Option<[f32; 2]>`
  (GUI pixels; `None` behind the camera or outside the viewport). Plain getters: a
  mod built against 1.1 exports version 1.1, and a 1.0 host refuses it. The
  trampolines also refuse a payload smaller than the 1.1 struct.
- `PreRender` event view with `camera() -> CameraState`.
- `CameraState` (getters/setters), `Camera<'a>` (`state()`, `state_mut()`,
  `next()`), `CameraInput` getters, `cx.wrap_camera(..)`, `cx.replace_camera(..)`.
- `PlayerState::set_flying`, `allow_flying()`, `fly_speed()`, `game_mode()`,
  `set_game_mode(GameMode)`; `GameMode` enum.
- `Entity`: `health()`, `max_health()`, `armor()`, `team()`, `width()`,
  `height()`, `velocity()`, `flags()`, `is_player()`, `render_position()`, each
  `Option`-free with 1.0 fallbacks documented; `world().snapshot(&mut buf)`.
- `Anchor::Screen`; `Hud::outline`, `Hud::segment`.
- Every new host function is used only after checking `UmiHost::size`.

### Post prelude

```wgsl
struct UmiPost {
    resolution: vec2<f32>, time: f32, frame: u32,
    params: array<vec4<f32>, 4>,
    depth: vec4<f32>,          // x near, y far, z 1.0 when umi_depth is valid
};
@group(0) @binding(3) var umi_depth: texture_depth_2d;
fn umi_linear_depth(uv: vec2<f32>) -> f32 // blocks from the eye
```

## Rules for the contributors

- Do not touch an existing `tests/abi.rs` assertion; only add.
- One build directory per worktree (spec §14 rule 12).
- New `.rs` files start with `// SPDX-License-Identifier: MIT OR Apache-2.0`.
- Physics: with no mod, trajectories stay bit-identical (ADR-0076).
