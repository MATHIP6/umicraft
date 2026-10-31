// SPDX-License-Identifier: MIT OR Apache-2.0

//! The holder end of a leash (MCP `EntityLiving.getLeashedToEntity`,
//! `RenderLiving.renderLeash`).
//!
//! A leashed mob names its holder (a player or the knot on a fence). The game
//! thread finds where the holder is for each frame, so that the renderer, which
//! only has the snapshots of the entities it draws, can hang the rope on it.

use std::collections::HashMap;

use glam::Vec3;

use super::kind::{EntityId, EntityKind};
use super::snapshot::EntityFrameData;

/// Eye height of a player (MCP `EntityPlayer.getEyeHeight`).
const PLAYER_EYE_HEIGHT: f32 = 1.62;

/// Where a holder was at the previous tick and is now, and where it looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeashFrame {
    /// Feet at the previous tick.
    pub prev_position: Vec3,
    /// Feet now.
    pub position: Vec3,
    /// Yaw in degrees at the previous tick.
    pub prev_yaw: f32,
    /// Yaw in degrees now.
    pub yaw: f32,
    /// Pitch in degrees at the previous tick.
    pub prev_pitch: f32,
    /// Pitch in degrees now.
    pub pitch: f32,
    /// Height of the eyes over the feet.
    pub eye_height: f32,
    /// Whether the holder is a hanging entity (the knot of a fence), which the
    /// rope meets from above.
    pub hanging: bool,
}

/// The holder of a leash at one sub-tick fraction, ready to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeashAnchor {
    /// Feet of the holder.
    pub position: Vec3,
    /// Yaw in degrees (interpolated by half the fraction, as the reference).
    pub yaw: f32,
    /// Pitch in degrees (interpolated by half the fraction).
    pub pitch: f32,
    /// Height of the eyes over the feet.
    pub eye_height: f32,
    /// Whether the holder is a hanging entity.
    pub hanging: bool,
}

impl LeashFrame {
    /// The holder at the sub-tick fraction `alpha`.
    ///
    /// `RenderLiving.renderLeash` interpolates the angles with `partialTicks *
    /// 0.5` and the position with `partialTicks`.
    #[must_use]
    pub fn at(&self, alpha: f32) -> LeashAnchor {
        LeashAnchor {
            position: self.prev_position.lerp(self.position, alpha),
            yaw: self.prev_yaw + (self.yaw - self.prev_yaw) * alpha * 0.5,
            pitch: self.prev_pitch + (self.pitch - self.prev_pitch) * alpha * 0.5,
            eye_height: self.eye_height,
            hanging: self.hanging,
        }
    }

    fn of_entity(frame: &EntityFrameData) -> Self {
        Self {
            prev_position: frame.prev_position,
            position: frame.position,
            prev_yaw: frame.prev_yaw,
            yaw: frame.yaw,
            prev_pitch: frame.prev_pitch,
            pitch: frame.pitch,
            eye_height: if frame.kind == EntityKind::Player {
                PLAYER_EYE_HEIGHT
            } else {
                frame.kind.height() * 0.85
            },
            hanging: frame.kind == EntityKind::LeashKnot,
        }
    }
}

/// Fills `leash` of every leashed entity of `frames` with the frame of its
/// holder: another entity of `frames`, or `local`, the local player (who is not
/// among them when seen from the first person).
pub fn leash_entities(frames: &mut [EntityFrameData], local: Option<(EntityId, LeashFrame)>) {
    let holders: HashMap<EntityId, LeashFrame> = frames
        .iter()
        .map(|frame| (frame.id, LeashFrame::of_entity(frame)))
        .chain(local)
        .collect();
    for frame in frames {
        frame.leash = frame
            .leash_holder
            .and_then(|holder| holders.get(&holder).copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: i32, kind: EntityKind, position: Vec3) -> EntityFrameData {
        crate::entity::EntityRecord::new(
            EntityId(id),
            kind,
            glam::DVec3::new(
                f64::from(position.x),
                f64::from(position.y),
                f64::from(position.z),
            ),
            0.0,
            0.0,
        )
        .frame_data()
    }

    #[test]
    fn a_leashed_mob_finds_its_holder_among_the_frames() {
        let mut cow = frame(1, EntityKind::Cow, Vec3::ZERO);
        cow.leash_holder = Some(EntityId(2));
        let knot = frame(2, EntityKind::LeashKnot, Vec3::new(4.5, 64.5, 4.5));
        let mut frames = [cow, knot];
        leash_entities(&mut frames, None);
        let holder = frames[0].leash.expect("holder");
        assert!(holder.hanging);
        assert_eq!(holder.position, Vec3::new(4.5, 64.5, 4.5));
        assert!(frames[1].leash.is_none(), "the knot holds nothing");
    }

    #[test]
    fn the_local_player_holds_a_leash_without_being_a_frame() {
        let mut pig = frame(1, EntityKind::Pig, Vec3::ZERO);
        pig.leash_holder = Some(EntityId(9));
        let me = LeashFrame {
            prev_position: Vec3::new(1.0, 64.0, 1.0),
            position: Vec3::new(2.0, 64.0, 1.0),
            prev_yaw: 0.0,
            yaw: 90.0,
            prev_pitch: 0.0,
            pitch: 0.0,
            eye_height: 1.62,
            hanging: false,
        };
        let mut frames = [pig];
        leash_entities(&mut frames, Some((EntityId(9), me)));
        let anchor = frames[0].leash.expect("holder").at(1.0);
        assert_eq!(anchor.position, Vec3::new(2.0, 64.0, 1.0));
        // The angles move by half the fraction only.
        assert!((anchor.yaw - 45.0).abs() < 1e-4);
    }

    #[test]
    fn an_unknown_holder_leaves_the_mob_without_a_rope() {
        let mut pig = frame(1, EntityKind::Pig, Vec3::ZERO);
        pig.leash_holder = Some(EntityId(7));
        let mut frames = [pig];
        leash_entities(&mut frames, None);
        assert!(frames[0].leash.is_none());
    }
}
