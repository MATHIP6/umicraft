# The render camera

API 1.1, ADR-0094. A mod can move the camera a frame is drawn from without moving
the player: an orbit, a fixed shot, a smoothed or lifted view.

## Declaring it

```toml
[load]
wraps = ["render_camera"]      # or: replaces = ["render_camera"]

[permissions]
camera = true
```

The rules are those of `player_physics` (`world.md`). Any number of mods may wrap the
service; they nest in load order, the first loaded outermost. At most one mod may
replace it, and a replacing layer sits inside every wrapper.

`camera` is shown in the mods menu. Server-compatible mode does **not** hold camera
mods off: the camera changes what is drawn, not what the server sees (owner decision,
ADR-0094). The host does not stop the eye from going through blocks either.

## Writing a layer

```rust
cx.wrap_camera(|camera| {
    camera.next()?;                       // let the client (and inner layers) place it
    let [x, y, z] = camera.state().eye();
    camera.state_mut().set_eye([x, y + 2.0, z]);
    Ok(())
})?;
```

C: `register_camera(me, UMI_SERVICE_WRAP, step, user)` with
`UmiStatus step(UmiCameraCx *cx, UmiRenderCamera *camera, const UmiCameraInput *input, void *user)`,
and `camera_next(cx)` to run what is inside.

- **When.** The layer runs on the render thread once per drawn frame in a world. That
  is after the client placed its camera (eye, angles, third person, zoom, sprint and
  flight FOV, view bobbing, hurt tilt), and before the camera reaches the GPU. The
  budget is 1 ms, counted on the layer's own time; failures fail the mod, as for
  physics layers. With no layer enabled, the frame is drawn exactly as without mods.
- **The camera** (`CameraState`, C `UmiRenderCamera`) has these fields:
  - `eye`, in blocks, in `f64`;
  - `yaw` and `pitch`, in Minecraft degrees (yaw clockwise from south, pitch
    positive down);
  - `fov_y`, the effective vertical field of view in degrees;
  - `view_bob`, a column-major camera-space matrix;
  - `aspect`, read-only.

  After the layers run, the host:
  - restores the read-only fields;
  - replaces non-finite values with the client's;
  - clamps the pitch to ±90 and the field of view to 1..=170.

  A field of view set by a layer is final: no sprint or flight factor is applied on
  top. A layer that changes nothing leaves the frame as drawn without it.
- **The input** (`CameraInput`, C `UmiCameraInput`) is the same aggregated intent the
  client's own view uses:
  - `look_dx`/`look_dy`: degrees the mouse turned the view this frame;
  - `forward`/`strafe`: as in a physics layer;
  - `up`: +1 jump, -1 sneak;
  - the jump and sneak flags;
  - `frame_seconds`.

  There is no raw key state. The mouse still turns the player and the keys still move
  it, unless the layer captures the input (below).
- **The player does not move.** The camera feeds nothing back into the game. The
  player's position, yaw and pitch, the packets and the built-in mods are untouched.
  `FRAME.position`, `yaw` and `pitch` also come from the player, not the camera
  (since 1.1 this is true in the third person too). To move the player, wrap
  `player_physics` as well (`movement = true`).

## Capturing the input (API 1.3)

`Camera::capture_input()` (C: `camera_capture(cx)`, its own `cx` only, else `DENIED`)
gives the movement keys and the mouse look to the camera, for a free camera
(ADR-0096). While a layer asks for it:

- the player gets no movement key (forward, strafe, jump, sneak, sprint are
  released) and the mouse does not turn it; attack, use and every other key still
  act;
- `CameraInput` still reports the keys and the look, so the layer moves the camera
  with them.

The request lasts one frame and applies to the next frame's input: call it on every
frame the camera wants the input. When the layer stops asking, or the mod is switched
off or fails, the player gets the input back. The world only exists where the server
sent chunks, around the player. The example `mods/examples/free_cam` flies the camera
this way.

## `PRE_RENDER`

Event 13, on the render thread, read-only and not cancellable. Payload
`UmiPreRender { header, camera: UmiRenderCamera }`. It fires once per drawn frame in
a world, after the camera layers, with the final camera. `FRAME` follows in the same
frame, with the matrix of that camera, so `world_to_screen` matches what is drawn.

## Example

`mods/examples/camera_orbit` has a key binding (default `KeyO`). Once pressed, the
camera circles the player at a distance set by a slider, looking back at the player.
