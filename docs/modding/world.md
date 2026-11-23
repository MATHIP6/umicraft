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

## The entity snapshot (render thread, API 1.1)

`world_entities_snapshot(out, capacity, count)` (Rust: `World::snapshot(visitor)`)
answers on the **render thread**, inside a mod callback that thread runs: a `FRAME`
intercept or a HUD widget draw (and, once added, `PRE_RENDER` and camera layers). The
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
