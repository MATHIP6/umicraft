// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attack and use input event, emitted on the render thread once per frame in a
//! world, after `FRAME` (API 1.5, ADR-0098).

use umicraft_modapi_sys::{
    UmiEventHeader, UmiEventId, UmiInput, UMI_POINTED_BLOCK, UMI_POINTED_ENTITY, UMI_POINTED_MISS,
    UMI_USE_ACTION_BLOCK, UMI_USE_ACTION_BOW, UMI_USE_ACTION_DRINK, UMI_USE_ACTION_EAT,
    UMI_USE_ACTION_NONE,
};

use super::{header_for, host_event, EventThread, HostEvent};
use crate::world::HeldItem;

/// What using an item does (MCP `EnumAction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UseKind {
    /// Nothing lasting: the item is used at once, or not at all.
    #[default]
    None,
    /// Eaten.
    Eat,
    /// Drunk.
    Drink,
    /// Held up to block (a sword).
    Block,
    /// Drawn (a bow).
    Bow,
}

impl UseKind {
    fn to_wire(self) -> u8 {
        match self {
            Self::None => UMI_USE_ACTION_NONE,
            Self::Eat => UMI_USE_ACTION_EAT,
            Self::Drink => UMI_USE_ACTION_DRINK,
            Self::Block => UMI_USE_ACTION_BLOCK,
            Self::Bow => UMI_USE_ACTION_BOW,
        }
    }

    fn from_wire(value: u8) -> Self {
        match value {
            UMI_USE_ACTION_EAT => Self::Eat,
            UMI_USE_ACTION_DRINK => Self::Drink,
            UMI_USE_ACTION_BLOCK => Self::Block,
            UMI_USE_ACTION_BOW => Self::Bow,
            _ => Self::None,
        }
    }
}

/// What the crosshair points at (MCP `Minecraft.objectMouseOver`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PointedFacts {
    /// Nothing in reach.
    #[default]
    Miss,
    /// A block.
    Block {
        /// The block's position.
        position: [i32; 3],
        /// The face the ray enters: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east.
        face: u8,
        /// The block as `id << 4 | metadata`.
        state: u32,
        /// Where the ray hits, in blocks.
        point: [f64; 3],
        /// From the eye to `point`, in blocks.
        distance: f32,
    },
    /// An entity.
    Entity {
        /// The entity's server id.
        id: i32,
        /// Where the ray enters its box (the eye when inside it), in blocks.
        point: [f64; 3],
        /// From the eye to `point`, in blocks.
        distance: f32,
    },
}

/// What the game thread knows of the hand at its last tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HandFacts {
    /// Game ticks since the join.
    pub tick: u64,
    /// The arm swing, 0 to 1.
    pub swing_progress: f32,
    /// Whether an item is in use.
    pub using_item: bool,
    /// The held item, `None` for an empty hand.
    pub held: Option<HeldItem>,
    /// What using the held item does.
    pub held_use: UseKind,
    /// Whether the held item places a block.
    pub held_is_block: bool,
    /// What the crosshair points at.
    pub pointed: PointedFacts,
}

/// Everything the `INPUT` event carries, in Rust types.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputFacts {
    /// Monotonic time in milliseconds, the clock of `FRAME`.
    pub now_ms: u64,
    /// Whether the attack binding is held.
    pub attack_held: bool,
    /// Whether the use binding is held.
    pub use_held: bool,
    /// Attack presses not sent to the game thread yet.
    pub attack_presses: u8,
    /// Use presses not sent to the game thread yet.
    pub use_presses: u8,
    /// Whether the game owns the input (no screen, mouse captured, in a world).
    pub game_focus: bool,
    /// The hand and the crosshair at the last tick.
    pub hand: HandFacts,
}

/// Fired once per frame in a world, after [`Frame`](super::Frame), to the mods that
/// declared the `input` permission.
///
/// A plain `Copy` payload with no pointer, read-only and not cancellable. It is the
/// one exception to "mods never see the player's keys" (ADR-0086, F3.4 as amended
/// by ADR-0098): the attack and use bindings only.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Input {
    pub(crate) raw: UmiInput,
}

impl Input {
    /// Builds the event from the facts of the frame.
    pub fn new(facts: InputFacts) -> Self {
        let hand = facts.hand;
        let (kind, face, entity, point, distance, block, block_pos) = match hand.pointed {
            PointedFacts::Miss => (UMI_POINTED_MISS, 0, -1, [0.0; 3], 0.0, 0, [0; 3]),
            PointedFacts::Block {
                position,
                face,
                state,
                point,
                distance,
            } => (
                UMI_POINTED_BLOCK,
                face,
                -1,
                point,
                distance,
                state,
                position,
            ),
            PointedFacts::Entity {
                id,
                point,
                distance,
            } => (UMI_POINTED_ENTITY, 0, id, point, distance, 0, [0; 3]),
        };
        Self {
            raw: UmiInput {
                header: header_for::<UmiInput>(UmiEventId::INPUT),
                attack_held: u8::from(facts.attack_held),
                use_held: u8::from(facts.use_held),
                attack_presses: facts.attack_presses,
                use_presses: facts.use_presses,
                now_ms: facts.now_ms,
                tick: hand.tick,
                game_focus: u8::from(facts.game_focus),
                using_item: u8::from(hand.using_item),
                held_use_action: hand.held_use.to_wire(),
                held_is_block: u8::from(hand.held_is_block),
                swing_progress: hand.swing_progress,
                held_item: hand.held.map_or(-1, |held| held.id),
                held_damage: hand.held.map_or(0, |held| held.damage),
                pointed_kind: kind,
                pointed_face: face,
                reserved: [0; 2],
                pointed_entity: entity,
                pointed_point: point,
                pointed_distance: distance,
                pointed_block: block,
                pointed_block_pos: block_pos,
            },
        }
    }

    /// The facts of the frame, as the event was built from them.
    pub fn facts(&self) -> InputFacts {
        let raw = &self.raw;
        InputFacts {
            now_ms: raw.now_ms,
            attack_held: raw.attack_held != 0,
            use_held: raw.use_held != 0,
            attack_presses: raw.attack_presses,
            use_presses: raw.use_presses,
            game_focus: raw.game_focus != 0,
            hand: HandFacts {
                tick: raw.tick,
                swing_progress: raw.swing_progress,
                using_item: raw.using_item != 0,
                held: (raw.held_item >= 0).then_some(HeldItem {
                    id: raw.held_item,
                    damage: raw.held_damage,
                }),
                held_use: UseKind::from_wire(raw.held_use_action),
                held_is_block: raw.held_is_block != 0,
                pointed: self.pointed(),
            },
        }
    }

    /// What the crosshair pointed at during the last tick.
    pub fn pointed(&self) -> PointedFacts {
        let raw = &self.raw;
        match raw.pointed_kind {
            UMI_POINTED_BLOCK => PointedFacts::Block {
                position: raw.pointed_block_pos,
                face: raw.pointed_face,
                state: raw.pointed_block,
                point: raw.pointed_point,
                distance: raw.pointed_distance,
            },
            UMI_POINTED_ENTITY => PointedFacts::Entity {
                id: raw.pointed_entity,
                point: raw.pointed_point,
                distance: raw.pointed_distance,
            },
            _ => PointedFacts::Miss,
        }
    }
}

host_event!(Input, INPUT, Render, cancellable: false, mutable: false);
