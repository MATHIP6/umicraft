# API 1.5 — the attack and use input

Status: **in progress** (2026-11-26). Design: ADR-0098. Base: API 1.4 (ADR-0097).

| # | Phase | Acceptance | State |
|---|---|---|---|
| 1.5.0 | Presses counted, not flagged, from the window to the tick | regression test: two presses in one tick click twice | done |
| 1.5.1 | ADR-0098 and this plan | ADR accepted | done |
| 1.5.2 | `umicraft-modapi-sys`: types below, `UMI_API_VERSION = 1.5`, host functions appended | layout tests; a 1.5 mod is refused by a 1.4 host | |
| 1.5.3 | `umicraft-modhost`: `input` permission, `INPUT` event, host functions and the input queue | refusal, queue order and capacity tests | |
| 1.5.4 | Client: tick facts, the event each frame, the queue applied to the presses and the look | facts tests; injected press and look reach the game thread | |
| 1.5.5 | `umicraft-modapi` wrappers, docs, header, badge | fmt, clippy `-D warnings`, workspace tests | |

## Frozen interface (spec §14 rule 11)

### Version

`UMI_API_VERSION = UmiVersion { major: 1, minor: 5 }`.

### Permission

`Permissions::input: bool` (`[permissions] input = true`). Not part of
`ModContext::alters_game`: server-compatible mode does not hold it off.

### Event (`events.rs`)

`UmiEventId::INPUT = 16`, render thread, not cancellable, not mutable; subscribing
without `input` answers `DENIED`.

```rust
#[repr(C)]
pub struct UmiInput {
    pub header: UmiEventHeader,     //  0, 12 bytes
    pub attack_held: u8,            // 12  the attack binding is held
    pub use_held: u8,               // 13  the use binding is held
    pub attack_presses: u8,         // 14  presses not sent to the game yet
    pub use_presses: u8,            // 15
    pub now_ms: u64,                // 16  the clock of FRAME
    pub tick: u64,                  // 24  game ticks since the join
    pub game_focus: u8,             // 32  1: no screen, mouse captured, in a world
    pub using_item: u8,             // 33  an item is in use (eating, blocking, drawing)
    pub held_use_action: u8,        // 34  UMI_USE_ACTION_*
    pub held_is_block: u8,          // 35  the held item places a block
    pub swing_progress: f32,        // 36  0..1, 0 when not swinging
    pub held_item: i32,             // 40  item id, -1 when empty
    pub held_damage: i32,           // 44
    pub pointed_kind: u8,           // 48  UMI_POINTED_*
    pub pointed_face: u8,           // 49  0 down, 1 up, 2 north, 3 south, 4 west, 5 east
    pub reserved: [u8; 2],          // 50
    pub pointed_entity: i32,        // 52  server id, -1 unless ENTITY
    pub pointed_point: [f64; 3],    // 56  the hit point, in blocks
    pub pointed_distance: f32,      // 80  from the eye, in blocks; 0 for a miss
    pub pointed_block: u32,         // 84  id << 4 | metadata, 0 unless BLOCK
    pub pointed_block_pos: [i32; 3],// 88  the block, 0 unless BLOCK
}                                   // size 104 (align 8)

pub const UMI_POINTED_MISS: u8 = 0;
pub const UMI_POINTED_BLOCK: u8 = 1;
pub const UMI_POINTED_ENTITY: u8 = 2;

pub const UMI_USE_ACTION_NONE: u8 = 0;
pub const UMI_USE_ACTION_EAT: u8 = 1;
pub const UMI_USE_ACTION_DRINK: u8 = 2;
pub const UMI_USE_ACTION_BLOCK: u8 = 3;
pub const UMI_USE_ACTION_BOW: u8 = 4;
```

### Host functions (appended to `UmiHost`, after `world_highlight`)

```rust
input_press: extern "C" fn(me: UmiModHandle, action: u32, count: u32) -> UmiStatus,
input_clear_press: extern "C" fn(me: UmiModHandle, action: u32) -> UmiStatus,
input_add_look: extern "C" fn(me: UmiModHandle, yaw: f32, pitch: f32) -> UmiStatus,
```

```rust
pub const UMI_INPUT_ATTACK: u32 = 0;
pub const UMI_INPUT_USE: u32 = 1;
pub const UMI_MAX_INPUT_PRESSES: u32 = 8;   // per call
```

- All three: any thread; `DENIED` without `input` or while the mod is not enabled;
  `UNAVAILABLE` while the game does not own the input; `BUSY` when the queue (256
  intents shared by the host) is full, nothing queued.
- `input_press`: `INVALID_ARGUMENT` for an unknown action or a count outside
  `1..=UMI_MAX_INPUT_PRESSES`. The pending count saturates at 255.
- `input_clear_press`: `INVALID_ARGUMENT` for an unknown action. Clears the presses
  queued before it, the player's included, that were not sent yet.
- `input_add_look`: degrees, positive yaw turns right, positive pitch looks down;
  `INVALID_ARGUMENT` for a non-finite value or one beyond ±180.

### Limits

| What | Limit |
|---|---|
| Presses per `input_press` call | 8 |
| Queued intents (all mods) | 256 |
| Look delta per call | ±180° |
