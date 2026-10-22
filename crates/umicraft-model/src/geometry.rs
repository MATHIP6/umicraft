// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baked block-local geometry shared by the model pipeline and the mesher.

/// The six axis-aligned block faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Neighbour offset from the current block.
    #[must_use]
    pub const fn offset(self) -> [i32; 3] {
        match self {
            Self::West => [-1, 0, 0],
            Self::East => [1, 0, 0],
            Self::Down => [0, -1, 0],
            Self::Up => [0, 1, 0],
            Self::North => [0, 0, -1],
            Self::South => [0, 0, 1],
        }
    }

    /// Outward unit normal.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        match self {
            Self::West => [-1.0, 0.0, 0.0],
            Self::East => [1.0, 0.0, 0.0],
            Self::Down => [0.0, -1.0, 0.0],
            Self::Up => [0.0, 1.0, 0.0],
            Self::North => [0.0, 0.0, -1.0],
            Self::South => [0.0, 0.0, 1.0],
        }
    }

    /// All faces in the canonical order used by the model baker.
    pub const ALL: [Self; 6] = [
        Self::West,
        Self::East,
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
    ];

    /// Parses a 1.8 face name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "west" => Some(Self::West),
            "east" => Some(Self::East),
            "down" => Some(Self::Down),
            "up" => Some(Self::Up),
            "north" => Some(Self::North),
            "south" => Some(Self::South),
            _ => None,
        }
    }
}

/// One baked, textured quad in block-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuad {
    /// Four corners, counter-clockwise seen from outside, in block-local space.
    pub positions: [[f32; 3]; 4],
    /// Outward normal after element and variant rotation.
    pub normal: [f32; 3],
    /// Four normalized texture coordinates with `v = 0` at the texture top.
    pub uvs: [[f32; 2]; 4],
    /// Concrete texture key, e.g. `blocks/stone`.
    pub texture: String,
    /// Face that may cull this quad against a full opaque neighbour.
    pub cullface: Option<Face>,
    /// Whether vanilla face shading applies (false for cross/plants).
    pub shade: bool,
    /// Optional tint index from the model.
    pub tint: Option<u32>,
}
