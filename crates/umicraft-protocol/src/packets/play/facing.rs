// SPDX-License-Identifier: MIT OR Apache-2.0

//! The six block faces of the 1.8 world (MCP `net/minecraft/util/EnumFacing`).
//!
//! The wire form of a face is its index in the reference enum: 0 = down,
//! 1 = up, 2 = north, 3 = south, 4 = west, 5 = east. The reference server
//! wraps the index around (`EnumFacing.getFront` takes the absolute value
//! modulo the count), so only 0–5 are meaningful.

/// A block face, as 1.8 numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facing {
    /// Towards the block below (−Y).
    Down,
    /// Towards the block above (+Y).
    Up,
    /// Towards −Z.
    North,
    /// Towards +Z.
    South,
    /// Towards −X.
    West,
    /// Towards +X.
    East,
}

impl Facing {
    /// The wire index of this face.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Down => 0,
            Self::Up => 1,
            Self::North => 2,
            Self::South => 3,
            Self::West => 4,
            Self::East => 5,
        }
    }

    /// The face with the given wire index, `None` for an unknown index.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Down),
            1 => Some(Self::Up),
            2 => Some(Self::North),
            3 => Some(Self::South),
            4 => Some(Self::West),
            5 => Some(Self::East),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_face_indices_match_the_reference_enum() {
        assert_eq!(Facing::Down.code(), 0);
        assert_eq!(Facing::Up.code(), 1);
        assert_eq!(Facing::North.code(), 2);
        assert_eq!(Facing::South.code(), 3);
        assert_eq!(Facing::West.code(), 4);
        assert_eq!(Facing::East.code(), 5);
    }

    #[test]
    fn every_face_round_trips_through_its_index() {
        for code in 0..=5u8 {
            let facing = Facing::from_code(code).unwrap();
            assert_eq!(facing.code(), code);
        }
        assert_eq!(Facing::from_code(6), None);
    }
}
