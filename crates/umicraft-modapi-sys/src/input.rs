// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attack and use input (API 1.5, ADR-0098): the `INPUT` payload and the
//! constants of the `input_*` host functions.

use crate::UmiEventHeader;

/// Payload of [`UmiEventId::INPUT`](crate::UmiEventId::INPUT): the attack and use
/// bindings, the hand and what the crosshair points at. Added in API 1.5.
///
/// The hand and crosshair facts (`tick` to `pointed_block_pos`) are those of the
/// last game tick; the bindings are those of this frame.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiInput {
    /// Common header.
    pub header: UmiEventHeader,
    /// 1 while the attack binding is held, whatever it is bound to.
    pub attack_held: u8,
    /// 1 while the use binding is held.
    pub use_held: u8,
    /// Attack presses not sent to the game thread yet (MCP `KeyBinding.pressTime`).
    pub attack_presses: u8,
    /// Use presses not sent to the game thread yet.
    pub use_presses: u8,
    /// Monotonic time in milliseconds, the clock of `FRAME`.
    pub now_ms: u64,
    /// Game ticks since the session started.
    pub tick: u64,
    /// 1 while the game owns the input: no screen open, mouse captured, in a world.
    pub game_focus: u8,
    /// 1 while an item is in use (eating, drinking, blocking, drawing a bow).
    pub using_item: u8,
    /// What using the held item does, one of `UMI_USE_ACTION_*`.
    pub held_use_action: u8,
    /// 1 when the held item places a block.
    pub held_is_block: u8,
    /// The arm swing, 0 to 1, 0 when the arm is not swinging.
    pub swing_progress: f32,
    /// The held item id, -1 when the hand is empty.
    pub held_item: i32,
    /// The held item's damage or metadata value.
    pub held_damage: i32,
    /// What the crosshair points at, one of `UMI_POINTED_*`.
    pub pointed_kind: u8,
    /// The face of the pointed block: 0 down, 1 up, 2 north, 3 south, 4 west,
    /// 5 east; 0 unless `UMI_POINTED_BLOCK`.
    pub pointed_face: u8,
    /// Zero.
    pub reserved: [u8; 2],
    /// The pointed entity's server id, -1 unless `UMI_POINTED_ENTITY`.
    pub pointed_entity: i32,
    /// Where the ray hits, in blocks; the end of the reach for a miss.
    pub pointed_point: [f64; 3],
    /// From the eye to `pointed_point`, in blocks; 0 for a miss.
    pub pointed_distance: f32,
    /// The pointed block as `id << 4 | metadata`, 0 unless `UMI_POINTED_BLOCK`.
    pub pointed_block: u32,
    /// The pointed block's position, zeros unless `UMI_POINTED_BLOCK`.
    pub pointed_block_pos: [i32; 3],
}

/// `UmiInput::pointed_kind`: nothing in reach (MCP `MovingObjectType.MISS`).
pub const UMI_POINTED_MISS: u8 = 0;
/// `UmiInput::pointed_kind`: a block.
pub const UMI_POINTED_BLOCK: u8 = 1;
/// `UmiInput::pointed_kind`: an entity.
pub const UMI_POINTED_ENTITY: u8 = 2;

/// `UmiInput::held_use_action`: using the item does nothing lasting (MCP
/// `EnumAction.NONE`).
pub const UMI_USE_ACTION_NONE: u8 = 0;
/// `UmiInput::held_use_action`: the item is eaten.
pub const UMI_USE_ACTION_EAT: u8 = 1;
/// `UmiInput::held_use_action`: the item is drunk.
pub const UMI_USE_ACTION_DRINK: u8 = 2;
/// `UmiInput::held_use_action`: the item blocks (a sword).
pub const UMI_USE_ACTION_BLOCK: u8 = 3;
/// `UmiInput::held_use_action`: the item is drawn (a bow).
pub const UMI_USE_ACTION_BOW: u8 = 4;

/// `input_press` / `input_clear_press` action: the attack binding.
pub const UMI_INPUT_ACTION_ATTACK: u32 = 0;
/// `input_press` / `input_clear_press` action: the use binding.
pub const UMI_INPUT_ACTION_USE: u32 = 1;
/// Most presses one `input_press` call makes.
pub const UMI_MAX_INPUT_PRESSES: u32 = 8;
