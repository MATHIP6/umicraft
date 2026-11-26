# Attack and use input (API 1.5)

A mod that declares `input = true` in `[permissions]` reads the attack and use
bindings, the hand and what the crosshair points at, and can press those bindings and
turn the player's look through the client's own input path (ADR-0098). Without the
permission, subscribing to `INPUT` and every input function answer `DENIED`.

This is the only key state a mod ever sees: whether the **attack** and **use**
bindings are held and how many of their presses are pending. Which key or button they
are bound to, every other binding and the typed text stay hidden (ADR-0086, F3.4 as
amended).

## The `INPUT` event (id 16)

Render thread, once per frame in a world, right after `FRAME` and before the widgets
are drawn; not cancellable, not mutable. Rust: `PlayerInput`.

| Field | Rust | Meaning |
|---|---|---|
| `attack_held`, `use_held` | `attack_held()`, `use_held()` | The binding is held this frame, whatever it is bound to |
| `attack_presses`, `use_presses` | `attack_presses()`, `use_presses()` | Presses not sent to the game yet, the player's and the mods' (MCP `KeyBinding.pressTime`) |
| `now_ms` | `now_ms()` | The clock of `FRAME` |
| `tick` | `tick()` | Game ticks since the session started, at the tick the facts below come from |
| `game_focus` | `game_focus()` | No screen open, mouse captured, in a world |
| `using_item` | `using_item()` | Eating, drinking, blocking or drawing a bow |
| `swing_progress` | `swing_progress()` | The arm swing, 0 to 1 (MCP `swingProgress`; the tick right after a swing still reads 0) |
| `held_item`, `held_damage` | `held_item()` | Item id and damage, `-1` / `None` for an empty hand |
| `held_use_action` | `held_use_action()` | `UMI_USE_ACTION_NONE`, `EAT`, `DRINK`, `BLOCK` (a sword) or `BOW` |
| `held_is_block` | `held_is_block()` | The held item places a block |
| `pointed_*` | `pointed()` | What the crosshair points at, below |

`pointed_kind` is `UMI_POINTED_MISS`, `UMI_POINTED_BLOCK` or `UMI_POINTED_ENTITY`
(Rust: `Pointed::Miss`, `Block { .. }`, `Entity { .. }`):

- a block has its position (`pointed_block_pos`), the face the ray enters
  (`pointed_face`: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east, the numbering of
  `Player Digging`) and its state as `id << 4 | metadata` (`pointed_block`, as
  `world_block` reads it);
- an entity has its server id (`pointed_entity`);
- both have the hit point (`pointed_point`) and its distance from the eye
  (`pointed_distance`).

The hand and crosshair facts are those of the last game tick (the client picks its
target once per tick); the bindings and pending presses are those of this frame.

## Pressing and turning

| Function | Rust (`cx.input()`, a `Controller`) | Does |
|---|---|---|
| `input_press(me, action, count)` | `press(action, count)` | `count` presses (1 to 8) of `UMI_INPUT_ACTION_ATTACK` or `UMI_INPUT_ACTION_USE` |
| `input_clear_press(me, action)` | `clear_press(action)` | Drops the presses of `action` not sent yet, the player's included, without a release |
| `input_add_look(me, yaw, pitch)` | `add_look(yaw, pitch)` | Turns the look by degrees, each within ±180; positive turns right and looks down |

They are callable from any thread and queue a request; the window applies the queue,
in call order, just before it sends the next input update to the game thread. So a
press joins the player's own presses and a turn is added to the mouse's:

- the next game tick clicks **once per press**, as `Minecraft.runTick` does with
  `while (isPressed())`. Every rule of a real click applies: a miss outside creative
  starts the 10-tick delay that ignores the following attack presses, an item in use
  swallows the presses, the arm swings, a block breaks or is placed locally and the
  packets leave in the order of a vanilla tick;
- the look goes through `set_angles` like the mouse (the pitch stays within ±90°), so
  the server sees it in the next movement packet and the camera follows.

| Status | Rust | Why |
|---|---|---|
| `OK` | `Ok(())` | Queued |
| `DENIED` | `HostRefused(DENIED)` | No `input` permission, before `init` returned, or the mod is off |
| `INVALID_ARGUMENT` | `HostRefused(INVALID_ARGUMENT)` | An unknown action, a count outside 1..=8, a look that is not finite or beyond ±180 |
| `UNAVAILABLE` | `Unavailable` | The game does not own the input: a screen is open, the mouse is free or there is no world. A real click is ignored then too |
| `BUSY` | `Busy` | The shared queue of 256 requests is full: try again later |

Requests queued while the game loses the input (a screen opens in the same frame) are
dropped when the window applies them, as the player's own press would be.

## Server-compatible mode

Server-compatible mode does **not** hold an `input` mod off (the owner's choice,
ADR-0098): what it sends is what a player clicking and moving the mouse could send.
The mods menu shows the permission as an "attack & use input" badge; a player who must
follow a server's rules turns such a mod off there. Using a mod against a server's
rules may get the account banned (spec §10).

See `mods/examples/crosshair_info`, which only reads the event.
