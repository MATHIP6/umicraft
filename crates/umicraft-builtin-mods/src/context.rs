// SPDX-License-Identifier: MIT OR Apache-2.0
//! The read-only snapshot of the game a mod sees each frame.

/// The most potion effects a context carries; the 1.8 game has 23 kinds.
pub const MAX_EFFECTS: usize = 24;

/// The keys and buttons the keystrokes mod shows, `true` while held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyState {
    /// Walk forward.
    pub forward: bool,
    /// Strafe left.
    pub left: bool,
    /// Walk backward.
    pub back: bool,
    /// Strafe right.
    pub right: bool,
    /// Jump.
    pub jump: bool,
    /// The attack button (left mouse button by default).
    pub attack: bool,
    /// The use-item button (right mouse button by default).
    pub use_item: bool,
}

/// What the combo counter needs to know about the fights of the session. Both are
/// running totals, so a mod only compares them with what it saw last frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CombatInfo {
    /// Attacks on an entity that the server confirmed (the target was hurt).
    pub hits: u32,
    /// Times the local player was hurt.
    pub hurts: u32,
}

/// An item shown by the armor status mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmorPiece {
    /// The registry name of the item without namespace, e.g. `diamond_helmet`.
    pub name: &'static str,
    /// Durability used so far; can be negative on a corrupt stack.
    pub damage: i16,
    /// Durability of a new item, `0` for an item that does not wear out.
    pub max_damage: u16,
}

/// An active potion effect of the local player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectInfo {
    /// The 1.8 effect id, `1` for Speed.
    pub id: u8,
    /// Level minus one, as the server sends it.
    pub amplifier: i8,
    /// Ticks left; `32767` is an endless effect.
    pub ticks: i32,
}

/// Facts about the running game, built once per frame by the client and handed to
/// every enabled mod. It is `Copy` so building and passing it never allocates.
///
/// Mods read this instead of reaching into the client, which keeps them testable
/// and keeps the client free of any knowledge of a particular mod.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModContext {
    /// Monotonic time in milliseconds, for rates such as clicks per second.
    pub now_ms: u64,
    /// Frames drawn during the last second.
    pub fps: u32,
    /// Smoothed duration of a frame, in milliseconds.
    pub frame_ms: f32,
    /// Position of the player's feet, in blocks.
    pub position: [f64; 3],
    /// Horizontal view angle, in degrees.
    pub yaw: f32,
    /// Vertical view angle, in degrees.
    pub pitch: f32,
    /// Whether the player is sprinting.
    pub sprinting: bool,
    /// Left clicks since the previous frame.
    pub left_clicks: u32,
    /// Right clicks since the previous frame.
    pub right_clicks: u32,
    /// Round-trip time to the server, if connected to one and measured.
    pub ping_ms: Option<u32>,
    /// The movement keys and mouse buttons held now.
    pub keys: KeyState,
    /// The fights of the session.
    pub combat: CombatInfo,
    /// The armor worn, helmet first then chestplate, leggings and boots.
    pub armor: [Option<ArmorPiece>; 4],
    /// The item in the hand.
    pub held: Option<ArmorPiece>,
    /// The active potion effects, packed at the front.
    pub effects: [Option<EffectInfo>; MAX_EFFECTS],
}
