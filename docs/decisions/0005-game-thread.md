# ADR-0005 — M3: a dedicated game thread, and the frozen M3 contract

- Status: Accepted
- Date: 2026-10-21
- Deciders: project owner

## Context

M2 streams world packets from a network thread into a `SessionWorld` owned by
the winit/render thread, meshes chunk snapshots on a worker pool, and uploads
the results. `docs/threading.md` records the deviation explicitly: *"M2 still
applies world packets and input on the render thread. The game thread with a
fixed 20 TPS tick split is M3 work."*

M3 adds the player (spec §12, GAM-1/2/3/8): 1.8 movement rules at 20 TPS,
client-side prediction with server reconciliation, block collision, block
breaking and placing, and interpolated rendering between ticks. The target
threading model (spec §5.3) requires a game thread that owns the fixed-step
simulation, and it forbids shared locks on hot paths.

Two problems force a decision now rather than later:

1. **Who owns the world.** Physics needs block lookups every tick, and the
   mesher needs chunk snapshots. If the world stays on the render thread and
   the game thread reads it, the collision path needs a lock on a hot path.
   If the game thread owns the world, the render thread must receive chunk
   data through a channel.
2. **Packet flow.** Chunk payloads are large (12 544 B per section in the
   captured flat world). Moving them between threads must not add a copy and
   must not create an unbounded queue.

The M3 work is also split across three independent crates, so the public
surface between them has to be frozen before the code is written, otherwise
each crate invents its own version of the same interface.

## Options

### World ownership

1. **World stays on the render thread; the game thread locks it for collision.**
   Rejected: a mutex on the collision path, exactly what spec §5.3 forbids.
2. **World on the game thread; the render thread receives snapshots.**
   *Chosen.* The world has a single owner, the collision path is lock-free,
   and the render thread receives `Arc<ChunkColumn>` handles rather than
   payloads.
3. **World in a shared concurrent map (e.g. `dashmap`).**
   Rejected: concurrent map access is a lock in disguise, and it gives up the
   copy-on-write snapshot model that M2 already relies on.

### Tick placement

1. **Tick on the render thread, driven by frame time.** Rejected: the
   simulation rate then depends on the frame rate, which breaks the
   "exactly 20 TPS" requirement of GAM-1 and the physics conformance tests.
2. **Tick on a dedicated game thread at a fixed 20 Hz.** *Chosen.* The
   render thread interpolates between the last two tick states.

## Decision

### Threads

| Thread | Owns | Talks to |
|---|---|---|
| Network thread (M2, unchanged) | Tokio runtime, `Connection`, `PlaySession` | game thread, via the bounded event channel |
| **Game thread (new)** | `umicraft_game::GameState`, the `umicraft_world::World`, the 20 Hz tick | network thread (events in), render thread (snapshots and render state out) |
| Render thread (M2, unchanged) | Window, wgpu, camera, `SessionWorld`-equivalent render state, mesh pool | game thread, via bounded channels |
| Mesh workers (M2, unchanged) | CPU meshing of immutable `ChunkSnapshot`s | render thread, via bounded channels |

- The **world moves to the game thread**. It is the only mutable owner.
  The render thread never mutates a chunk.
- Chunk data crosses from the network thread to the game thread through the
  existing bounded event channel. The game thread marks the affected columns
  dirty and hands the render thread an `Arc`-based `ChunkSnapshot` for the
  columns whose geometry changed.
- The render thread never waits: it polls its channels with `try_recv`, and
  the game thread never blocks on the render thread (dropped snapshots are
  re-derived from the dirty set).

### Tick model

- Fixed **20 Hz** (50 ms) simulation on the game thread, driven by an
  accumulator, independent of the frame rate.
- The game thread records the tick timestamp; the render thread computes
  `alpha` (the fraction of a tick elapsed since the last state) and asks the
  game state for an interpolated render state (GAM-8).
- The renderer consumes an immutable snapshot of the interpolated state; it
  never mutates game state.

### Prediction and reconciliation

- The client predicts its own position every tick from the local input.
- The server's authoritative corrections (`Player Position and Look` 0x08,
  `Entity Teleport` 0x18) are applied on the game thread: the authoritative
  position replaces the predicted one and the pending inputs are replayed.
- The prediction error is logged so the M3 acceptance can assert
  "no rubber-banding" numerically instead of by eye.

### Crate layout

`umicraft-game` is a new crate (already planned in spec §5.2: *"entities,
player physics, interactions, game state"*). It is **pure game logic**: no
I/O, no GPU, no async runtime, no networking. It depends on `umicraft-world`
(block lookups) and `glam` (math) only. This keeps it unit-testable without
a server and without a GPU, which is what the physics conformance tests
(spec §11) need.

`umicraft-core` (module loader, service registry, event bus) is **not**
wired in during M3. The service and event surfaces it provides become
relevant when the mod API lands (M9, spec §8); the game thread is designed
so that the physics step can later be published as a `PlayerPhysics` service
without changing its call sites.

## Consequences

- The render thread no longer applies world packets, so `SessionWorld` in
  `umicraft-client` splits: world state moves into the game thread, and the
  render thread keeps only camera, overlay, and mesh-upload state.
- Chunk payloads travel one extra hop (network → game → render). They are
  already boxed by the network thread, and the channel bounds are unchanged,
  so the memory ceiling does not move.
- The physics becomes testable in isolation: `umicraft-game` exposes a pure
  `tick` function, so a test can drive 20 ticks without a server and assert
  the trajectory.
- The client gains one more thread and one more bounded channel; both are
  documented in `docs/threading.md`.
- `umicraft-game` must not grow a dependency on `umicraft-client`,
  `umicraft-render`, `umicraft-net`, or `umicraft-protocol`; the game
  thread consumes plain data and the client translates at the boundary.

---

# Appendix A — the frozen M3 contract

This appendix is the interface the M3 workstreams code against. Signatures
may be refined, but **names, ownership, and semantics are frozen**: changing
them requires updating this ADR first.

## A.1 Ownership map (which crate owns which files)

| Workstream | Crate | May touch |
|---|---|---|
| A — protocol | `umicraft-protocol` | `crates/umicraft-protocol/**`, root `Cargo.toml` |
| B — game logic | `umicraft-game` (new) | `crates/umicraft-game/**`, root `Cargo.toml` |
| C — HUD | `umicraft-render` | `crates/umicraft-render/src/hud.rs` (+ `lib.rs` export) |
| D — assets transport | `umicraft-assets` (new) | `crates/umicraft-assets/**`, `.github/workflows/ci.yml`, root `Cargo.toml` |
| E — client integration | `umicraft-client` | `crates/umicraft-client/**`, root `Cargo.toml` |

No two workstreams edit the same file. The root `Cargo.toml` is edited in
each workstream's own branch and reconciled at integration time.

## A.2 `umicraft-game` public surface

```rust
// crates/umicraft-game/src/lib.rs
pub mod collision;   // AABB sweep against the voxel grid
pub mod input;       // InputIntent
pub mod physics;     // 1.8 movement rules
pub mod prediction;  // pending-input buffer + server reconciliation
pub mod raycast;     // voxel DDA for block targeting
pub mod shapes;      // per-block collision AABB (slabs, stairs, fences)

/// One tick of player intent, sampled from the input system.
pub struct InputIntent {
    pub forward: f32,  // -1.0 ..= 1.0
    pub strafe: f32,   // -1.0 ..= 1.0
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
    pub yaw: f32,      // radians
    pub pitch: f32,    // radians
}

/// Authoritative correction from the server.
pub struct ServerCorrection {
    pub x: f64, pub y: f64, pub z: f64,
    pub yaw: f32, pub pitch: f32,
    pub on_ground: bool,
}

/// The mutable game state owned by the game thread.
pub struct GameState { /* player, tick counter, pending inputs */ }

impl GameState {
    /// Advances the simulation by exactly one tick.
    pub fn tick(&mut self, intent: &InputIntent, world: &World);

    /// Applies an authoritative server position and replays pending inputs.
    pub fn apply_correction(&mut self, correction: &ServerCorrection, world: &World);

    /// Interpolated view of the player for the renderer.
    pub fn interpolated(&self, alpha: f32) -> RenderState;
}

/// What the renderer needs: plain data, no game types.
pub struct RenderState {
    pub position: Vec3,   // eye position
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}
```

Invariants:

- `GameState` is **not** `Send`-shared: it is owned by the game thread and
  never behind a lock. The client moves it into the game thread and receives
  `RenderState` snapshots.
- `tick` is deterministic: the same `InputIntent` sequence and the same
  `World` produce the same trajectory. No wall-clock reads inside `tick`.
- The physics constants are **derived from the MCP reference** and each one
  is cited in `docs/sources.md` (spec §14.6). Starting points:
  `mcp:net/minecraft/entity/EntityLivingBase.java` (`jump()` and the 0.91
  friction), `mcp:net/minecraft/entity/EntityPlayerSP.java` (the 0.2 input
  scaling and the sprint toggle), `mcp:net/minecraft/entity/Entity.java`
  (the 0.6 x 1.8 bounding box, `moveEntity`), and
  `mcp:net/minecraft/block/Block.java` (`setBlockBoundsBasedOnState`,
  `getCollisionBoundingBox`).

## A.3 `umicraft-render` HUD surface

```rust
// crates/umicraft-render/src/hud.rs
/// Everything the HUD draws, as plain data.
pub struct HudState {
    pub health: f32,       // 0..20
    pub food: f32,         // 0..20
    pub armor: f32,        // 0..20
    pub experience: f32,   // 0..1 progress in the level
    pub level: i32,
    pub hotbar: [Option<ItemStack>; 9],
    pub selected_slot: u8,
    pub crosshair: bool,
    pub chat_lines: Vec<String>,
    pub debug_lines: Vec<String>,
}

/// A hotbar slot: item id, damage, and stack count.
pub struct ItemStack {
    pub item_id: i32,
    pub damage: i16,
    pub count: u8,
}
```

The HUD is drawn with the existing `Overlay` primitives; it must not add a
second font or a second pipeline.

## A.4 M3 packet set (workstream A)

The wire id of a play-state packet is its **registration index** in
`mcp:net/minecraft/network/EnumConnectionState.java` (the `PLAY` constant).
The MCP class-name prefix matches that index, with one known exception:
`S19PacketEntityStatus` is registered at index **0x1A**, not 0x19
(`S19PacketEntityHeadLook` occupies 0x19). The project already relies on
this table for the M1/M2 packets; it is the authority, not the class name.

### Clientbound (server → client), to add

> **Corrected 2026-10-21.** The first draft of this table described the 1.19+
> layouts (the agent A workstream checked every field against the MCP source
> and found twelve packets where the draft was wrong). The column below now
> holds the **1.8** layouts, each verified against the MCP `readPacketData`
> named in the third column. The packet **ids** of the first draft were all
> correct.

| Id | Struct | MCP source | 1.8 layout |
|---|---|---|---|
| 0x03 | `TimeUpdate` | `S03PacketTimeUpdate` | i64 total world time, i64 world time |
| 0x04 | `EntityEquipment` | `S04PacketEntityEquipment` | varint entity id, i16 slot, **item stack** |
| 0x05 | `SpawnPosition` | `S05PacketSpawnPosition` | block position |
| 0x06 | `UpdateHealth` | `S06PacketUpdateHealth` | f32 health, **varint** food, f32 saturation |
| 0x07 | `Respawn` | `S07PacketRespawn` | i32 dimension, u8 difficulty, u8 game type, **string** world type (cap 16) — no hardcore flag |
| 0x09 | `HeldItemChange` | `S09PacketHeldItemChange` | i8 slot |
| 0x0B | `Animation` | `S0BPacketAnimation` | varint entity id, u8 animation |
| 0x0C | `SpawnPlayer` | `S0CPacketSpawnPlayer` | varint entity id, uuid (16 raw bytes), i32 x/y/z, i8 yaw, i8 pitch, i16 held item, watcher list — **no name string** |
| 0x13 | `DestroyEntities` | `S13PacketDestroyEntities` | varint count, varint entity ids |
| 0x14 | `Entity` | `S14PacketEntity` | varint entity id **only**; the movement is in the nested packets below |
| 0x15 | `EntityRelativeMove` | `S14PacketEntity.S15PacketEntityRelMove` | varint entity id, **i8** dx/dy/dz, bool on ground — no yaw/pitch |
| 0x16 | `EntityLook` | `S14PacketEntity.S16PacketEntityLook` | varint entity id, i8 yaw, i8 pitch, bool on ground |
| 0x17 | `EntityLookMove` | `S14PacketEntity.S17PacketEntityLookMove` | varint entity id, **i8** dx/dy/dz, i8 yaw, i8 pitch, bool on ground |
| 0x18 | `EntityTeleport` | `S18PacketEntityTeleport` | varint entity id, i32 x/y/z, i8 yaw, i8 pitch, bool on ground — **no relative flag** |
| 0x1A | `EntityStatus` | `S19PacketEntityStatus` | **plain i32** entity id, u8 status |
| 0x1F | `SetExperience` | `S1FPacketSetExperience` | f32 experience, varint level, varint total |
| 0x24 | `BlockAction` | `S24PacketBlockAction` | block position, u8 instrument, u8 pitch, **varint** block id |
| 0x25 | `BlockBreakAnim` | `S25PacketBlockBreakAnim` | varint breaker id, block position, u8 progress |
| 0x2B | `ChangeGameState` | `S2BPacketChangeGameState` | u8 state, f32 value |

`Entity Metadata` (0x1C) and `Entity Properties` (0x1D) are **out of scope
for M3**: their terminator-based format needs its own decision. The
**framing** of the watcher list is implemented (it is needed by `Spawn
Player`), but the typed metadata model is not.

### Serverbound (client → server), to add

| Id | Struct | MCP source | 1.8 layout |
|---|---|---|---|
| 0x02 | `UseEntity` | `C02PacketUseEntity` | varint target, u8 action ordinal, f32 x/y/z when the action is *interact at* |
| 0x04 | `PlayerPosition` | `C03PacketPlayer.C04PacketPlayerPosition` | f64 x, f64 y (feet), f64 z, u8 on ground |
| 0x05 | `PlayerLook` | `C03PacketPlayer.C05PacketPlayerLook` | f32 yaw, f32 pitch, u8 on ground |
| 0x07 | `PlayerDigging` | `C07PacketPlayerDigging` | u8 status, block position, u8 face |
| 0x08 | `PlayerBlockPlacement` | `C08PacketPlayerBlockPlacement` | block position, u8 face, **item stack**, u8 cursor x/y/z (in sixteenths) |
| 0x09 | `HeldItemChange` | `C09PacketHeldItemChange` | **i16** slot |
| 0x0A | `Animation` | `C0APacketAnimation` | **empty body** in 1.8 |
| 0x0B | `EntityAction` | `C0BPacketEntityAction` | varint entity id, u8 action, **varint** aux data |
| 0x0C | `Input` | `C0CPacketInput` | **f32 strafe, f32 forward, u8 flags** (bit 0 = jump, bit 1 = sneak) — not six booleans |

`Player Position and Look` (0x06) already exists.

## A.5 M3 acceptance

Spec §12: *"can walk, jump, sprint, place/break blocks on a Paper 1.8 server
without rubber-banding or kicks over a 30-minute session."* Made checkable:

1. A 30-minute session against the local Paper 1.8 server with movement
   enabled: **no kick** and no server-side anti-cheat complaint
   (*"moved too quickly"*, *"moved wrongly"*, *"flying"*).
2. The prediction error (distance between the predicted position and the
   server's correction) stays below **0.05 block** during normal movement.
3. Block breaking and placing are acknowledged by the server: the block
   state in `World` matches the server's `Block Change` (0x23).
4. The same scenario runs headless (no window), as spec §11 requires for the
   integration suite.
