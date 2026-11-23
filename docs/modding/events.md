# Mod events

Events a native mod can subscribe to (API 1.0, ADR-0086). Handlers run on the thread
shown below; an event nobody subscribes to costs nothing.

| Event | Id | Thread | Payload | Cancellable | Mutable | Fires |
|---|---|---|---|---|---|---|
| `PRE_TICK` | 1 | game | `tick: u64` | no | no | Before each 20 Hz game tick; `tick` counts from 0 |
| `POST_TICK` | 2 | game | `tick: u64` | no | no | After that tick, same number |
| `CHAT_RECEIVED` | 3 | game | `json: str`, `position: u8` (0 chat, 1 system, 2 action bar) | yes: the line is not shown | the JSON | When the server's chat packet arrives, before the chat visibility filter and the parsing. The replacement is what is parsed and shown |
| `CHAT_SEND` | 4 | game | `text: str` | yes: nothing is sent | the line | When the player sends a line (chat or command), before it is queued. The server length limit is checked after the mods, so a mod cannot send an over-long line |
| `CONNECTED` | 5 | render | `address: str` | no | no | When a session to the server is started (threads up), not when the server accepted the player |
| `DISCONNECTED` | 6 | render | `reason: str` | no | no | When that session ends; `reason` is the kick or error text, empty when the player left. Always paired with one `CONNECTED` |
| `SCREEN_OPENED` | 7 | render | `name: str` | no | no | After the screen changed to a new one |
| `SCREEN_CLOSED` | 8 | render | `name: str` | no | no | When the screen changes away from one, before `SCREEN_OPENED` of the next |
| `KEY_PRESSED` | 9 | render | `key: str` | no | no | When one of the mod's own key bindings is pressed with no screen open; only that mod receives it (see `ui.md`) |
| `PACKET_RECEIVED` | 10 | network | `id: u32`, `data: bytes` | yes: the client never sees it (not `Set Compression` 0x46 nor `Disconnect` 0x40) | no | For every play packet read from the server, before the client decodes it. Needs `network_packets` (see `packets.md`) |
| `PACKET_SEND` | 11 | network | `id: u32`, `data: bytes` | yes: nothing is written | no | For every play packet about to be written: the client's own, its automatic replies (keepalives) and injected ones. Needs `network_packets` |
| `FRAME` | 12 | render | `now_ms`, feet `position`, `fps`, `frame_ms`, `yaw`, `pitch`, `left_clicks`/`right_clicks` since the last frame, `ping_ms` (`-1` unknown), `sprinting`; since 1.1 the camera: `eye`, `render_origin`, `view_proj`, `viewport`, `gui_scale`, `fov_y`, `aspect`, `view_bob` | no | no | Once per drawn frame, once the camera of the frame is final and before the external widgets and the HUD are drawn; built from the snapshot the built-in mods read, without the key state |

The intercept handlers of the game-thread events may query the world (`world.md`).

## The camera of `FRAME` (API 1.1)

`view_proj` is the matrix the renderer draws the world with. Its input is a point
relative to `render_origin`, the integer corner the GPU positions are relative to:
`clip = view_proj * (point - render_origin, 1)`. Subtracting in `f64` first keeps the
precision far from the world origin (F2-1a). The matrix already holds the effective
field of view (option, zoom, sprint, flight; `fov_y`) and the view bobbing
(`view_bob`). `Frame::world_to_screen(point)` applies all of this. It returns GUI
pixels from the top-left corner, or `None` when the point is behind the camera or
outside the viewport.

Since 1.1 the client sends `FRAME`, and lets the external widgets draw, only after it
has placed the camera for the frame. Before, it did both at the start of the frame
and showed the previous frame's camera. The event still fires once per frame, before
the HUD, so a 1.0 mod sees no difference except fresher data. A widget fed by a
`FRAME` intercept therefore draws with the projection of the frame it is shown in.

Every string and byte view is borrowed for the duration of the call: copy it to keep it.

## Screen names

Stable snake_case names, never changed or reused: `chat`, `inventory`,
`creative_inventory`, `game_menu`, `options`, `pack_prompt`, `sign`, `book`, `debug`,
`title`, `loading`, `server_list`, `server_edit`, `confirm_delete`, `accounts`,
`connecting`, `disconnected`, `stats`, `achievements`, `mods`.

No screen (the game owns the input) fires neither event: going from `none` to `chat`
fires only `SCREEN_OPENED chat`, and back only `SCREEN_CLOSED chat`. A server container
shown by the inventory screen without a screen change fires nothing.
