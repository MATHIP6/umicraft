// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-frame snapshot event, emitted on the render thread.

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiFrame};

use super::{header_for, host_event, EventThread, HostEvent};

/// What the player sees in one frame, in Rust types.
///
/// The client fills it from its one per-frame snapshot (F3.9) and hands it to
/// [`Frame::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameFacts {
    /// Monotonic time in milliseconds.
    pub now_ms: u64,
    /// Feet position, in blocks.
    pub position: [f64; 3],
    /// Frames drawn during the last second.
    pub fps: u32,
    /// Smoothed duration of a frame, in milliseconds.
    pub frame_ms: f32,
    /// View angle in degrees (Minecraft yaw).
    pub yaw: f32,
    /// View angle in degrees, positive looks down.
    pub pitch: f32,
    /// Left clicks since the previous frame.
    pub left_clicks: u32,
    /// Right clicks since the previous frame.
    pub right_clicks: u32,
    /// Round trip to the server in milliseconds, `None` when not measured.
    pub ping_ms: Option<u32>,
    /// Whether the player sprints.
    pub sprinting: bool,
}

/// Fired once per rendered frame, right before the built-in mods update.
///
/// A plain `Copy` payload with no pointer, so unlike the text events a derived
/// `Clone` is correct and the type is `Send` without any `unsafe`. It is read-only
/// and not cancellable. It carries no key state: mods never see the player's keys
/// (ADR-0086, F3.4 amendment).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Frame {
    pub(crate) raw: UmiFrame,
}

impl Frame {
    /// Builds the event from the facts of the frame.
    ///
    /// A ping above `i32::MAX` is reported as unknown: the ABI field is signed and
    /// `-1` is its "not measured" value.
    pub fn new(facts: FrameFacts) -> Self {
        Self {
            raw: UmiFrame {
                header: header_for::<UmiFrame>(UmiEventId::FRAME),
                now_ms: facts.now_ms,
                position: facts.position,
                fps: facts.fps,
                frame_ms: facts.frame_ms,
                yaw: facts.yaw,
                pitch: facts.pitch,
                left_clicks: facts.left_clicks,
                right_clicks: facts.right_clicks,
                ping_ms: facts
                    .ping_ms
                    .and_then(|ping| i32::try_from(ping).ok())
                    .unwrap_or(-1),
                sprinting: u8::from(facts.sprinting),
            },
        }
    }

    /// The facts of the frame, as the event was built from them.
    pub fn facts(&self) -> FrameFacts {
        let raw = &self.raw;
        FrameFacts {
            now_ms: raw.now_ms,
            position: raw.position,
            fps: raw.fps,
            frame_ms: raw.frame_ms,
            yaw: raw.yaw,
            pitch: raw.pitch,
            left_clicks: raw.left_clicks,
            right_clicks: raw.right_clicks,
            ping_ms: u32::try_from(raw.ping_ms).ok(),
            sprinting: raw.sprinting != 0,
        }
    }

    /// Monotonic time in milliseconds.
    pub fn now_ms(&self) -> u64 {
        self.raw.now_ms
    }

    /// Feet position, in blocks.
    pub fn position(&self) -> [f64; 3] {
        self.raw.position
    }

    /// Frames drawn during the last second.
    pub fn fps(&self) -> u32 {
        self.raw.fps
    }

    /// Smoothed duration of a frame, in milliseconds.
    pub fn frame_ms(&self) -> f32 {
        self.raw.frame_ms
    }

    /// View yaw in degrees.
    pub fn yaw(&self) -> f32 {
        self.raw.yaw
    }

    /// View pitch in degrees, positive looks down.
    pub fn pitch(&self) -> f32 {
        self.raw.pitch
    }

    /// Left clicks since the previous frame.
    pub fn left_clicks(&self) -> u32 {
        self.raw.left_clicks
    }

    /// Right clicks since the previous frame.
    pub fn right_clicks(&self) -> u32 {
        self.raw.right_clicks
    }

    /// Round trip to the server in milliseconds, `None` when not measured.
    pub fn ping_ms(&self) -> Option<u32> {
        self.facts().ping_ms
    }

    /// Whether the player sprints.
    pub fn sprinting(&self) -> bool {
        self.raw.sprinting != 0
    }
}

host_event!(Frame, FRAME, Render, cancellable: false, mutable: false);
