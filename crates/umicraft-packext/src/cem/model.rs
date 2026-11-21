// SPDX-License-Identifier: MIT OR Apache-2.0

//! The typed form of a `.jem` file: a tree of parts with boxes.
//!
//! Coordinates stay in the space of the file, which is the model space of the
//! vanilla 1.8 models (pixels, y pointing down, angles in radians). The
//! consumer converts to its own space.

/// Where a texture or model file is, after the path rules of the format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackPath {
    /// Below the pack-feature folder (`optifine/` or `mcpatcher/`), for
    /// example `cem/creeper.png`.
    Feature(String),
    /// Below `assets/minecraft/`, for example `textures/entity/creeper.png`.
    Resource(String),
}

/// How a box takes its texels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxUv {
    /// The usual unfolded layout starting at this offset in pixels.
    Offset([f32; 2]),
    /// One rectangle `[u1, v1, u2, v2]` per face, in the order down, up,
    /// north, south, west, east; a face without one is not drawn.
    Faces([Option<[f32; 4]>; 6]),
}

/// A cuboid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CemBox {
    /// Minimum corner, after `invertAxis`.
    pub origin: [f32; 3],
    /// Width, height and depth.
    pub size: [f32; 3],
    /// Growth on every side.
    pub size_add: f32,
    /// The texels.
    pub uv: BoxUv,
}

/// One part: a pivot, a rotation, boxes and child parts.
#[derive(Debug, Clone, PartialEq)]
pub struct CemPart {
    /// The id other parts and animations name it by.
    pub id: Option<String>,
    /// A texture of its own, which the consumer may ignore.
    pub texture: Option<PackPath>,
    /// Texture size in pixels (own, else the parent's, else the file's).
    pub texture_size: Option<[f32; 2]>,
    /// Rotation point, after `invertAxis`.
    pub translate: [f32; 3],
    /// Rotation in radians, after `invertAxis`.
    pub rotate: [f32; 3],
    /// Uniform scale.
    pub scale: f32,
    /// Mirrors the texture horizontally.
    pub mirror_u: bool,
    /// Mirrors the texture vertically.
    pub mirror_v: bool,
    /// The cuboids.
    pub boxes: Vec<CemBox>,
    /// Number of flat sprites; they are parsed but not drawn.
    pub sprites: usize,
    /// Child parts, which move with this one.
    pub children: Vec<CemPart>,
}

impl CemPart {
    /// This part and its descendants, depth first, parent before children.
    pub fn walk<'a>(&'a self, out: &mut Vec<&'a CemPart>) {
        out.push(self);
        for child in &self.children {
            child.walk(out);
        }
    }
}

/// One `animations` entry: assign `expression` to `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CemAnimation {
    /// The variable, such as `head.ry` or `var.counter`.
    pub target: String,
    /// The expression text.
    pub expression: String,
}

/// One entry of `models`.
#[derive(Debug, Clone, PartialEq)]
pub struct CemEntry {
    /// The vanilla part this replaces or attaches to.
    pub part: String,
    /// `true` adds to the vanilla part, `false` replaces it.
    pub attach: bool,
    /// The custom part tree.
    pub root: CemPart,
    /// The animations of this entry.
    pub animations: Vec<CemAnimation>,
}

/// A parsed `.jem` file.
#[derive(Debug, Clone, PartialEq)]
pub struct CemModel {
    /// The entity, from the file name (`creeper` for `creeper.jem`).
    pub entity: String,
    /// The texture of the whole model.
    pub texture: Option<PackPath>,
    /// Texture size in pixels.
    pub texture_size: Option<[f32; 2]>,
    /// The shadow radius, when the file sets one.
    pub shadow_size: Option<f32>,
    /// The part entries, in file order.
    pub entries: Vec<CemEntry>,
}

impl CemModel {
    /// Every part of every entry, depth first, in the order animation
    /// poses are indexed by.
    #[must_use]
    pub fn parts(&self) -> Vec<&CemPart> {
        let mut out = Vec::new();
        for entry in &self.entries {
            entry.root.walk(&mut out);
        }
        out
    }
}
