// SPDX-License-Identifier: MIT OR Apache-2.0

//! Armour stand entries (MCP `EntityArmorStand.entityInit`).

use umicraft_protocol::packets::play::Metadata;

use super::Variant;

/// Entry of the flags.
const FLAGS: u8 = 10;
/// First of the six rotation entries: head, body, left arm, right arm, left
/// leg, right leg.
const FIRST_ROTATION: u8 = 11;

/// A rotation in degrees about x, y and z.
pub type Pose = [f32; 3];

/// Armour stand state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmorStandMeta {
    /// Flags: `1` small, `4` arms, `8` no base plate, `16` marker.
    pub flags: u8,
    /// Head rotation.
    pub head: Pose,
    /// Body rotation.
    pub body: Pose,
    /// Left arm rotation.
    pub left_arm: Pose,
    /// Right arm rotation.
    pub right_arm: Pose,
    /// Left leg rotation.
    pub left_leg: Pose,
    /// Right leg rotation.
    pub right_leg: Pose,
}

impl Default for ArmorStandMeta {
    /// The poses of a new stand (MCP `DEFAULT_*_ROTATION`).
    fn default() -> Self {
        Self {
            flags: 0,
            head: [0.0, 0.0, 0.0],
            body: [0.0, 0.0, 0.0],
            left_arm: [-10.0, 0.0, -10.0],
            right_arm: [-15.0, 0.0, 10.0],
            left_leg: [-1.0, 0.0, -1.0],
            right_leg: [1.0, 0.0, 1.0],
        }
    }
}

impl ArmorStandMeta {
    /// A half-size stand.
    #[must_use]
    pub const fn small(&self) -> bool {
        self.flags & 1 != 0
    }

    /// A stand with arms.
    #[must_use]
    pub const fn has_arms(&self) -> bool {
        self.flags & 4 != 0
    }

    /// A stand without its stone base plate.
    #[must_use]
    pub const fn no_base_plate(&self) -> bool {
        self.flags & 8 != 0
    }

    /// A marker stand: no hitbox, still drawn.
    #[must_use]
    pub const fn marker(&self) -> bool {
        self.flags & 16 != 0
    }
}

pub(super) fn apply(variant: &mut Variant, metadata: &Metadata) {
    let mut stand = match variant {
        Variant::ArmorStand(stand) => *stand,
        _ => ArmorStandMeta::default(),
    };
    if let Some(flags) = metadata.get_byte(FLAGS) {
        stand.flags = flags;
    }
    let rotations = [
        &mut stand.head,
        &mut stand.body,
        &mut stand.left_arm,
        &mut stand.right_arm,
        &mut stand.left_leg,
        &mut stand.right_leg,
    ];
    for (offset, slot) in rotations.into_iter().enumerate() {
        let index = FIRST_ROTATION + u8::try_from(offset).unwrap_or(0);
        if let Some(rotation) = metadata.get_rotations(index) {
            *slot = rotation;
        }
    }
    *variant = Variant::ArmorStand(stand);
}

#[cfg(test)]
mod tests {
    use super::super::tests::{byte, meta};
    use super::super::EntityMeta;
    use super::*;
    use crate::entity::kind::EntityKind;
    use umicraft_protocol::packets::play::metadata::WatchedObject;

    fn rotation(index: u8, angles: [f32; 3]) -> WatchedObject {
        WatchedObject {
            type_id: 7,
            index,
            value: angles.iter().flat_map(|a| a.to_be_bytes()).collect(),
        }
    }

    #[test]
    fn a_new_stand_has_the_default_pose() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::ArmorStand, &meta(vec![byte(10, 0)]));
        let Variant::ArmorStand(stand) = state.variant else {
            panic!("a stand variant");
        };
        assert_eq!(stand.right_arm, [-15.0, 0.0, 10.0]);
        assert!(!stand.small() && !stand.has_arms());
    }

    #[test]
    fn flags_and_rotations_are_decoded() {
        let mut state = EntityMeta::default();
        state.apply(
            EntityKind::ArmorStand,
            &meta(vec![
                byte(10, 1 | 4 | 8 | 16),
                rotation(11, [10.0, 20.0, 30.0]),
                rotation(16, [1.0, 2.0, 3.0]),
            ]),
        );
        let Variant::ArmorStand(stand) = state.variant else {
            panic!("a stand variant");
        };
        assert!(stand.small() && stand.has_arms() && stand.no_base_plate() && stand.marker());
        assert_eq!(stand.head, [10.0, 20.0, 30.0]);
        assert_eq!(stand.right_leg, [1.0, 2.0, 3.0]);
        assert_eq!(stand.body, [0.0, 0.0, 0.0]);
    }
}
