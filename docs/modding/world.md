# World queries and the player physics

API 0.5, F3.6, ADR-0086.

## Where the queries answer

`world_block` and `world_entities` (Rust: `cx.world().block(..)`, `.entities(..)`)
read the game thread's world directly, so they answer only on that thread while it
runs one of the mod's callbacks:

- an intercept handler of a game-thread event (`PRE_TICK`, `POST_TICK`,
  `CHAT_RECEIVED`, `CHAT_SEND`);
- a client command (`/name`);
- a `player_physics` layer.

Anywhere else (an observer on the mod's worker, a HUD widget on the render thread, a
thread the mod started) they return `UNSUPPORTED`: v1 keeps no snapshot of the world
for other threads. Store what a widget needs from a game-thread callback, as
`mods/examples/physics_tweak` does. A block of a chunk that is not loaded is
`UNAVAILABLE`; a mod that is off gets `DENIED`. No permission is needed: the queries
only read.

`world_block` returns `id << 4 | metadata`. `world_entities` lists every entity but the
local player, with its server id, kind, position, view angles and a name (a player's
name or the name tag, else empty). The visitor returns non-`OK` (Rust: `false`) to stop.

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

The state exposes the position, velocity, fall distance, `on_ground` and `sprinting`
(writable) and `sneaking`, `flying`, `in_water`, yaw and pitch (read-only). The client
writes back only the fields a layer changed, so a wrapper that changes nothing leaves
the vanilla trajectory bit for bit as it was.

A layer runs on the game thread under the 1 ms budget, counted on its own time (the
time inside `next()` is not charged to it). A layer that panics, returns an error or
blows its budget fails its mod, and that tick its own changes are discarded: if it had
called `next()`, the state is what the inner layers produced; otherwise the rest of the
chain runs on the state it received.

The step is re-run for past ticks when the server corrects the position (the client
replays the inputs the server had not seen yet), so a layer must be a function of the
state and the input; a counter of calls is not a tick counter.

The server-compatible mode (`packets.md`) holds every `movement` mod off.
