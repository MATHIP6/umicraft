// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world border as the server describes it.
//!
//! Source: MCP `S44PacketWorldBorder.func_179788_a` (what each action sets)
//! and `WorldBorder` (defaults, `getDiameter` interpolation). The reference
//! interpolates against the wall clock; here the resize advances with the
//! 20 Hz tick (50 ms each) so the model stays deterministic and testable.

use umicraft_protocol::packets::play::clientbound::world_border::WorldBorder as BorderPacket;

/// Milliseconds per client tick.
const TICK_MS: i64 = 50;
/// MCP `WorldBorder` default diameter.
const DEFAULT_DIAMETER: f64 = 6.0e7;
/// MCP `WorldBorder` default `worldSize` (the portal boundary).
const DEFAULT_PORTAL_BOUNDARY: i32 = 29_999_984;
/// MCP `WorldBorder` default warning time, in seconds.
const DEFAULT_WARNING_TIME: i32 = 15;
/// MCP `WorldBorder` default warning distance, in blocks.
const DEFAULT_WARNING_BLOCKS: i32 = 5;

/// The border of the current dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    center_x: f64,
    center_z: f64,
    start_diameter: f64,
    end_diameter: f64,
    duration_ms: i64,
    elapsed_ms: i64,
    portal_boundary: i32,
    warning_time: i32,
    warning_blocks: i32,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            start_diameter: DEFAULT_DIAMETER,
            end_diameter: DEFAULT_DIAMETER,
            duration_ms: 0,
            elapsed_ms: 0,
            portal_boundary: DEFAULT_PORTAL_BOUNDARY,
            warning_time: DEFAULT_WARNING_TIME,
            warning_blocks: DEFAULT_WARNING_BLOCKS,
        }
    }
}

impl Border {
    /// Applies a border packet (0x44).
    pub fn apply(&mut self, packet: &BorderPacket) {
        match *packet {
            BorderPacket::SetSize { diameter } => self.set_size(diameter),
            BorderPacket::LerpSize {
                old_diameter,
                new_diameter,
                speed,
            } => self.lerp(old_diameter, new_diameter, speed),
            BorderPacket::SetCenter { x, z } => self.set_center(x, z),
            BorderPacket::Initialize {
                x,
                z,
                old_diameter,
                new_diameter,
                speed,
                portal_boundary,
                warning_time,
                warning_blocks,
            } => {
                self.set_center(x, z);
                self.lerp(old_diameter, new_diameter, speed);
                self.portal_boundary = portal_boundary;
                self.warning_blocks = warning_blocks;
                self.warning_time = warning_time;
            }
            BorderPacket::SetWarningTime { warning_time } => self.warning_time = warning_time,
            BorderPacket::SetWarningBlocks { warning_blocks } => {
                self.warning_blocks = warning_blocks;
            }
        }
    }

    fn set_center(&mut self, x: f64, z: f64) {
        self.center_x = x;
        self.center_z = z;
    }

    fn set_size(&mut self, diameter: f64) {
        self.start_diameter = diameter;
        self.end_diameter = diameter;
        self.duration_ms = 0;
        self.elapsed_ms = 0;
    }

    /// `setTransition(old, new, time)`; a non-positive time is an immediate
    /// change, as `Initialize` does for `timeUntilTarget <= 0`.
    fn lerp(&mut self, old: f64, new: f64, time_ms: i64) {
        if time_ms <= 0 {
            self.set_size(new);
            return;
        }
        self.start_diameter = old;
        self.end_diameter = new;
        self.duration_ms = time_ms;
        self.elapsed_ms = 0;
    }

    /// Advances a running resize by one tick.
    pub fn tick(&mut self) {
        if self.is_resizing() {
            self.elapsed_ms = self.elapsed_ms.saturating_add(TICK_MS);
            if self.elapsed_ms >= self.duration_ms {
                self.set_size(self.end_diameter);
            }
        }
    }

    /// The current diameter (`WorldBorder.getDiameter`).
    pub fn diameter(&self) -> f64 {
        if !self.is_resizing() {
            return self.end_diameter;
        }
        let progress = self.elapsed_ms as f64 / self.duration_ms as f64;
        self.start_diameter + (self.end_diameter - self.start_diameter) * progress
    }

    /// The diameter the border is heading to.
    pub fn target_diameter(&self) -> f64 {
        self.end_diameter
    }

    /// Whether a resize is in progress.
    pub fn is_resizing(&self) -> bool {
        self.duration_ms > 0
    }

    /// The centre `(x, z)`.
    pub fn center(&self) -> (f64, f64) {
        (self.center_x, self.center_z)
    }

    /// The portal boundary (MCP `WorldBorder.getSize`).
    pub fn portal_boundary(&self) -> i32 {
        self.portal_boundary
    }

    /// The warning time in seconds.
    pub fn warning_time(&self) -> i32 {
        self.warning_time
    }

    /// The warning distance in blocks.
    pub fn warning_blocks(&self) -> i32 {
        self.warning_blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_reference() {
        let border = Border::default();
        assert_eq!(border.diameter(), 6.0e7);
        assert_eq!(border.center(), (0.0, 0.0));
        assert_eq!(border.portal_boundary(), 29_999_984);
        assert_eq!((border.warning_time(), border.warning_blocks()), (15, 5));
        assert!(!border.is_resizing());
    }

    #[test]
    fn set_size_and_center_apply_immediately() {
        let mut border = Border::default();
        border.apply(&BorderPacket::SetSize { diameter: 100.0 });
        border.apply(&BorderPacket::SetCenter { x: 3.0, z: -4.0 });
        assert_eq!(border.diameter(), 100.0);
        assert_eq!(border.center(), (3.0, -4.0));
    }

    #[test]
    fn warnings_are_stored() {
        let mut border = Border::default();
        border.apply(&BorderPacket::SetWarningTime { warning_time: 30 });
        border.apply(&BorderPacket::SetWarningBlocks { warning_blocks: 9 });
        assert_eq!((border.warning_time(), border.warning_blocks()), (30, 9));
    }

    #[test]
    fn lerp_interpolates_by_tick_and_ends_on_target() {
        let mut border = Border::default();
        border.apply(&BorderPacket::LerpSize {
            old_diameter: 100.0,
            new_diameter: 200.0,
            speed: 200, // 4 ticks
        });
        assert!(border.is_resizing());
        assert_eq!(border.diameter(), 100.0);
        border.tick();
        border.tick();
        assert_eq!(border.diameter(), 150.0);
        border.tick();
        border.tick();
        assert!(!border.is_resizing());
        assert_eq!(border.diameter(), 200.0);
        assert_eq!(border.target_diameter(), 200.0);
    }

    #[test]
    fn a_non_positive_lerp_time_is_immediate() {
        let mut border = Border::default();
        for speed in [0, -5] {
            border.apply(&BorderPacket::LerpSize {
                old_diameter: 1.0,
                new_diameter: 7.0,
                speed,
            });
            assert!(!border.is_resizing());
            assert_eq!(border.diameter(), 7.0);
        }
    }

    #[test]
    fn initialize_sets_everything() {
        let mut border = Border::default();
        border.apply(&BorderPacket::Initialize {
            x: 1.0,
            z: 2.0,
            old_diameter: 10.0,
            new_diameter: 20.0,
            speed: 1000,
            portal_boundary: 500,
            warning_time: 7,
            warning_blocks: 3,
        });
        assert_eq!(border.center(), (1.0, 2.0));
        assert_eq!(border.diameter(), 10.0);
        assert!(border.is_resizing());
        assert_eq!(border.portal_boundary(), 500);
        assert_eq!((border.warning_time(), border.warning_blocks()), (7, 3));
    }

    #[test]
    fn a_new_set_size_cancels_a_running_lerp() {
        let mut border = Border::default();
        border.apply(&BorderPacket::LerpSize {
            old_diameter: 1.0,
            new_diameter: 9.0,
            speed: i64::MAX,
        });
        border.tick();
        border.apply(&BorderPacket::SetSize { diameter: 5.0 });
        border.tick();
        assert_eq!(border.diameter(), 5.0);
    }
}
