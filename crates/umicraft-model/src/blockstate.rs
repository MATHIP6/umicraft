// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of 1.8 blockstate documents (`variants` only; 1.8 has no
//! `multipart` blockstates).
//!
//! A variant key is a comma-separated list of `name=value` pairs. The parser
//! stores every key in canonical form (pairs sorted by name), so the order in
//! the JSON document never affects selection. The bare key `normal` is the
//! canonical spelling of the empty property set used by single-variant blocks
//! such as stone.
//!
//! Reference: `mcp: src/minecraft/client/renderer/block/model/ModelBlockDefinition.java`
//! (variant weights, x/y rotation and `uvlock`) and
//! `assets/minecraft/blockstates/*.json` for the key syntax (ADR-0001).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::error::{ModelError, ModelResult};

/// One model reference in a blockstate variant list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// Model name relative to `models/block`, e.g. `stone` or `half_slab_stone`.
    pub model: String,
    /// X rotation in degrees (a multiple of 90, normalized when baked).
    pub x: i32,
    /// Y rotation in degrees (a multiple of 90, normalized when baked).
    pub y: i32,
    /// Whether UVs rotate with the model.
    pub uvlock: bool,
    /// Selection weight; 1 when omitted.
    pub weight: u32,
}

/// A parsed blockstate document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockStateFile {
    /// Canonical variant key (sorted `a=b,c=d`, or `normal`) to weighted models.
    pub variants: BTreeMap<String, Vec<Variant>>,
}

impl BlockStateFile {
    /// Parses one blockstate document.
    ///
    /// `path` is the virtual path used in errors. Weights default to `1` and
    /// must be positive; `x` and `y` default to `0` and must be multiples of
    /// 90. An object value is a single variant, an array value is a weighted
    /// variant list.
    ///
    /// # Errors
    /// [`ModelError::Json`] for malformed JSON and [`ModelError::Invalid`] for
    /// a document without `variants`, an empty variant list, an invalid
    /// rotation, an invalid weight, or a malformed variant key.
    pub fn parse(bytes: &[u8], path: &str) -> ModelResult<Self> {
        let value: Value = serde_json::from_slice(bytes).map_err(|source| ModelError::Json {
            path: path.to_string(),
            source,
        })?;
        Self::from_value(&value, path)
    }

    /// Parses an already decoded JSON value; [`Self::parse`] is the normal
    /// entry point.
    fn from_value(value: &Value, path: &str) -> ModelResult<Self> {
        let root = value
            .as_object()
            .ok_or_else(|| invalid(path, "expected a JSON object"))?;
        let raw_variants = root
            .get("variants")
            .ok_or_else(|| invalid(path, "blockstate has no `variants` object"))?
            .as_object()
            .ok_or_else(|| invalid(path, "`variants` must be a JSON object"))?;

        let mut variants: BTreeMap<String, Vec<Variant>> = BTreeMap::new();
        for (raw_key, raw_value) in raw_variants {
            let key =
                canonicalize_variant_key(raw_key).map_err(|message| invalid(path, message))?;
            let parsed = parse_variant_list(raw_value)
                .map_err(|message| invalid(path, format!("variant `{raw_key}`: {message}")))?;
            // Two spellings of the same property set are merged rather than
            // rejected: the canonical key is the one the registry selects.
            variants.entry(key).or_default().extend(parsed);
        }

        if variants.is_empty() {
            return Err(invalid(path, "`variants` contains no entries"));
        }
        Ok(Self { variants })
    }

    /// Looks up a variant key; `None` when the key is absent.
    ///
    /// The lookup key is canonicalized first, so `b=2,a=1` finds the stored
    /// `a=1,b=2` entry. An empty key is the same as `normal`.
    #[must_use]
    pub fn select(&self, key: &str) -> Option<&[Variant]> {
        let key = canonicalize_variant_key(key).ok()?;
        self.variants.get(&key).map(Vec::as_slice)
    }

    /// First variant in canonical key order, used when a metadata mapping does
    /// not match.
    ///
    /// A blockstate document normally has one variant per key; the canonical
    /// order makes the fallback deterministic even for multi-key documents.
    #[must_use]
    pub fn first(&self) -> Option<(&str, &[Variant])> {
        self.variants
            .iter()
            .next()
            .map(|(key, variants)| (key.as_str(), variants.as_slice()))
    }
}

impl Variant {
    /// Parses a variant key into its property pairs.
    ///
    /// `normal`, the empty string and bare named keys such as `all` or `map`
    /// have no properties, so they map to an empty property map. The result is
    /// sorted by property name because it is a [`BTreeMap`].
    #[must_use]
    pub fn parse_key(key: &str) -> BTreeMap<String, String> {
        let key = key.trim();
        if key.is_empty() || key == "normal" {
            return BTreeMap::new();
        }
        key.split(',')
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
            .collect()
    }
}

/// Produces the canonical spelling of a variant key.
///
/// Property keys (`a=1,b=2`) are sorted by property name. Bare names such as
/// `normal`, `all` and `map` are literal variant names in 1.8: the double-slab
/// and item-frame documents use them to select a second model, so they must not
/// collapse to the empty property set.
pub(crate) fn canonicalize_variant_key(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "normal" {
        return Ok("normal".to_string());
    }
    if !raw.contains('=') {
        return Ok(raw.to_string());
    }

    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut names = BTreeSet::new();
    for pair in raw.split(',') {
        let pair = pair.trim();
        let (name, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("variant key `{raw}` has a property without `=`"))?;
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            return Err(format!(
                "variant key `{raw}` has an empty property name or value"
            ));
        }
        if !names.insert(name.to_string()) {
            return Err(format!("variant key `{raw}` repeats property `{name}`"));
        }
        pairs.push((name.to_string(), value.to_string()));
    }
    pairs.sort();
    Ok(pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(","))
}

/// Parses the value of a `variants` entry: one object or an array of objects.
fn parse_variant_list(value: &Value) -> Result<Vec<Variant>, String> {
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                return Err("the variant array is empty".to_string());
            }
            items.iter().map(parse_variant).collect()
        }
        Value::Object(_) => Ok(vec![parse_variant(value)?]),
        _ => Err("expected an object or an array of objects".to_string()),
    }
}

/// Parses one variant object.
fn parse_variant(value: &Value) -> Result<Variant, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "expected a variant object".to_string())?;

    let model = required_string(object, "model")?;
    if model.trim().is_empty() {
        return Err("`model` is empty".to_string());
    }
    let x = optional_i32(object, "x", 0)?;
    let y = optional_i32(object, "y", 0)?;
    if x % 90 != 0 {
        return Err(format!("x rotation {x} is not a multiple of 90"));
    }
    if y % 90 != 0 {
        return Err(format!("y rotation {y} is not a multiple of 90"));
    }
    let uvlock = optional_bool(object, "uvlock", false)?;
    let weight = match object.get("weight") {
        None => 1,
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|weight| u32::try_from(weight).ok())
            .ok_or_else(|| "`weight` must be a positive 32-bit integer".to_string())?,
        Some(_) => return Err("`weight` must be an integer".to_string()),
    };
    if weight == 0 {
        return Err("`weight` must be greater than zero".to_string());
    }

    Ok(Variant {
        model: model.trim().to_string(),
        x,
        y,
        uvlock,
        weight,
    })
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, String> {
    match object.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(format!("`{key}` must be a string")),
        None => Err(format!("missing `{key}`")),
    }
}

fn optional_bool(object: &Map<String, Value>, key: &str, default: bool) -> Result<bool, String> {
    match object.get(key) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("`{key}` must be a boolean")),
    }
}

fn optional_i32(object: &Map<String, Value>, key: &str, default: i32) -> Result<i32, String> {
    match object.get(key) {
        None => Ok(default),
        Some(Value::Number(number)) => number
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| format!("`{key}` must be a 32-bit integer")),
        Some(_) => Err(format!("`{key}` must be an integer")),
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

    const PATH: &str = "assets/minecraft/blockstates/test.json";

    #[test]
    fn keys_are_canonicalized_and_normal_is_empty() {
        let parsed = BlockStateFile::parse(
            br#"{"variants":{"b=2,a=1":{"model":"stone"},"normal":{"model":"dirt"}}}"#,
            PATH,
        )
        .expect("synthetic blockstate");
        assert!(parsed.select("a=1,b=2").is_some());
        assert!(parsed.select("b=2,a=1").is_some());
        assert_eq!(parsed.select("normal").map(<[Variant]>::len), Some(1));
        assert_eq!(parsed.select("").map(<[Variant]>::len), Some(1));
        assert_eq!(parsed.first().map(|(key, _)| key), Some("a=1,b=2"));
    }

    #[test]
    fn bare_named_variants_stay_distinct() {
        let parsed = BlockStateFile::parse(
            br#"{"variants":{"normal":{"model":"a"},"all":{"model":"b"},"map":{"model":"c"}}}"#,
            PATH,
        )
        .expect("synthetic blockstate");
        assert_eq!(parsed.select("normal").expect("normal")[0].model, "a");
        assert_eq!(parsed.select("all").expect("all")[0].model, "b");
        assert_eq!(parsed.select("map").expect("map")[0].model, "c");
        assert_eq!(parsed.variants.len(), 3);
    }

    #[test]
    fn array_variants_keep_declaration_order_and_weights() {
        let parsed = BlockStateFile::parse(
            br#"{"variants":{"normal":[{"model":"a","weight":3},{"model":"b","weight":5}]}}"#,
            PATH,
        )
        .expect("synthetic blockstate");
        let variants = parsed.select("normal").expect("normal variant");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].model, "a");
        assert_eq!(variants[0].weight, 3);
        assert_eq!(variants[1].model, "b");
        assert_eq!(variants[1].weight, 5);
    }

    #[test]
    fn invalid_rotations_and_zero_weight_are_rejected() {
        for json in [
            br#"{"variants":{"normal":{"model":"stone","x":45}}}"#.as_slice(),
            br#"{"variants":{"normal":{"model":"stone","y":1}}}"#.as_slice(),
            br#"{"variants":{"normal":{"model":"stone","weight":0}}}"#.as_slice(),
            br#"{"variants":{"normal":[]}}"#.as_slice(),
            br#"{"variants":{}}"#.as_slice(),
            br#"{"multipart":[]}"#.as_slice(),
        ] {
            assert!(
                BlockStateFile::parse(json, PATH).is_err(),
                "accepted invalid blockstate: {}",
                String::from_utf8_lossy(json)
            );
        }
    }

    #[test]
    fn malformed_json_is_a_json_error() {
        let error = BlockStateFile::parse(b"{not json", PATH).expect_err("malformed JSON");
        assert!(matches!(error, ModelError::Json { .. }));
    }

    #[test]
    fn negative_multiples_of_ninety_are_accepted() {
        let parsed = BlockStateFile::parse(
            br#"{"variants":{"normal":{"model":"stone","x":-90,"y":360}}}"#,
            PATH,
        )
        .expect("synthetic blockstate");
        let variant = &parsed.select("normal").expect("normal variant")[0];
        assert_eq!(variant.x, -90);
        assert_eq!(variant.y, 360);
    }
}
