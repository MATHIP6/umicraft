// SPDX-License-Identifier: MIT OR Apache-2.0

//! Natural textures: a block face turns or flips by a hash of its position, so
//! a field of one texture stops repeating in obvious tiles.
//!
//! `optifine/natural.properties` maps a block-texture name to a kind:
//!
//! | value | effect |
//! |---|---|
//! | `4` | a quarter turn picked from four |
//! | `2` | a half turn or none |
//! | `F` | a mirror or none |
//! | `4F`, `2F` | both |
//!
//! [`NaturalRules::parse`] reads the file. [`Natural::variant`] picks the turn
//! and the mirror of one face, and [`Variant::apply`] permutes the four
//! texture coordinates of a quad. The choice is a pure function of the block
//! position and the face, so a block looks the same on every visit.
//!
//! Behaviour from OptiFine's `NaturalTextures` and `NaturalProperties`
//! (ADR-0088, `docs/sources.md`).

use thiserror::Error;

use crate::ctm::position_hash;
use crate::{Properties, PropertiesError};

/// Texture coordinates may differ from a sprite corner by this much and still
/// count as a corner (`NaturalProperties.isFullSprite`: one 256th of the
/// sprite).
const CORNER_TOLERANCE: f32 = 1.0 / 256.0;

/// Why a line of `natural.properties` was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NaturalError {
    /// The value is none of `4`, `2`, `F`, `4F`, `2F`.
    #[error("unknown natural-texture kind {0:?}")]
    UnknownKind(String),
}

/// How many turns a texture may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turns {
    /// No rotation.
    None,
    /// Zero or two quarter turns.
    Half,
    /// Zero to three quarter turns.
    Quarter,
}

/// The kind of one texture: how it may turn and whether it may mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Natural {
    turns: Turns,
    flip: bool,
}

/// The turn and the mirror chosen for one face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant {
    /// Quarter turns, 0 to 3.
    pub turns: u8,
    /// Whether the texture is mirrored.
    pub flip: bool,
}

impl Natural {
    /// Parses a kind: `4`, `2`, `F`, `4F` or `2F`.
    ///
    /// # Errors
    ///
    /// [`NaturalError::UnknownKind`] for any other text.
    pub fn parse(text: &str) -> Result<Self, NaturalError> {
        let (turns, flip) = match text {
            "4" => (Turns::Quarter, false),
            "2" => (Turns::Half, false),
            "F" => (Turns::None, true),
            "4F" => (Turns::Quarter, true),
            "2F" => (Turns::Half, true),
            other => return Err(NaturalError::UnknownKind(other.to_string())),
        };
        Ok(Self { turns, flip })
    }

    /// The turn and mirror of the face numbered `side` (`ctm::Side`, `-1` when
    /// unknown) of the block at `position`.
    #[must_use]
    pub fn variant(self, position: [i32; 3], side: i32) -> Variant {
        let hash = position_hash(position, side);
        let turns = match self.turns {
            Turns::None => 0,
            Turns::Half => (hash & 2) as u8,
            Turns::Quarter => (hash & 3) as u8,
        };
        Variant {
            turns,
            flip: self.flip && hash & 4 != 0,
        }
    }
}

impl Variant {
    /// Whether the quad is left as it is.
    #[must_use]
    pub fn is_identity(self) -> bool {
        self.turns == 0 && !self.flip
    }

    /// Permutes the corner texture coordinates of a quad. Coordinates that do
    /// not span the whole sprite (a partial block face) are returned as they
    /// are, because turning them would show another part of the texture
    /// (`NaturalProperties.makeQuad` keeps the mirror but drops the turn
    /// there; a mirror of a partial face is applied the same way here).
    #[must_use]
    pub fn apply(self, uvs: [[f32; 2]; 4]) -> [[f32; 2]; 4] {
        let turns = if spans_sprite(&uvs) { self.turns } else { 0 };
        if turns == 0 && !self.flip {
            return uvs;
        }
        let mut start = 4 - usize::from(turns);
        if self.flip {
            start += 3;
        }
        let mut out = uvs;
        for (corner, uv) in uvs.into_iter().enumerate() {
            let slot = if self.flip {
                (start + 4 - corner) % 4
            } else {
                (start + corner) % 4
            };
            out[slot] = uv;
        }
        out
    }
}

/// Whether every coordinate is within a tolerance of 0 or 1.
fn spans_sprite(uvs: &[[f32; 2]; 4]) -> bool {
    let near_edge =
        |value: f32| value.abs() < CORNER_TOLERANCE || (value - 1.0).abs() < CORNER_TOLERANCE;
    uvs.iter().flatten().all(|value| near_edge(*value))
}

/// The parsed `natural.properties`: texture names and their kinds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NaturalRules {
    /// `(name, kind)` in key order. The name is relative to `textures/blocks/`
    /// without the extension, as written in the file (`grass_top`).
    pub entries: Vec<(String, Natural)>,
    /// The lines that were dropped, with the reason.
    pub rejected: Vec<(String, NaturalError)>,
}

impl NaturalRules {
    /// Parses `bytes`. A line with a bad kind is recorded in
    /// [`rejected`](Self::rejected) and costs only itself.
    ///
    /// # Errors
    ///
    /// [`PropertiesError`] when the file's escapes are broken.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_packext::natural::NaturalRules;
    ///
    /// let rules = NaturalRules::parse(b"grass_top=4F\nstone=2")?;
    /// assert_eq!(rules.entries.len(), 2);
    /// # Ok::<(), umicraft_packext::PropertiesError>(())
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<Self, PropertiesError> {
        let properties = Properties::parse(bytes)?;
        let mut rules = Self::default();
        for key in properties.keys() {
            let value = properties.get(key).unwrap_or_default();
            match Natural::parse(value) {
                Ok(natural) => rules.entries.push((key.to_string(), natural)),
                Err(error) => rules.rejected.push((key.to_string(), error)),
            }
        }
        Ok(rules)
    }
}

#[cfg(test)]
mod tests;
