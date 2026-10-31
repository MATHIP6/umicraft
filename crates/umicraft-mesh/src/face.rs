// SPDX-License-Identifier: MIT OR Apache-2.0

//! The six cube faces and their geometry table.

/// The six cube faces, in the order of the internal face table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// −X.
    West,
    /// +X.
    East,
    /// −Y.
    Down,
    /// +Y.
    Up,
    /// −Z.
    North,
    /// +Z.
    South,
}

impl Face {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::West => 0,
            Self::East => 1,
            Self::Down => 2,
            Self::Up => 3,
            Self::North => 4,
            Self::South => 5,
        }
    }
}

pub(crate) const ALL_FACES: [Face; 6] = [
    Face::West,
    Face::East,
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
];

pub(crate) struct FaceDef {
    pub(crate) normal: [f32; 3],
    pub(crate) offset: [i32; 3],
    /// Corners in counter-clockwise order seen from outside.
    pub(crate) corners: [[f32; 3]; 4],
    pub(crate) uvs: [[f32; 2]; 4],
}

/// Face order: west, east, down, up, north, south. Winding is CCW from
/// outside for wgpu's default front-face setting; side UVs keep `v = 0` at the
/// top so textures are upright.
pub(crate) const FACE_DEFS: [FaceDef; 6] = [
    FaceDef {
        normal: [-1.0, 0.0, 0.0],
        offset: [-1, 0, 0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
    FaceDef {
        normal: [1.0, 0.0, 0.0],
        offset: [1, 0, 0],
        corners: [
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
    FaceDef {
        normal: [0.0, -1.0, 0.0],
        offset: [0, -1, 0],
        corners: [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    },
    FaceDef {
        normal: [0.0, 1.0, 0.0],
        offset: [0, 1, 0],
        corners: [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        uvs: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
    },
    FaceDef {
        normal: [0.0, 0.0, -1.0],
        offset: [0, 0, -1],
        corners: [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
    FaceDef {
        normal: [0.0, 0.0, 1.0],
        offset: [0, 0, 1],
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
];
