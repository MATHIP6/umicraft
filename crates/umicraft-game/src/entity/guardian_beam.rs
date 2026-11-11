// SPDX-License-Identifier: MIT OR Apache-2.0

//! The far end of a guardian's beam (MCP `EntityGuardian.getTargetedEntity`,
//! `RenderGuardian.doRender`).
//!
//! A guardian names what it beams at in its metadata (entry 17). The beam runs
//! from the guardian's eyes to the middle of the height of its target, which
//! must be a living entity. As for a leash ([`super::leash`]), the game thread
//! finds where the target is for each frame, since the renderer only has the
//! snapshots of what it draws, and the local player is not among them from the
//! first person.

use std::collections::HashMap;

use glam::{DVec3, Vec3};

use super::kind::EntityId;
use super::meta::Variant;
use super::snapshot::EntityFrameData;
use crate::combat::hitbox::size_of;

/// Where the middle of a target was at the previous tick and is now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamTarget {
    /// The middle of its height at the previous tick.
    pub prev_middle: DVec3,
    /// The middle of its height now.
    pub middle: DVec3,
}

impl BeamTarget {
    /// The target of an entity whose feet were at `prev_feet` and are at
    /// `feet`, `height` tall (`func_177110_a(target, height * 0.5, partial)`).
    #[must_use]
    pub fn of_feet(prev_feet: DVec3, feet: DVec3, height: f64) -> Self {
        let up = DVec3::Y * (height * 0.5);
        Self {
            prev_middle: prev_feet + up,
            middle: feet + up,
        }
    }

    /// The middle at the sub-tick fraction `alpha`, relative to `origin`
    /// (subtracted in f64).
    #[must_use]
    pub fn at(&self, alpha: f32, origin: DVec3) -> Vec3 {
        (self.prev_middle.lerp(self.middle, f64::from(alpha)) - origin).as_vec3()
    }

    /// The target that `frame` makes, if it can be beamed at: only a living
    /// entity can (`getTargetedEntity` casts to `EntityLivingBase`).
    fn of_frame(frame: &EntityFrameData) -> Option<Self> {
        if !frame.kind.is_living() {
            return None;
        }
        let (_, height) = size_of(frame.kind, &frame.meta)?;
        Some(Self::of_feet(frame.prev_position, frame.position, height))
    }
}

/// Fills `beam` of every guardian of `frames` with the target it beams at:
/// another entity of `frames`, or `local`, the local player (who is not among
/// them when seen from the first person). A target that is not tracked leaves
/// no beam, as `getTargetedEntity` returns `null`.
pub fn aim_guardians(frames: &mut [EntityFrameData], local: Option<(EntityId, BeamTarget)>) {
    let targets: HashMap<EntityId, BeamTarget> = frames
        .iter()
        .filter_map(|frame| Some((frame.id, BeamTarget::of_frame(frame)?)))
        .chain(local)
        .collect();
    for frame in frames {
        frame.beam = match frame.meta.variant {
            Variant::Guardian { target, .. } if target != 0 => {
                targets.get(&EntityId(target)).copied()
            }
            _ => None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{EntityKind, EntityRecord};

    fn frame(id: i32, kind: EntityKind, feet: DVec3) -> EntityFrameData {
        EntityRecord::new(EntityId(id), kind, feet, 0.0, 0.0).frame_data()
    }

    fn guardian(target: i32) -> EntityFrameData {
        let mut guardian = frame(1, EntityKind::Guardian, DVec3::ZERO);
        guardian.meta.variant = Variant::Guardian {
            moving: false,
            elder: false,
            target,
        };
        guardian
    }

    #[test]
    fn a_guardian_beams_at_the_middle_of_its_target() {
        let squid = frame(2, EntityKind::Squid, DVec3::new(5.0, 60.0, 0.0));
        let height = size_of(EntityKind::Squid, &squid.meta).expect("a body").1;
        let mut frames = [guardian(2), squid];
        aim_guardians(&mut frames, None);
        let beam = frames[0].beam.expect("a beam");
        assert_eq!(beam.middle, DVec3::new(5.0, 60.0 + height * 0.5, 0.0));
        assert!(frames[1].beam.is_none(), "a squid beams at nothing");
    }

    #[test]
    fn the_local_player_is_a_target_from_the_first_person() {
        let me = BeamTarget::of_feet(DVec3::ZERO, DVec3::new(0.0, 64.0, 0.0), 1.8);
        let mut frames = [guardian(9)];
        aim_guardians(&mut frames, Some((EntityId(9), me)));
        let beam = frames[0].beam.expect("a beam");
        assert!((beam.middle.y - 64.9).abs() < 1e-9);
        assert!((beam.at(0.5, DVec3::ZERO).y - 32.9).abs() < 1e-6);
    }

    #[test]
    fn no_target_an_unknown_one_or_an_object_leaves_no_beam() {
        let item = frame(3, EntityKind::Item, DVec3::ZERO);
        let mut frames = [guardian(0), guardian(42), guardian(3), item];
        aim_guardians(&mut frames, None);
        assert!(frames.iter().all(|frame| frame.beam.is_none()));
    }
}
