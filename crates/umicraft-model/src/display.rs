// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `display` block of an item model: how it is posed in each context.
//!
//! References: MCP `ItemCameraTransforms` (the six contexts and the apply
//! order), `ItemTransformVec3f.Deserializer` (translation in sixteenths of a
//! block, clamped to 1.5; scale clamped to 4) and `ModelBlock.func_181681_a`
//! (a context the child leaves at its default is inherited from the parent).

use serde_json::{Map, Value};

use crate::error::{ModelError, ModelResult};

/// Largest translation magnitude after the sixteenth conversion.
const MAX_TRANSLATION: f32 = 1.5;
/// Largest scale magnitude.
const MAX_SCALE: f32 = 4.0;
/// One sixteenth of a block, the unit of the JSON translation.
const SIXTEENTH: f32 = 0.0625;

/// Where an item model is drawn (MCP `ItemCameraTransforms.TransformType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayContext {
    /// `thirdperson`: held by another player or the F5 view.
    ThirdPerson,
    /// `firstperson`: held by the local player.
    FirstPerson,
    /// `head`: worn on the head (pumpkin, skull).
    Head,
    /// `gui`: inventory slots and the hotbar.
    Gui,
    /// `ground`: a dropped item.
    Ground,
    /// `fixed`: in an item frame.
    Fixed,
}

impl DisplayContext {
    const ALL: [Self; 6] = [
        Self::ThirdPerson,
        Self::FirstPerson,
        Self::Head,
        Self::Gui,
        Self::Ground,
        Self::Fixed,
    ];

    /// The JSON key of the context.
    const fn key(self) -> &'static str {
        match self {
            Self::ThirdPerson => "thirdperson",
            Self::FirstPerson => "firstperson",
            Self::Head => "head",
            Self::Gui => "gui",
            Self::Ground => "ground",
            Self::Fixed => "fixed",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// One pose: the renderer applies translate, rotate Y, X, Z, then scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemTransform {
    /// Euler angles in degrees, applied as Y, then X, then Z.
    pub rotation: [f32; 3],
    /// Offset in blocks (already converted from sixteenths and clamped).
    pub translation: [f32; 3],
    /// Scale factors.
    pub scale: [f32; 3],
}

impl ItemTransform {
    /// The identity pose (MCP `ItemTransformVec3f.DEFAULT`).
    pub const IDENTITY: Self = Self {
        rotation: [0.0; 3],
        translation: [0.0; 3],
        scale: [1.0; 3],
    };

    fn parse(value: &Value, path: &str, context: &str) -> ModelResult<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid(path, format!("display `{context}` must be an object")))?;
        let translation = vector(object, "translation", [0.0; 3], path)?
            .map(|axis| (axis * SIXTEENTH).clamp(-MAX_TRANSLATION, MAX_TRANSLATION));
        Ok(Self {
            rotation: vector(object, "rotation", [0.0; 3], path)?,
            translation,
            scale: vector(object, "scale", [1.0; 3], path)?
                .map(|axis| axis.clamp(-MAX_SCALE, MAX_SCALE)),
        })
    }
}

/// The poses a model declares, one optional entry per context.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayTransforms {
    entries: [Option<ItemTransform>; 6],
}

impl DisplayTransforms {
    /// The pose for `context`, the identity when the chain declares none.
    #[must_use]
    pub fn get(&self, context: DisplayContext) -> ItemTransform {
        self.entries[context.index()].unwrap_or(ItemTransform::IDENTITY)
    }

    /// These poses with the contexts they leave empty taken from `parent`.
    #[must_use]
    pub(crate) fn over(self, parent: &Self) -> Self {
        let mut entries = self.entries;
        for (own, inherited) in entries.iter_mut().zip(parent.entries) {
            *own = own.or(inherited);
        }
        Self { entries }
    }

    /// Reads the `display` object of a model document.
    pub(crate) fn parse(value: &Value, path: &str) -> ModelResult<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid(path, "`display` must be an object"))?;
        let mut entries = [None; 6];
        for context in DisplayContext::ALL {
            if let Some(pose) = object.get(context.key()) {
                entries[context.index()] = Some(ItemTransform::parse(pose, path, context.key())?);
            }
        }
        Ok(Self { entries })
    }
}

fn vector(
    object: &Map<String, Value>,
    key: &str,
    default: [f32; 3],
    path: &str,
) -> ModelResult<[f32; 3]> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let numbers: Option<Vec<f32>> = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_f64().map(|n| n as f32))
                .collect()
        })
        .unwrap_or(None);
    match numbers.as_deref() {
        Some(&[x, y, z]) if [x, y, z].iter().all(|n| n.is_finite()) => Ok([x, y, z]),
        _ => Err(invalid(
            path,
            format!("display `{key}` needs 3 finite numbers"),
        )),
    }
}

fn invalid(path: &str, message: impl Into<String>) -> ModelError {
    ModelError::Invalid {
        path: path.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(value: Value) -> ModelResult<DisplayTransforms> {
        DisplayTransforms::parse(&value, "test.json")
    }

    #[test]
    fn translation_is_in_sixteenths_and_clamped_and_scale_is_clamped() {
        let poses = parse(json!({"gui": {"translation": [16, -400, 0], "scale": [9, 0.5, 1]}}))
            .expect("valid");
        let gui = poses.get(DisplayContext::Gui);
        assert_eq!(gui.translation, [1.0, -1.5, 0.0]);
        assert_eq!(gui.scale, [4.0, 0.5, 1.0]);
        assert_eq!(gui.rotation, [0.0; 3]);
    }

    #[test]
    fn missing_contexts_are_the_identity() {
        let poses = parse(json!({})).expect("valid");
        assert_eq!(poses.get(DisplayContext::Fixed), ItemTransform::IDENTITY);
    }

    #[test]
    fn a_child_inherits_only_the_contexts_it_leaves_out() {
        let parent =
            parse(json!({"gui": {"rotation": [1, 2, 3]}, "head": {"rotation": [9, 9, 9]}}))
                .expect("valid");
        let child = parse(json!({"gui": {"rotation": [4, 5, 6]}})).expect("valid");
        let merged = child.over(&parent);
        assert_eq!(merged.get(DisplayContext::Gui).rotation, [4.0, 5.0, 6.0]);
        assert_eq!(merged.get(DisplayContext::Head).rotation, [9.0, 9.0, 9.0]);
    }

    #[test]
    fn rejects_vectors_that_do_not_have_three_numbers() {
        assert!(parse(json!({"gui": {"rotation": [1, 2]}})).is_err());
        assert!(parse(json!({"gui": {"scale": "big"}})).is_err());
        assert!(parse(json!({"gui": 3})).is_err());
    }
}
