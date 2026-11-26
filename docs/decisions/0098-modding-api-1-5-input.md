# ADR-0098 — Modding API 1.5: the attack and use input

Status: accepted (2026-11-26)

## Context

API 1.4 lets a mod count the clicks of the previous frame (`FRAME.left_clicks`) and
inject raw packets (`packet_send`), but not read whether the attack or use button is
held, nor press them through the client's own input path. A raw `Animation` +
`Use Entity` pair skips everything the client does around a click: the miss delay,
the item in use that swallows presses, the arm swing, the local block break or
placement, and the packet order of `Minecraft.runTick`. A mod also cannot see what
the crosshair points at (`objectMouseOver`), the swing progress or what the held
item does when used.

A mod author listed the missing pieces; the owner accepted the design below
(2026-11-26), including that the new permission is **not** held off by
server-compatible mode.

## Decision

API **1.5**, append-only as in ADR-0087. The exact types are frozen in
`docs/tasks/api-1.5/README.md`.

### Permission `input`

`[permissions] input = true` gates the `INPUT` event (subscribing without it answers
`DENIED`, like the packet events) and the three input host functions (`DENIED`). The
mods menu shows it as a badge. Server-compatible mode does **not** hold an `input` mod
off: the owner's choice. A synthetic press is a press of the player's own binding and
obeys every rule a real one does, so what reaches the server is what a player
clicking could send. A player who must follow a server's rules turns such a mod off
in the menu, where the badge shows it.

### Event `INPUT` (id 16)

Render thread, once per frame in a world, right after `FRAME`; neither cancellable
nor mutable. It carries:

- the held state of the attack and use **bindings** (whatever key or button they are
  bound to) and the presses the game has not consumed yet (`KeyBinding.pressTime`);
- whether the game owns the input (no screen open, mouse captured, in a world);
- facts the game thread publishes each tick: the game tick, the swing progress,
  whether an item is in use, the held item, its use action and whether it is a block,
  and what the crosshair points at (`combat::pointed`, the client's
  `EntityRenderer.getMouseOver`): miss, block (position, face, state) or entity (id),
  with the hit point and its distance from the eye. These are one tick old, like the
  entity snapshot.

### Host functions

Appended after `world_highlight`:

- `input_press(me, action, count)`: `count` presses of the attack or use binding, as
  if the player clicked `count` times. They join the window's pending presses and
  travel in the next `InputUpdate`; the game thread clicks once per press in its next
  tick (`while (isPressed())`, step 1 of this work).
- `input_clear_press(me, action)`: drops the presses not sent yet, without a release.
- `input_add_look(me, yaw, pitch)`: turns the player's look by degrees. It is added to
  the next `InputUpdate`'s look, so the game thread turns the player with `set_angles`
  (pitch clamped to ±90°) and the server sees it in the next movement packet. The
  render camera follows the player as it does for the mouse.

They are callable from any thread and queue an intent; the window applies the queue
on the render thread before it builds the next `InputUpdate`, in call order. They
answer `UNAVAILABLE` while the game does not own the input (as a real click is
ignored then), and the window checks again when it applies them. A queue that is full
answers `BUSY`.

### Amendment of ADR-0086 F3.4

F3.4 keeps the raw key stream from mods so that none can read what the player types.
The `INPUT` event is a narrow exception: the held state and pending presses of the
**attack and use bindings only**, never which key they are bound to, never another
binding, and only to a mod that declared `input`.

## Consequences

- A 1.5 mod is refused by a 1.4 host; 1.0 to 1.4 mods load unchanged.
- The presses are counted, not flagged, everywhere between the window and the tick:
  two clicks in one tick are two clicks, for the player as for a mod.
- A press injected while a screen opens in the same frame is dropped by the second
  check, as the player's own press would be.
- Nothing caps the presses a mod makes beyond the per-call limit and the queue: the
  client gives the API, mods are responsible for how they use it (spec §10), and no
  official example presses or turns for the player.
