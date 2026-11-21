// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reads `.jem` / `.jpm` JSON into a [`CemModel`].
//!
//! Keys and rules follow the public CEM documentation (`cem_model.txt`,
//! `cem_part.txt`) and the parser of the reference tree (ADR-0088): `baseId`
//! copies the keys an earlier entry has, `model` merges a `.jpm` file, and
//! neither overrides a key the entry sets itself.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::model::{BoxUv, CemAnimation, CemBox, CemEntry, CemModel, CemPart, PackPath};

/// Why a model file is rejected.
#[derive(Debug, thiserror::Error)]
pub enum CemError {
    /// Not valid JSON.
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A value of the wrong JSON type.
    #[error("{key} must be {expected}")]
    Type {
        /// The key.
        key: &'static str,
        /// What it must be.
        expected: &'static str,
    },
    /// A required key is missing.
    #[error("missing {0}")]
    Missing(&'static str),
    /// An array of the wrong length.
    #[error("{key} needs {len} numbers")]
    Length {
        /// The key.
        key: &'static str,
        /// The length it needs.
        len: usize,
    },
    /// A box that sets neither `textureOffset` nor any face.
    #[error("a box has no textureOffset and no face UVs")]
    NoUv,
    /// Parts nested deeper than allowed.
    #[error("parts are nested too deeply")]
    TooDeep,
}

/// The deepest part nesting accepted.
const MAX_PART_DEPTH: usize = 32;

/// Reads a file from the pack, by feature-relative path.
pub type Loader<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// Parses the `.jem` at the feature-relative `path` (`cem/creeper.jem`).
///
/// `load` fetches the `.jpm` files that entries refer to; one that is missing
/// or invalid is skipped with a warning, as the reference does.
///
/// # Errors
///
/// A [`CemError`] when the file is not JSON, lacks `models`, or has a part or
/// box that is malformed. The caller drops the whole model then.
pub fn parse_jem(path: &str, bytes: &[u8], load: &Loader<'_>) -> Result<CemModel, CemError> {
    let root: Value = serde_json::from_slice(bytes)?;
    let obj = root.as_object().ok_or(CemError::Type {
        key: "the file",
        expected: "an object",
    })?;
    let base = parent_dir(path);
    let texture_size = size_pair(obj.get("textureSize"), "textureSize")?;
    let models = obj
        .get("models")
        .ok_or(CemError::Missing("models"))?
        .as_array()
        .ok_or(CemError::Type {
            key: "models",
            expected: "an array",
        })?;
    let mut by_id: HashMap<String, Map<String, Value>> = HashMap::new();
    let mut entries = Vec::with_capacity(models.len());
    for model in models {
        let mut entry = model.as_object().cloned().ok_or(CemError::Type {
            key: "models",
            expected: "objects",
        })?;
        merge_base_id(&mut entry, &by_id);
        merge_external(&mut entry, base, load);
        if let Some(id) = entry.get("id").and_then(Value::as_str) {
            if id.is_empty() || by_id.contains_key(id) {
                tracing::warn!(%id, "empty or duplicate model id");
            } else {
                by_id.insert(id.to_string(), entry.clone());
            }
        }
        entries.push(parse_entry(&entry, texture_size, base)?);
    }
    Ok(CemModel {
        entity: file_stem(path).to_string(),
        texture: str_key(obj, "texture").map(|t| resolve_path(base, t, ".png")),
        texture_size,
        shadow_size: obj
            .get("shadowSize")
            .and_then(Value::as_f64)
            .map(|v| v as f32),
        entries,
    })
}

/// Resolves a texture or model path of a file in `base` (both feature
/// relative): no `/` is next to the file, `./` too, `~/` is the feature root,
/// anything else is below `assets/minecraft/`. `extension` is appended when
/// missing. A `namespace:` prefix is dropped.
#[must_use]
pub fn resolve_path(base: &str, path: &str, extension: &str) -> PackPath {
    let mut path = path.to_string();
    if !path.ends_with(extension) {
        path.push_str(extension);
    }
    if let Some((_, rest)) = path.split_once(':') {
        path = rest.to_string();
    }
    if !path.contains('/') {
        PackPath::Feature(join(base, &path))
    } else if let Some(rest) = path.strip_prefix("./") {
        PackPath::Feature(join(base, rest))
    } else if let Some(rest) = path.strip_prefix("~/") {
        PackPath::Feature(rest.to_string())
    } else {
        PackPath::Resource(path)
    }
}

fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else {
        format!("{base}/{name}")
    }
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn str_key<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

/// Copies the keys of the earlier entry `baseId` names, except `id`.
fn merge_base_id(entry: &mut Map<String, Value>, by_id: &HashMap<String, Map<String, Value>>) {
    let Some(base_id) = str_key(entry, "baseId").map(str::to_string) else {
        return;
    };
    match by_id.get(&base_id) {
        Some(base) => copy_missing(base, entry),
        None => tracing::warn!(%base_id, "baseId not found"),
    }
}

/// Merges the `.jpm` file `model` names.
fn merge_external(entry: &mut Map<String, Value>, base: &str, load: &Loader<'_>) {
    let Some(name) = str_key(entry, "model").map(str::to_string) else {
        return;
    };
    let PackPath::Feature(path) = resolve_path(base, &name, ".jpm") else {
        tracing::warn!(%name, "a model file must be inside the feature folder");
        return;
    };
    let parsed = load(&path).and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    match parsed {
        Some(Value::Object(external)) => copy_missing(&external, entry),
        _ => tracing::warn!(%path, "model file missing or invalid"),
    }
}

fn copy_missing(from: &Map<String, Value>, to: &mut Map<String, Value>) {
    for (key, value) in from {
        if key != "id" && !to.contains_key(key) {
            to.insert(key.clone(), value.clone());
        }
    }
}

fn parse_entry(
    entry: &Map<String, Value>,
    texture_size: Option<[f32; 2]>,
    base: &str,
) -> Result<CemEntry, CemError> {
    let part = str_key(entry, "part")
        .ok_or(CemError::Missing("part"))?
        .to_string();
    let attach = entry
        .get("attach")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let animations = parse_animations(entry.get("animations"))?;
    let root = parse_part(entry, texture_size, base, 0)?;
    Ok(CemEntry {
        part,
        attach,
        root,
        animations,
    })
}

fn parse_animations(value: Option<&Value>) -> Result<Vec<CemAnimation>, CemError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let list = value.as_array().ok_or(CemError::Type {
        key: "animations",
        expected: "an array",
    })?;
    let mut out = Vec::new();
    for item in list {
        let pairs = item.as_object().ok_or(CemError::Type {
            key: "animations",
            expected: "objects",
        })?;
        for (target, expression) in pairs {
            // The reference reads any value as text, so `0.5` works too.
            let expression = match expression {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            out.push(CemAnimation {
                target: target.clone(),
                expression,
            });
        }
    }
    Ok(out)
}

/// A float array of exactly `len` numbers, or `None` when the key is absent.
fn floats<const N: usize>(
    obj: &Map<String, Value>,
    key: &'static str,
) -> Result<Option<[f32; N]>, CemError> {
    let Some(value) = obj.get(key) else {
        return Ok(None);
    };
    let list = value
        .as_array()
        .filter(|list| list.len() == N)
        .ok_or(CemError::Length { key, len: N })?;
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(list) {
        *slot = item.as_f64().ok_or(CemError::Length { key, len: N })? as f32;
    }
    Ok(Some(out))
}

fn size_pair(value: Option<&Value>, key: &'static str) -> Result<Option<[f32; 2]>, CemError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let mut wrapper = Map::new();
    wrapper.insert(key.to_string(), value.clone());
    floats::<2>(&wrapper, key)
}

fn parse_part(
    obj: &Map<String, Value>,
    parent_size: Option<[f32; 2]>,
    base: &str,
    depth: usize,
) -> Result<CemPart, CemError> {
    if depth > MAX_PART_DEPTH {
        return Err(CemError::TooDeep);
    }
    let texture_size = floats::<2>(obj, "textureSize")?.or(parent_size);
    let invert = str_key(obj, "invertAxis")
        .unwrap_or("")
        .to_ascii_lowercase();
    let flips = ["x", "y", "z"].map(|axis| invert.contains(axis));
    let mut translate = floats::<3>(obj, "translate")?.unwrap_or_default();
    let mut rotate = floats::<3>(obj, "rotate")?.unwrap_or_default();
    for axis in 0..3 {
        rotate[axis] = rotate[axis].to_radians();
        if flips[axis] {
            translate[axis] = -translate[axis];
            rotate[axis] = -rotate[axis];
        }
    }
    let mirror = str_key(obj, "mirrorTexture")
        .unwrap_or("")
        .to_ascii_lowercase();
    let boxes = parse_boxes(obj.get("boxes"), flips)?;
    let sprites = obj
        .get("sprites")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut children = Vec::new();
    if let Some(single) = obj.get("submodel") {
        children.push(parse_child(single, texture_size, base, depth)?);
    }
    if let Some(list) = obj.get("submodels") {
        let list = list.as_array().ok_or(CemError::Type {
            key: "submodels",
            expected: "an array",
        })?;
        for child in list {
            children.push(parse_child(child, texture_size, base, depth)?);
        }
    }
    Ok(CemPart {
        id: str_key(obj, "id").map(str::to_string),
        texture: str_key(obj, "texture").map(|t| resolve_path(base, t, ".png")),
        texture_size,
        translate,
        rotate,
        scale: obj
            .get("scale")
            .and_then(Value::as_f64)
            .map_or(1.0, |s| s as f32),
        mirror_u: mirror.contains('u'),
        mirror_v: mirror.contains('v'),
        boxes,
        sprites,
        children,
    })
}

fn parse_child(
    value: &Value,
    texture_size: Option<[f32; 2]>,
    base: &str,
    depth: usize,
) -> Result<CemPart, CemError> {
    let obj = value.as_object().ok_or(CemError::Type {
        key: "submodel",
        expected: "an object",
    })?;
    parse_part(obj, texture_size, base, depth + 1)
}

fn parse_boxes(value: Option<&Value>, flips: [bool; 3]) -> Result<Vec<CemBox>, CemError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let list = value.as_array().ok_or(CemError::Type {
        key: "boxes",
        expected: "an array",
    })?;
    list.iter().map(|item| parse_box(item, flips)).collect()
}

fn parse_box(value: &Value, flips: [bool; 3]) -> Result<CemBox, CemError> {
    let obj = value.as_object().ok_or(CemError::Type {
        key: "boxes",
        expected: "objects",
    })?;
    let uv = match (floats::<2>(obj, "textureOffset")?, face_uvs(obj)?) {
        (_, Some(faces)) => BoxUv::Faces(faces),
        (Some(offset), None) => BoxUv::Offset(offset),
        (None, None) => return Err(CemError::NoUv),
    };
    let c = floats::<6>(obj, "coordinates")?.ok_or(CemError::Missing("coordinates"))?;
    let mut origin = [c[0], c[1], c[2]];
    let size = [c[3], c[4], c[5]];
    for axis in 0..3 {
        if flips[axis] {
            origin[axis] = -origin[axis] - size[axis];
        }
    }
    Ok(CemBox {
        origin,
        size,
        size_add: obj
            .get("sizeAdd")
            .and_then(Value::as_f64)
            .map_or(0.0, |v| v as f32),
        uv,
    })
}

/// The per-face rectangles, or `None` when the box sets none.
fn face_uvs(obj: &Map<String, Value>) -> Result<Option<[Option<[f32; 4]>; 6]>, CemError> {
    let get = |key: &'static str, alias: Option<&'static str>| -> Result<_, CemError> {
        match floats::<4>(obj, key)? {
            Some(face) => Ok(Some(face)),
            None => alias.map_or(Ok(None), |alias| floats::<4>(obj, alias)),
        }
    };
    let faces = [
        get("uvDown", None)?,
        get("uvUp", None)?,
        get("uvNorth", Some("uvFront"))?,
        get("uvSouth", Some("uvBack"))?,
        get("uvWest", Some("uvLeft"))?,
        get("uvEast", Some("uvRight"))?,
    ];
    Ok(faces.iter().any(Option::is_some).then_some(faces))
}

#[cfg(test)]
mod tests;
