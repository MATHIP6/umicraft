# 0066. Motion blur

Status: accepted

## Context

The built-in mods (ADR-0065) have no mod that changes the picture. The owner asked
for a motion blur with three versions: V1 the classic one, V2 and V3 more modern
and better thought out. It must be cheap and must not make the player ill or hide
what happens.

The renderer draws the world, the hand, the overlays and the HUD in one pass
(`Renderer::draw`). The menu blur already knows how to send the world to a texture
(`SceneBlur`).

## Decision

- **A mod that holds only settings.** `motion_blur` (category Visual, off by
  default) has `version` (`v1`, `v2`, `v3`, default `v3`) and `strength` (0.1 to
  1.0). The client reads them every frame (`ModsRuntime::motion_blur`) and hands
  them to `Renderer::set_motion_blur`. The mods crate stays free of the GPU.
- **A pass between the world and the hand.** While the blur is on and the camera
  moves, the world goes to the `scene` texture, `MotionBlur::run` blurs it into one
  of two pictures, and the next pass copies that picture to the screen and draws
  the hand, the overlays and the HUD sharp over it, with the depth buffer loaded so
  the hand still tests against the world. The copy has its own layout because that
  pass has the depth buffer as an attachment and cannot also read it.
- **The three versions.** V1 mixes the picture with the previous output (a trail
  that fades). V2 reprojects each pixel with its depth and the previous camera and
  averages ten taps along the line. V3 uses a number of taps that follows the length
  of the line, a tent weight, and gives little weight to a tap much closer than the
  pixel, so an edge does not smear over what is behind it.
- **A planner without a GPU** (`motion_blur::Planner`) decides each frame and holds
  the tests. A frame has no blur, and nothing is drawn or allocated, when the blur
  is off, when a menu blurs the game, on the first frame, for a frame shorter than
  0.1 ms or longer than 0.2 s (a pause or a hitch), after a jump of the camera (more
  than 4 blocks, a turn of more than a radian, or a change of the field of view of
  more than a fifth), and when the camera stands still. A jump or a still frame also
  forgets the history, so V1 never mixes with a stale picture.
- **Frame-rate independent.** V1 keeps `persistence ^ (seconds * 60)` of the
  previous picture; V2 and V3 cover `exposure / seconds` of the motion of a frame
  (an exposure of 1/120 s to 1/30 s by strength), and V3 smooths that over 50 ms.
  The blur has a longest length, 3 to 8 % of the screen height, so a fast turn
  smears but never washes the screen out.
- **The body of the player stays sharp in the third person.** The body is part of
  the picture but moves with the camera, not with the world, so reprojecting its
  pixels with the previous camera would smear the skin every time the camera orbits.
  The client tells the renderer the eye of the player while the camera is not on it
  (`WorldView::third_person_body`, `Renderer::set_motion_blur_body`). The shaders
  rebuild the world position of each pixel from its depth and leave unblurred what is
  inside a vertical capsule around the body (1.1 blocks of radius, wider than the
  player for the arms, a held tool and the shadow, fading out over the outer quarter).
  V1 keeps no trail there, V2 and V3 give it no motion. A change between the first and
  the third person counts as a jump of the camera.
- **The hand is in the blurred picture.** The first-person hand and the item in it
  are drawn into the picture the blur reads (before it, not after), with their depth
  range squeezed below `2 * HAND_DEPTH_RANGE`, which no world pixel reaches. The hand
  moves with the camera, so the motion of the world says nothing about it: where it
  is, or where it just was, the picture mixes with the previous one (the rule of V1)
  and the alpha of the picture keeps how much of the hand is left in a pixel, so its
  trail fades out. This only runs while the hand moves (a swing, an item being
  raised: the window compares the pose of the hand from one frame to the next and
  tells the renderer), and a still camera is blurred anyway while it does. A hand
  that stands still costs nothing. The overlays and the HUD stay sharp.
- **Built when first needed.** The pipelines are built when the blur is first
  switched on and the pictures when the first blurred frame comes; switching it off
  frees the pictures. A resize forgets them.

## Consequences

- Measured in `docs/perf-log.md` (2026-11-09): about +0.25 to +0.45 ms per frame at
  1280x720 on a strong GPU, none while the camera stands still. It scales with the
  pixel count and was not measured on a weak GPU.
- The blur follows the camera only. A moving entity is not blurred by its own
  motion, and the camera shake of a hit is blurred like any camera motion, within
  the longest length. Whatever stands within the radius of the body in the third
  person, a wall next to the player say, is spared too.
- `offscreen_motion_blur` (opt-in, needs a GPU) checks that every version blurs a
  turning camera and leaves a still camera, a jump and the HUD alone, and keeps the
  brightness.
- The depth texture gained `TEXTURE_BINDING` usage for the blur to sample it.
