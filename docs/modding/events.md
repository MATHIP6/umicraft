# Mod events

Events a native mod can subscribe to (API 0.2, ADR-0086). Handlers run on the thread
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

Every string is borrowed for the duration of the call: copy it to keep it.

## Screen names

Stable snake_case names, never changed or reused: `chat`, `inventory`,
`creative_inventory`, `game_menu`, `options`, `pack_prompt`, `sign`, `book`, `debug`,
`title`, `loading`, `server_list`, `server_edit`, `confirm_delete`, `accounts`,
`connecting`, `disconnected`, `stats`, `achievements`, `mods`.

No screen (the game owns the input) fires neither event: going from `none` to `chat`
fires only `SCREEN_OPENED chat`, and back only `SCREEN_CLOSED chat`. A server container
shown by the inventory screen without a screen change fires nothing.
