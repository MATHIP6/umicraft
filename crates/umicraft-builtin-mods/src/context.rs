// SPDX-License-Identifier: MIT OR Apache-2.0
//! The read-only snapshot of the game a mod sees each frame.

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
}
