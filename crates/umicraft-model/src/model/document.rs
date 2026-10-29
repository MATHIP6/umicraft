// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of one model JSON document (`models/block/*`, `models/item/*`).
//!
//! References: `ModelBlock.java` (`parent`, `textures`, `elements`,
//! `display`), `BlockPart.java` and `BlockPartFace.java` (element and face
//! validation).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::display::DisplayTransforms;
use crate::error::{ModelError, ModelResult};
use crate::geometry::{ElementRotation, Face, RotationAxis};

use super::loader::MODELS_ROOT;

/// A parsed model document before parent merging.
#[derive(Debug, Clone)]
pub(crate) struct RawModel {
    pub(crate) parent: Option<String>,
    pub(crate) textures: BTreeMap<String, String>,
    pub(crate) elements: Option<Vec<Element>>,
    pub(crate) ambient_occlusion: Option<bool>,
    pub(crate) display: DisplayTransforms,
    /// `gui3d` of this document alone (not inherited, MCP `ModelBlock.gui3d`).
    pub(crate) gui3d: bool,
}

impl RawModel {
    pub(crate) fn parse(bytes: &[u8], path: &str) -> ModelResult<Self> {
        let value: Value = serde_json::from_slice(bytes).map_err(|source| ModelError::Json {
            path: path.to_string(),
            source,
        })?;
        let object = value
            .as_object()
            .ok_or_else(|| invalid(path, "expected a JSON object"))?;

        let parent = match object.get("parent") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.trim().is_empty() => None,
            Some(Value::String(value)) => {
                let normalized = normalize_model_name(value);
                if normalized.is_empty() {
                    return Err(invalid(path, "`parent` is empty"));
                }
                Some(normalized)
            }
            Some(_) => return Err(invalid(path, "`parent` must be a string")),
        };

        let textures = parse_textures(object, path)?;
        let elements = match object.get("elements") {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| parse_element(item, path, index))
                    .collect::<ModelResult<Vec<_>>>()?,
            ),
            Some(_) => return Err(invalid(path, "`elements` must be an array")),
        };
        let ambient_occlusion = match object.get("ambientocclusion") {
            None => None,
            Some(Value::Bool(value)) => Some(*value),
            Some(_) => return Err(invalid(path, "`ambientocclusion` must be a boolean")),
        };

        let display = match object.get("display") {
            None | Some(Value::Null) => DisplayTransforms::default(),
            Some(value) => DisplayTransforms::parse(value, path)?,
        };

        let gui3d = match object.get("gui3d") {
            None => true,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(invalid(path, "`gui3d` must be a boolean")),
        };

        Ok(Self {
            parent,
            textures,
            elements,
            ambient_occlusion,
            display,
            gui3d,
        })
    }
}

/// One `elements` entry.
#[derive(Debug, Clone)]
pub(crate) struct Element {
    pub(crate) from: [f64; 3],
    pub(crate) to: [f64; 3],
    pub(crate) rotation: Option<ElementRotation>,
    pub(crate) faces: BTreeMap<Face, FaceSpec>,
    pub(crate) shade: bool,
}

/// One face of an element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FaceSpec {
    pub(crate) texture: String,
    pub(crate) uv: Option<[f64; 4]>,
    pub(crate) cullface: Option<Face>,
    pub(crate) rotation: u8,
    pub(crate) tint: Option<u32>,
}

fn parse_textures(
    object: &Map<String, Value>,
    path: &str,
) -> ModelResult<BTreeMap<String, String>> {
    let mut textures = BTreeMap::new();
    let Some(value) = object.get("textures") else {
        return Ok(textures);
    };
    let entries = value
        .as_object()
        .ok_or_else(|| invalid(path, "`textures` must be a JSON object"))?;
    for (key, value) in entries {
        let Some(texture) = value.as_str() else {
            return Err(invalid(path, format!("texture `{key}` must be a string")));
        };
        if texture.trim().is_empty() {
            return Err(invalid(path, format!("texture `{key}` is empty")));
        }
        textures.insert(key.clone(), texture.trim().to_string());
    }
    Ok(textures)
}

fn parse_element(value: &Value, path: &str, index: usize) -> ModelResult<Element> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(path, format!("element {index} must be a JSON object")))?;
    let from = parse_position(
        object
            .get("from")
            .ok_or_else(|| invalid(path, format!("element {index} has no `from`")))?,
        path,
        "from",
        true,
    )?;
    let to = parse_position(
        object
            .get("to")
            .ok_or_else(|| invalid(path, format!("element {index} has no `to`")))?,
        path,
        "to",
        true,
    )?;
    let rotation = match object.get("rotation") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_rotation(value, path)?),
    };
    let faces_object = object
        .get("faces")
        .ok_or_else(|| invalid(path, format!("element {index} has no `faces`")))?
        .as_object()
        .ok_or_else(|| invalid(path, format!("element {index} `faces` must be an object")))?;
    if faces_object.is_empty() {
        return Err(invalid(
            path,
            format!("element {index} has no faces (expected 1 to 6)"),
        ));
    }

    let mut faces = BTreeMap::new();
    for (name, value) in faces_object {
        let face = Face::parse(name)
            .ok_or_else(|| invalid(path, format!("element {index} has unknown face `{name}`")))?;
        faces.insert(face, parse_face(value, path)?);
    }
    let shade = match object.get("shade") {
        None => true,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(invalid(
                path,
                format!("element {index} `shade` must be a boolean"),
            ))
        }
    };

    Ok(Element {
        from,
        to,
        rotation,
        faces,
        shade,
    })
}

fn parse_rotation(value: &Value, path: &str) -> ModelResult<ElementRotation> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(path, "`rotation` must be an object"))?;
    let origin = parse_position(
        object
            .get("origin")
            .ok_or_else(|| invalid(path, "`rotation.origin` is missing"))?,
        path,
        "rotation.origin",
        false,
    )?;
    let axis = match object.get("axis").and_then(Value::as_str) {
        Some("x") => RotationAxis::X,
        Some("y") => RotationAxis::Y,
        Some("z") => RotationAxis::Z,
        Some(other) => {
            return Err(invalid(
                path,
                format!("`rotation.axis` is `{other}`, expected x, y or z"),
            ))
        }
        None => return Err(invalid(path, "`rotation.axis` must be a string")),
    };
    let angle = required_number(object, "angle", path)?;
    if angle != 0.0 && angle.abs() != 22.5 && angle.abs() != 45.0 {
        return Err(invalid(
            path,
            format!("rotation angle {angle} is not one of -45/-22.5/0/22.5/45"),
        ));
    }
    let rescale = match object.get("rescale") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(invalid(path, "`rotation.rescale` must be a boolean")),
    };
    Ok(ElementRotation {
        origin,
        axis,
        angle,
        rescale,
    })
}

fn parse_face(value: &Value, path: &str) -> ModelResult<FaceSpec> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(path, "a face must be a JSON object"))?;
    let texture = object
        .get("texture")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(path, "a face has no string `texture`"))?
        .trim()
        .to_string();
    if texture.is_empty() {
        return Err(invalid(path, "a face has an empty `texture`"));
    }
    let uv = match object.get("uv") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_uv(value, path)?),
    };
    let cullface = match object.get("cullface") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(
            Face::parse(name)
                .ok_or_else(|| invalid(path, format!("unknown `cullface` `{name}`")))?,
        ),
        Some(_) => return Err(invalid(path, "`cullface` must be a string")),
    };
    let rotation = match object.get("rotation") {
        None => 0,
        Some(value) => {
            let degrees = value
                .as_i64()
                .ok_or_else(|| invalid(path, "face `rotation` must be an integer"))?;
            match degrees {
                0 => 0,
                90 => 1,
                180 => 2,
                270 => 3,
                _ => {
                    return Err(invalid(
                        path,
                        format!("face rotation {degrees} is not 0/90/180/270"),
                    ))
                }
            }
        }
    };
    let tint = match object.get("tintindex") {
        None => None,
        Some(value) => {
            let tint = value
                .as_i64()
                .ok_or_else(|| invalid(path, "`tintindex` must be an integer"))?;
            if tint < -1 {
                return Err(invalid(path, "`tintindex` must be -1 or greater"));
            }
            u32::try_from(tint).ok()
        }
    };

    Ok(FaceSpec {
        texture,
        uv,
        cullface,
        rotation,
        tint,
    })
}

fn parse_position(value: &Value, path: &str, label: &str, bounded: bool) -> ModelResult<[f64; 3]> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid(path, format!("`{label}` must be an array of three numbers")))?;
    if array.len() != 3 {
        return Err(invalid(
            path,
            format!("`{label}` needs 3 values, found {}", array.len()),
        ));
    }
    let mut position = [0.0; 3];
    for (index, value) in array.iter().enumerate() {
        let number = number_value(value, path, label)?;
        if bounded && !(-16.0..=32.0).contains(&number) {
            return Err(invalid(
                path,
                format!("`{label}` value {number} is outside -16..32"),
            ));
        }
        position[index] = number;
    }
    Ok(position)
}

fn parse_uv(value: &Value, path: &str) -> ModelResult<[f64; 4]> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid(path, "`uv` must be an array of four numbers"))?;
    if array.len() != 4 {
        return Err(invalid(
            path,
            format!("`uv` needs 4 values, found {}", array.len()),
        ));
    }
    let mut uv = [0.0; 4];
    for (index, value) in array.iter().enumerate() {
        uv[index] = number_value(value, path, "uv")?;
    }
    Ok(uv)
}

fn required_number(object: &Map<String, Value>, key: &str, path: &str) -> ModelResult<f64> {
    let value = object
        .get(key)
        .ok_or_else(|| invalid(path, format!("missing `{key}`")))?;
    number_value(value, path, key)
}

fn number_value(value: &Value, path: &str, label: &str) -> ModelResult<f64> {
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| invalid(path, format!("`{label}` must be a finite number")))?;
    Ok(number)
}

/// Resolves a face texture (`#all` or a concrete key) to a concrete key by
/// following `#` references transitively.
pub(crate) fn resolve_texture_value(
    textures: &BTreeMap<String, String>,
    value: &str,
    model: &str,
) -> ModelResult<String> {
    let mut visited = BTreeSet::new();
    let mut current = value;
    loop {
        let Some(reference) = current.strip_prefix('#') else {
            let concrete = strip_namespace(current);
            if concrete.is_empty() {
                return Err(ModelError::UnresolvedTexture {
                    reference: value.to_string(),
                    path: model_path(model),
                });
            }
            return Ok(concrete.to_string());
        };
        if !visited.insert(reference.to_string()) {
            return Err(ModelError::UnresolvedTexture {
                reference: format!("#{reference}"),
                path: model_path(model),
            });
        }
        current = textures
            .get(reference)
            .ok_or_else(|| ModelError::UnresolvedTexture {
                reference: format!("#{reference}"),
                path: model_path(model),
            })?;
    }
}

/// Strips the `minecraft:` namespace from a texture or model name.
pub(crate) fn strip_namespace(value: &str) -> &str {
    value.strip_prefix("minecraft:").unwrap_or(value)
}

/// Normalizes a model reference to a key below `assets/minecraft/models/`.
///
/// Blockstate variants name their model without a directory (`stone`), which
/// means `block/stone`; parents already carry theirs (`block/cube`,
/// `item/generated`, `builtin/entity`).
pub(crate) fn normalize_model_name(raw: &str) -> String {
    let trimmed = strip_namespace(raw.trim());
    let trimmed = trimmed.strip_suffix(".json").unwrap_or(trimmed);
    if ["block/", "item/", "builtin/"]
        .iter()
        .any(|dir| trimmed.starts_with(dir))
    {
        trimmed.to_string()
    } else {
        format!("block/{trimmed}")
    }
}

pub(crate) fn model_path(name: &str) -> String {
    format!("{MODELS_ROOT}{name}.json")
}

pub(crate) fn invalid(path: &str, message: impl Into<String>) -> ModelError {
    ModelError::Invalid {
        path: path.to_string(),
        message: message.into(),
    }
}

pub(crate) fn resource(path: &str, message: impl Into<String>) -> ModelError {
    ModelError::Resource {
        path: path.to_string(),
        message: message.into(),
    }
}
