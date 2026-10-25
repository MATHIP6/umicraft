# ADR-0015 — M8: Entity system, models, skins and rendering

- Status: Proposed (2026-10-25)
- Date: 2026-10-25
- Deciders: project owner, Antigravity
- Supersedes: none
- Related: ADR-0001 (reference source policy), ADR-0005 (game thread),
  ADR-0007 (M5 block models), ADR-0008 (M6 lighting and sky),
  ADR-0014 (M7 assets and packs)

## Context

Milestones M1–M7 established the networking, chunk storage, 1.8 block models,
atlas, lighting/sky, and first-run asset/resource pack systems. However, other
players, mobs, and dropped items are not yet tracked or rendered:
the game thread discards remote entity spawns, and the renderer draws only terrain
chunks, sky, and overlay UI.

Milestone M8 (spec §12.2, NET-8, RND-9, AST-9) delivers:
1. Complete 1.8 entity protocol packets (spawn mob, spawn object, movement,
   velocity, equipment, head look, destroy).
2. A typed entity metadata model (replacing raw DataWatcher byte lists).
3. Game-thread entity state storage, dead reckoning, and inter-tick interpolation.
4. Player skin loading (Mojang API async fetch, cache, offline Steve/Alex fallback)
   and mob texture extraction from base assets/resource packs.
5. 1.8 entity box models (humanoid with dual skin layers, common mobs, dropped items).
6. GPU entity rendering pass in `wgpu` with entity lighting and fog integration.
7. Acceptance demo `--entity-demo` and headless offscreen validation.

To respect spec §14.11 ("Freeze the interface before splitting the work"), this
ADR defines the public types, contracts, and file boundaries for M8.

## Decision

### 1. ECS vs Entity Store Choice (spec §13.2)

After evaluation of `bevy_ecs` (overly complex, high compilation overhead) and
`hecs` (minimalist ECS), we choose a **dedicated typed `EntityStore`** inside
`umicraft-game`:

- Minecraft 1.8 has a known, fixed set of client-side entity behaviors and
  components.
- Entity IDs from the server are 32-bit signed integers (`i32`); a contiguous or
  `BTreeMap`/`HashMap<i32, EntityRecord>` provides fast lookup without archetype
  overhead.
- Zero extra external crate dependencies, ensuring immediate compliance with
  `deny.toml` and minimal compilation impact.
- Thread boundary safety: the game thread owns `EntityStore` and publishes an
  immutable `Vec<EntityRenderSnapshot>` on each `Frame` sent to the render thread.
  The render thread never takes locks or mutates entities.

### 2. Entity State & Interpolation (`umicraft-game`)

Entities are stored with position, previous position, velocity, rotations,
interpolated limb swing (for walking animation), equipment, and metadata:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Player,
    Zombie,
    Skeleton,
    Creeper,
    Cow,
    Pig,
    Sheep,
    Chicken,
    Item,
    Arrow,
    Other(u8),
}

#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub id: EntityId,
    pub kind: EntityKind,
    pub position: glam::DVec3,
    pub previous_position: glam::DVec3,
    pub velocity: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    pub limb_swing: f32,
    pub limb_swing_amount: f32,
    pub equipment: [Option<umicraft_render::ItemStack>; 5], // 0: held, 1-4: armor
    pub metadata: TypedMetadata,
    pub skin_uuid: Option<[u8; 16]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityRenderSnapshot {
    pub id: EntityId,
    pub kind: EntityKind,
    pub position: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub limb_swing: f32,
    pub limb_swing_amount: f32,
    pub skin_id: SkinId,
    pub held_item: Option<u16>,
    pub metadata: EntityRenderFlags,
}
```

The game thread updates `limb_swing` and `limb_swing_amount` on each 20 Hz tick
using horizontal speed. Inter-tick interpolation:
`position = previous_position.lerp(target_position, partial_tick)`.

### 3. Protocol & Typed Metadata (`umicraft-protocol`, `umicraft-net`)

Add packet decoders for 1.8 play packets:
- `SpawnMob` (0x0F)
- `SpawnObject` (0x0E)
- `SpawnExperienceOrb` (0x11)
- `EntityVelocity` (0x12)
- `EntityHeadLook` (0x19)
- `EntityMetadata` (0x1C)
- `EntityProperties` (0x20)
- `AttachEntity` (0x1B)

DataWatcher parsing in `metadata.rs` is upgraded with a typed representation:
```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypedMetadata {
    pub flags: u8,               // Index 0: fire(0x01), sneak(0x02), sprint(0x08), eating(0x10), invisible(0x20)
    pub air: i16,                // Index 1
    pub custom_name: String,     // Index 2
    pub custom_name_visible: bool,// Index 3
    pub health: f32,             // Index 6
    pub skin_parts: u8,          // Index 10 (Player): cape, jacket, sleeves, pants, hat
    pub is_baby: bool,           // Index 12 (Ageable / Mobs)
}
```

### 4. Player Skins & Mob Textures (`umicraft-assets`)

1. **Player Skins**:
   - `SkinId`: unique hash or offline indicator.
   - Offline players: deterministically Steve (default) or Alex (slim arms)
     based on `uuid[15] & 1`.
   - Online players: asynchronous fetch via official Mojang session server
     `https://sessionserver.mojang.com/session/minecraft/profile/{uuid}`.
   - Textures decoded from base64, validated (64×64 RGBA or converted from 64×32),
     and stored in `%APPDATA%/umicraft/assets/skins/{sha1}.png`.
   - Built-in procedural 64×64 fallback skin ensures 100% offline functionality.
2. **Mob Textures**:
   - Extracted from user jar or resource pack using `ClientResources`:
     `textures/entity/zombie/zombie.png`, `textures/entity/cow/cow.png`, etc.

### 5. Entity Geometry & Animations (`umicraft-model`)

1. **Box Model Primitive**:
   - Cuboids defined by `(origin, size, uv_offset, texture_dims)`.
   - Generates 6 textured quad faces (12 triangles) with correct outward normals.
2. **Humanoid Model (1.8 Player, Zombie, Skeleton)**:
   - Head (8×8×8), Torso (8×12×4), Left Arm (4×12×4 or 3×12×4 for Alex),
     Right Arm, Left Leg (4×12×4), Right Leg.
   - Dual-layer outer mesh (Hat, Jacket, Sleeves, Pants) with 0.25–0.5 block dilation
     and alpha cutout.
   - Hierarchical pivots for head, arms, and legs.
3. **Animal & Mob Models**:
   - Cow, Pig, Sheep, Chicken, Creeper.
4. **Dropped Items**:
   - 3D miniature block or 2D rotating item quad.

### 6. Entity GPU Rendering (`umicraft-render`)

- Entity pipeline (`entity.wgsl`) takes vertex positions, normals, UVs, and
  entity lighting:
  ```rust
  #[repr(C)]
  pub struct EntityVertex {
      pub position: [f32; 3],
      pub normal: [f32; 3],
      pub uv: [f32; 2],
      pub light: u32,
  }
  ```
- Lighting: sampled from `World::light(block_pos)` at entity base.
- Fog: linear fog calculated from `SkyState::fog_color` and camera distance.
- Depth test enabled, depth write enabled (cutout discard for transparent pixels).
- Rendered between opaque chunk pass and transparent chunk pass.

## File Ownership (frozen)

| Workstream | Owns | Must not edit |
|---|---|---|
| **W0 Lead** | `docs/decisions/0015-*.md`, `docs/tasks/m8/**`, Cargo workspace setup | crate implementation files |
| **W1 Protocol & Net** | `crates/umicraft-protocol/src/packets/play/clientbound/{spawn_mob.rs, spawn_object.rs, entity_velocity.rs, entity_head_look.rs, entity_metadata.rs}`, `crates/umicraft-net/src/play/session.rs` | game, assets, render, client |
| **W2 Entity State** | `crates/umicraft-game/src/entity/**`, `crates/umicraft-game/src/lib.rs` | protocol, net, assets, render |
| **W3 Skins & Assets** | `crates/umicraft-assets/src/skin/**`, `crates/umicraft-assets/src/lib.rs` | game, protocol, render, client |
| **W4 Entity Models** | `crates/umicraft-model/src/entity/**`, `crates/umicraft-model/src/lib.rs` | game, render, client |
| **W5 Entity Renderer** | `crates/umicraft-render/src/entity/**`, `crates/umicraft-render/src/renderer.rs` | game, model, assets |
| **W6 Client & Demo** | `crates/umicraft-client/src/{entity_demo.rs, game.rs, world_view.rs, main.rs, args.rs}`, `docs/perf-log.md` | protocol, assets, model |

## Acceptance Criteria

1. `--entity-demo` generates a synthetic scene containing:
   - Player with official dual-layer skin,
   - Player with slim (Alex) arms,
   - Zombie with arms extended forward,
   - Cow (quadruped),
   - Dropped block/item.
2. `--screenshot target/m8-entities.png` writes a valid 1280×720 offscreen image.
3. Clean-room compliance: zero Mojang textures committed to git.
4. Performance: rendering frame times must not regress by > 5% compared to M7 baseline.
5. All workspace quality gates green (`cargo test`, `cargo clippy -D warnings`, `cargo fmt`).
