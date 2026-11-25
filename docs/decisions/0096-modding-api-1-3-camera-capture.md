# ADR-0096 — Modding API 1.3: a camera layer may capture the input (free camera)

Status: accepted (2026-11-24)

## Context

API 1.1 (ADR-0094) lets a `render_camera` layer place the camera anywhere and hands
it the player's movement intent and mouse look. But the same input still moves and
turns the player, so a free camera drags the player along: the body walks where the
camera flies and the head turns with every mouse move, which the server sees.
Freezing the player through a `player_physics` layer would need the `movement`
permission (badge, held off by server-compatible mode) and still could not stop the
look, whose yaw and pitch are read-only.

## Decision

API **1.3** appends one host function, `camera_capture(cx)`, callable by a running
`render_camera` layer only (its own `cx`, else `DENIED`). While a layer asks for it:

- the player gets an input with no movement key held (forward, strafe, jump,
  sneak and sprint released), as if the player let go of the keys; attack, use and
  every other key still act;
- the mouse look does not turn the player;
- the camera input (`UmiCameraInput`) still reports the keys and the look, so the
  layer flies the camera with them.

The request lasts **one frame**: inputs are published before the camera is placed,
so a request made while placing frame N applies to the input of frame N+1, and a
layer renews it every frame it wants the input. When no layer asks, or the mod is
switched off or fails (its layer stops running), the player gets the input back on
the next frame, with no state to clean up and nothing a mod can leave stuck.

No new permission: `camera` already covers the service, and the capture only takes
input away from the player. The server sees a player standing still, which any
player can do; it is not held off by server-compatible mode, like the camera
itself (owner decision of ADR-0094).

## Consequences

- A free camera is a wrapper that calls `next`, flies its own eye from the input
  and calls `capture_input` (example `mods/examples/free_cam`).
- The world only exists where the server sent chunks, around the player.
- While a layer keeps the eye away from where the client put it (more than 0.05
  block, the third person's own threshold), the window tells the game thread, which
  draws the local body (with its skin) and hides the hand, as in the third person.
  Any camera layer benefits, the orbit too.
- The entities of a frame are built after the camera layers, against the final
  render origin. Built before (as in 1.1), they were offset by however far a layer
  moved the camera and seemed to follow it.
- The first frame after the key is pressed still goes to the player (one frame of
  input, at most a few hundredths of a block).
- A 1.3 mod is refused by a 1.2 host; older mods load unchanged.
