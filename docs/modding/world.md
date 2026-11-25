# World queries and the player physics

API 0.5, F3.6, ADR-0086.

## Where the queries answer

`world_block` and `world_entities` (Rust: `cx.world().block(..)`, `.entities(..)`)
read the game thread's world directly, so they answer only on that thread while it
runs one of the mod's callbacks. The render thread has its own view of the entities,
the [entity snapshot](#the-entity-snapshot-render-thread-api-11):

- an intercept handler of a game-thread event (`PRE_TICK`, `POST_TICK`,
  `CHAT_RECEIVED`, `CHAT_SEND`);
- a client command (`/name`);
- a `player_physics` layer.

Anywhere else (an observer on the mod's worker, a HUD widget on the render thread, a
thread the mod started) they return `UNSUPPORTED`: the blocks are not snapshotted for
other threads. Store what a widget needs from a game-thread callback, as
`mods/examples/physics_tweak` does, or, for entities, read the snapshot below. A block of a chunk that is not loaded is
`UNAVAILABLE`; a mod that is off gets `DENIED`. No permission is needed: the queries
only read.

`world_block` returns `id << 4 | metadata`. `world_entities` lists every entity but the
local player, with its server id, kind, position, view angles, a name (a player's
name or the name tag, else empty) and, since API 1.1, the facts below. The visitor
returns non-`OK` (Rust: `false`) to stop.

## What an entity tells (API 1.1)

Every `UmiEntity` (Rust: `Entity`) carries these, in a game-thread query and in the
snapshot alike. The values are what the client knows, not what the server knows:

| Field (Rust getter) | Meaning |
|---|---|
| `health` (`health()`) | Health points (half hearts); `0` unless `UMI_ENTITY_LIVING`. Before the first metadata packet it is the vanilla default, 1. |
| `max_health` (`max_health()`) | The `generic.maxHealth` attribute the server sent, else the vanilla default of the kind (a wolf 8 or 20 tamed, a slime its size squared, a horse 53...); `0` unless living. |
| `armor` (`armor()`) | Armour points of the equipment the server said the entity wears (diamond set 20). |
| `team` (`team()`) | Scoreboard team name; empty when none. Only players are looked up (by name): the client does not track the team of other entities. |
| `width`, `height` | The bounding box in blocks (the size of the crosshair ray; a child is half). |
| `velocity` (`velocity()`) | Position change over the last tick, blocks per tick. |
| `flags` (`flags()`, `on_fire()`, `sneaking()`, `sprinting()`, `using_item()`, `invisible()`, `is_living()`) | `UMI_ENTITY_*` bits from metadata entry 0, plus `LIVING` for everything that is not an object. |
| `is_player` (`is_player()`) | The entity is a player. |
| `render_position` (`render_position()`) | Where the frame being drawn shows the entity. In a game-thread query it is `position`. |

The sources of each value are in `docs/sources.md`.

## Combat, teams and the tab list (API 1.4)

API 1.4 (ADR-0097) appends these to `UmiEntity` (offset 168, the struct is now 280
bytes) and adds four flag bits. A mod built for 1.1 to 1.3 still gets rows of its own
168-byte struct from `world_entities_snapshot`: the host writes each row at the size
the mod was built with.

| Field (Rust getter) | Meaning |
|---|---|
| `hurt_time` (`hurt_time()`, `is_hurt()`) | Ticks left of the red hurt flash, 10 right after a hit, then down to 0 (vanilla `hurtTime`). |
| `death_time` (`death_time()`) | Ticks into the death fall, 0 while alive (vanilla `deathTime`). |
| `held_item`, `held_damage` (`held_item()`) | The item id in the main hand and its damage value; `-1` (`None`) when empty or unknown. |
| `display_name` (`display_name()`) | A player's team prefix, name and suffix with their `§` colour codes (`ScorePlayerTeam.formatPlayerName`); for another entity its name tag; empty when none. |
| `team_color` (`team_color()`) | `0xRRGGBB`: the team's colour index when the server set one, else the last colour code of the team prefix (what 1.8 colours names with); `UMI_NO_COLOR` (`None`) when there is none. |
| `ping` (`ping()`) | The latency the tab list shows, in ms; `-1` (`None`) for an entity the tab list does not list. |
| `uuid_version` (`uuid_version()`) | The version nibble of the UUID: 4 for an online-mode player, 3 offline, 2 for many server-made NPCs; 0 unknown. |
| `effect_count`, `effects` (`effects()`, `effect(id)`) | Up to 8 potion effects in id order: id, amplifier (0 = level I), a hide-particles flag and the ticks left. A 1.8 server sends `Entity Effect` for every tracked living entity; the client counts the durations down. |
| `UMI_ENTITY_SPECTATOR` (`spectator()`) | A player in spectator mode (from the tab list). |
| `UMI_ENTITY_IN_TAB_LIST` (`in_tab_list()`) | The tab list shows the entity. |
| `UMI_ENTITY_VISIBLE` (`visible()`) | A block-free line runs from the local player's eye to the entity's eye height, middle or feet, tested on the game thread each tick against the blocks the crosshair stops at, up to 128 blocks. One tick old; from the player's eye, not a mod camera's. For the exact visible pixels, use the [entity mask](post.md#the-entity-mask-api-14). |
| `UMI_ENTITY_TEAMMATE` (`teammate()`) | On the local player's scoreboard team. |

The client does not decide what a bot is: a mod combines `in_tab_list`, `ping` and
`uuid_version` with its own rules.

### The local player (API 1.4)

`world_local_player(out)` (Rust: `World::local_player(|me| ...)`) fills one
`UmiEntity` for the local player, under the same rules as the snapshot below (render
thread, inside a callback; strings valid until it returns). Its id, name, team,
display name and team colour, health, effects, held item, ping and position are the
player's own; `UMI_ENTITY_VISIBLE` is never set, and `UMI_ENTITY_TEAMMATE` is set when
the player is on a team. `UNAVAILABLE` before the client knows the player.

```rust
let my_team = world.local_player(|me| me.team().to_owned())?;
```

## The entity snapshot (render thread, API 1.1)

`world_entities_snapshot(out, capacity, count)` (Rust: `World::snapshot(visitor)`)
answers on the **render thread**, inside a mod callback that thread runs: a `FRAME`
intercept, a HUD widget draw, a `PRE_RENDER` handler, a camera layer or, since 1.4, a
[world layer](draw.md). The
client rebuilds the snapshot once per frame, right before the `FRAME` event, and keeps
it for the widget draws of that frame.

- **What it holds.** The 256 entities nearest to the camera eye at most, nearest first,
  the local player excluded. Nearest is measured to `render_position`.
- **Staleness.** Everything except `render_position` is a fact of the last game tick,
  so it can be one tick (50 ms) old. `render_position` is the position the frame draws
  the entity at (interpolated between the last two ticks, the same as the entity
  renderer).
- **Equality.** The snapshot is built with the same function as the game-thread query,
  so for the same tick the two agree on every fact; only `render_position` differs.
- **Calling it.** It copies `min(capacity, n)` rows to `out` and writes the total `n`
  (at most 256) to `*count`; a null `out` with capacity 0 asks how many. A null `count`
  is `INVALID_ARGUMENT`. The strings point into the snapshot and are valid until the
  callback returns.
- **Statuses.** `UNSUPPORTED` outside a render-thread callback (the game thread, an
  observer, a thread of the mod); `UNAVAILABLE` when there is no world (no session);
  `DENIED` for a mod that is off or failed.
- **Cost.** The game thread copies the facts of every entity into each frame it
  publishes only when external mods are loaded; the render thread selects the nearest
  256 once per frame without allocating once its buffers are warm.

In Rust, `World::snapshot` is a visitor like `World::entities` (`false` stops it):

```rust
let eye = ...; // from a Frame intercept
cx.hud_widget("list", "Entity list", placement, move |hud| {
    let _ = world.snapshot(|entity| {
        if entity.is_living() {
            // entity.name(), entity.health() / entity.max_health(), ...
        }
        true
    });
})?;
```

Errors are `ModError::Unavailable` (no world) and `ModError::HostRefused(status)`. A
panic in the visitor propagates to the callback, which fails the mod as any panic does.
The official example is `mods/examples/entity_list`, a debugging view that lists the
ten nearest entities with their health (owner decision, ADR-0094).

## Entity kinds

Stable snake_case names, never changed or reused:
`player`, `zombie`, `skeleton`, `creeper`, `cow`, `pig`, `sheep`, `chicken`, `spider`, `cave_spider`, `enderman`, `slime`, `magma_cube`, `zombie_pigman`, `giant`, `villager`, `zombie_villager`, `witch`, `iron_golem`, `snowman`, `wolf`, `mooshroom`, `squid`, `bat`, `blaze`, `dragon`, `endermite`, `ghast`, `guardian`, `horse`, `ocelot`, `rabbit`, `silverfish`, `wither`, `item`, `arrow`, `item_frame`, `armor_stand`, `fish_hook`, `boat`, `minecart`, `primed_tnt`, `ender_crystal`, `snowball`, `egg`, `large_fireball`, `small_fireball`, `ender_pearl`, `wither_skull`, `falling_block`, `eye_of_ender`, `potion`, `exp_bottle`, `firework_rocket`, `leash_knot`, `xp_orb`, `painting`, `lightning`, `other`.

`other` is an object type the client does not model.

## The `player_physics` service

The step that moves the local player once per tick (the vanilla 1.8 rules, ADR-0076).
A mod needs `movement = true` in `[permissions]` and the service in `[load]`:

```toml
[load]
wraps = ["player_physics"]      # or: replaces = ["player_physics"]

[permissions]
movement = true
```

Any other service id in `wraps` or `replaces` fails the mod at load. Two mods that
replace the service without declaring compatibility are a load conflict (the second
in load order is refused).

- **Wrap** (`cx.wrap_physics`): the layer gets the state and the input and calls
  `next()` at most once to run what is inside it. Wrappers nest in load order, the
  first loaded outermost; a replacing layer always sits inside every wrapper.
- **Replace** (`cx.replace_physics`): the layer is the step; `next()` is
  `UNSUPPORTED`.

The state exposes the position, velocity, fall distance, `on_ground`, `sprinting`,
`flying` and the game mode (writable) and `sneaking`, `in_water`, `allow_flying`,
`fly_speed`, yaw and pitch (read-only). The client writes back only the fields a layer
changed, so a wrapper that changes nothing leaves the vanilla trajectory bit for bit as
it was.

### Flight and game mode (API 1.1)

- `set_flying(bool)` is applied even when `allow_flying()` is false: the client
  predicts it and the server, which is the authority, may stop a flight it refuses. The
  change is reported to the server once, as the vanilla `Player Abilities` packet (MCP
  `EntityPlayerSP.sendPlayerAbilities`), after the step and only when the state really
  differs from the one before it. The replay of a server correction reports through the
  same comparison, so a replayed tick never sends a second packet.
- `set_game_mode(GameMode)` is a local prediction. The client applies it as it applies
  the server's own change (HUD, no-clip, flight rules) and sends nothing; the next
  game-mode packet of the server wins. A value outside `UMI_GAME_MODE_*` is ignored.
  The client stores its real mode before every run, so a layer that sets a mode every
  tick keeps predicting it until the server says otherwise.
- `allow_flying()` and `fly_speed()` are the abilities the server granted.
- On a host that gives the 1.0 struct (72 bytes) the getters answer `false`, `0.05`
  and survival, and the setters do nothing. A 1.1 mod does not load on a 1.0 host, so
  this only guards the view.

A layer must be idempotent (`set_flying(true)`, not a toggle): the step is re-run on
replay, starting from the state the previous run left.

A layer runs on the game thread under the 1 ms budget, counted on its own time (the
time inside `next()` is not charged to it). A layer that panics, returns an error or
blows its budget fails its mod, and that tick its own changes are discarded: if it had
called `next()`, the state is what the inner layers produced; otherwise the rest of the
chain runs on the state it received.

The step is re-run for past ticks when the server corrects the position (the client
replays the inputs the server had not seen yet), so a layer must be a function of the
state and the input; a counter of calls is not a tick counter.

The server-compatible mode (`packets.md`) holds every `movement` mod off, so flight and
game-mode writes never reach a server that asked for vanilla behaviour.
