// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reading `ctm/**/*.properties` into [`CtmRule`]s.
//!
//! The keys and their validation follow OptiFine's `ConnectedProperties`
//! constructor and `isValid` (ADR-0088). The deviations are listed in
//! `docs/tasks/optifine/README.md` (phase 6): block-state properties in
//! `matchBlocks` (`stained_glass:color=red`) are not resolved, and `tiles`
//! is required for every method (OptiFine's defaults for a missing key are
//! unreachable, because it rejects the file first).

use super::{
    BlockMatch, Connect, CtmError, CtmRule, CtmRules, FaceMask, Method, Symmetry, Weights,
};
use crate::{BiomeFilter, Properties, RangeList};

/// The most pattern cells a `repeat` rule may have along one side.
const MAX_REPEAT: i64 = 16;

/// Parses every `.properties` file of `files`, given as `(path relative to
/// the pack-feature folder, bytes)` such as `ctm/glass/glass.properties`.
///
/// `block_id` maps a block name (`minecraft:glass`) to its id. A file that
/// does not parse is logged with a warning and left out; the other rules are
/// unaffected.
pub fn parse_rules<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> CtmRules {
    let mut rules = Vec::new();
    for (path, bytes) in files {
        if !path.ends_with(".properties") {
            continue;
        }
        match parse_rule(path, bytes, block_id) {
            Ok(rule) => rules.push(rule),
            Err(error) => tracing::warn!(path, %error, "ignoring a connected-texture file"),
        }
    }
    CtmRules { rules }
}

/// Parses one rule file. `path` is relative to the pack-feature folder; its
/// folder is the base of the relative tile names.
pub(super) fn parse_rule(
    path: &str,
    bytes: &[u8],
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Result<CtmRule, CtmError> {
    let props = Properties::parse(bytes)?;
    let (base, file) = path.rsplit_once('/').unwrap_or(("", path));
    let name = file
        .rsplit_once('.')
        .map_or(file, |(stem, _)| stem)
        .to_string();

    let mut match_blocks = props
        .get("matchBlocks")
        .map(|text| parse_match_blocks(text, block_id))
        .transpose()?;
    if match_blocks.is_none() {
        match_blocks = block_id_from_name(&name);
    }
    let match_tiles = props.get("matchTiles").map(parse_match_tiles);
    let metadata = list(&props, "metadata")?;
    let tiles = parse_tile_names(props.get("tiles").ok_or(CtmError::NoTiles)?, base)?;
    let connect = match props.get("connect") {
        Some(text) => parse_connect(text)?,
        None if match_blocks.is_some() => Connect::Block,
        None => Connect::Tile,
    };
    let render_pass = int(&props, "renderPass");
    if render_pass > 0 {
        return Err(CtmError::RenderPass(render_pass));
    }
    let weights = props
        .get("weights")
        .and_then(|text| parse_weights(text, tiles.len()));
    let method = parse_method(&props, tiles.len())?;
    let metadata_max = metadata_max(metadata.as_ref(), match_blocks.as_deref());
    Ok(CtmRule {
        name,
        match_blocks,
        metadata,
        match_tiles,
        method,
        tiles,
        connect,
        faces: props.get("faces").map_or(Ok(FaceMask::ALL), parse_faces)?,
        biomes: props.get("biomes").map(BiomeFilter::parse),
        heights: parse_heights(&props)?,
        linked: props
            .get("linked")
            .is_some_and(|text| text.eq_ignore_ascii_case("true")),
        weights,
        symmetry: props
            .get("symmetry")
            .map_or(Symmetry::None, Symmetry::parse),
        metadata_max,
    })
}

impl Symmetry {
    /// Parses a `symmetry` value; an unknown word means no symmetry, as in
    /// OptiFine.
    fn parse(text: &str) -> Self {
        match text.trim() {
            "opposite" => Self::Opposite,
            "all" => Self::All,
            _ => Self::None,
        }
    }
}

/// An integer property, 0 when absent or malformed (`parseInt(str, 0)`).
fn int(props: &Properties, key: &str) -> i64 {
    props
        .get(key)
        .and_then(|text| text.parse().ok())
        .unwrap_or(0)
}

/// A range-list property.
fn list(props: &Properties, key: &'static str) -> Result<Option<RangeList>, CtmError> {
    props
        .get(key)
        .map(|text| {
            RangeList::parse(text).map_err(|_| CtmError::InvalidList {
                key,
                text: text.to_string(),
            })
        })
        .transpose()
}

/// `heights`, or the `minHeight`/`maxHeight` pair.
fn parse_heights(props: &Properties) -> Result<Option<RangeList>, CtmError> {
    if let Some(heights) = list(props, "heights")? {
        return Ok(Some(heights));
    }
    let min = props.get("minHeight").and_then(|t| t.parse::<i64>().ok());
    let max = props.get("maxHeight").and_then(|t| t.parse::<i64>().ok());
    if min.is_none() && max.is_none() {
        return Ok(None);
    }
    let text = format!("{}-{}", min.unwrap_or(0).max(0), max.unwrap_or(1024).max(0));
    list_from(&text, "maxHeight").map(Some)
}

fn list_from(text: &str, key: &'static str) -> Result<RangeList, CtmError> {
    RangeList::parse(text).map_err(|_| CtmError::InvalidList {
        key,
        text: text.to_string(),
    })
}

/// The `method` key with the tile-count checks of `isValid*`.
fn parse_method(props: &Properties, tiles: usize) -> Result<Method, CtmError> {
    let name = props.get("method").unwrap_or("ctm");
    let (method, label, expected, fits): (Method, &'static str, &'static str, bool) = match name {
        "ctm" | "glass" => (Method::Ctm, "ctm", "47 or more", tiles >= 47),
        "horizontal" | "bookshelf" => (Method::Horizontal, "horizontal", "4", tiles == 4),
        "vertical" => (Method::Vertical, "vertical", "4", tiles == 4),
        "horizontal+vertical" | "h+v" => (
            Method::HorizontalVertical,
            "horizontal+vertical",
            "7",
            tiles == 7,
        ),
        "vertical+horizontal" | "v+h" => (
            Method::VerticalHorizontal,
            "vertical+horizontal",
            "7",
            tiles == 7,
        ),
        "top" => (Method::Top, "top", "1", tiles == 1),
        "random" => (Method::Random, "random", "1 or more", tiles >= 1),
        "fixed" => (Method::Fixed, "fixed", "1", tiles == 1),
        "repeat" => return parse_repeat(props, tiles),
        other => return Err(CtmError::UnknownMethod(other.to_string())),
    };
    if fits {
        Ok(method)
    } else {
        Err(CtmError::TileCount {
            method: label,
            expected,
            found: tiles,
        })
    }
}

fn parse_repeat(props: &Properties, tiles: usize) -> Result<Method, CtmError> {
    let (width, height) = (int(props, "width"), int(props, "height"));
    let valid = |size: i64| (1..=MAX_REPEAT).contains(&size);
    if !valid(width) || !valid(height) {
        return Err(CtmError::InvalidRepeatSize { width, height });
    }
    if tiles as i64 != width * height {
        return Err(CtmError::TileCount {
            method: "repeat",
            expected: "width x height",
            found: tiles,
        });
    }
    Ok(Method::Repeat {
        width: width as u8,
        height: height as u8,
    })
}

fn parse_connect(text: &str) -> Result<Connect, CtmError> {
    match text {
        "block" => Ok(Connect::Block),
        "tile" => Ok(Connect::Tile),
        "material" => Ok(Connect::Material),
        other => Err(CtmError::UnknownConnect(other.to_string())),
    }
}

pub(super) fn parse_faces(text: &str) -> Result<FaceMask, CtmError> {
    let mut bits = 0u8;
    for word in text.split([' ', ',']).filter(|word| !word.is_empty()) {
        bits |= match word.to_lowercase().as_str() {
            "bottom" | "down" => 0b00_0001,
            "top" | "up" => 0b00_0010,
            "north" => 0b00_0100,
            "south" => 0b00_1000,
            "west" => 0b01_0000,
            "east" => 0b10_0000,
            "sides" => FaceMask::SIDES,
            "all" => FaceMask::ALL.0,
            _ => return Err(CtmError::UnknownFace(word.to_string())),
        };
    }
    Ok(FaceMask(bits))
}

/// `weights`, trimmed or padded with the average to `tiles` entries
/// (`isValidRandom`), as running sums. `None` when the list is malformed.
fn parse_weights(text: &str, tiles: usize) -> Option<Weights> {
    let mut weights: Vec<u32> = text
        .split([' ', ','])
        .filter(|token| !token.is_empty())
        .map(|token| token.parse().ok())
        .collect::<Option<_>>()?;
    if weights.is_empty() {
        return None;
    }
    let average = weights.iter().sum::<u32>() / weights.len() as u32;
    weights.resize(tiles, average);
    let mut total = 0u32;
    let cumulative = weights
        .iter()
        .map(|weight| {
            total = total.saturating_add(*weight);
            total
        })
        .collect();
    Some(Weights {
        cumulative,
        total: total.max(1),
    })
}

/// The `block<ID>` file-name convention (`detectMatchBlockIds`).
fn block_id_from_name(name: &str) -> Option<Vec<BlockMatch>> {
    let digits = name.strip_prefix("block")?;
    let digits = digits.split(|c: char| !c.is_ascii_digit()).next()?;
    let id = digits.parse().ok()?;
    Some(vec![BlockMatch { id, metadata: None }])
}

/// `matchBlocks`: entries separated by spaces. An entry that names no known
/// block is skipped with a warning, as OptiFine does; a list left empty
/// invalidates the rule.
fn parse_match_blocks(
    text: &str,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Result<Vec<BlockMatch>, CtmError> {
    let mut blocks = Vec::new();
    for entry in text.split(' ').filter(|entry| !entry.is_empty()) {
        match parse_block_entry(entry, block_id) {
            Some(parsed) => blocks.extend(parsed),
            None => tracing::warn!(entry, "ignoring a matchBlocks entry"),
        }
    }
    if blocks.is_empty() {
        return Err(CtmError::BadBlockMatch(text.to_string()));
    }
    Ok(blocks)
}

/// One `matchBlocks` entry: `[domain:]block[:metadata]`, where the block is a
/// name or a list of ids and the metadata a number list.
fn parse_block_entry(
    entry: &str,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Option<Vec<BlockMatch>> {
    let parts: Vec<&str> = entry.split(':').filter(|part| !part.is_empty()).collect();
    let full_name = parts.len() > 1
        && !parts[1].starts_with(|c: char| c.is_ascii_digit())
        && !parts[1].contains('=');
    let (domain, rest) = if full_name {
        (parts[0], &parts[1..])
    } else {
        ("minecraft", &parts[..])
    };
    let block_part = rest.first()?;
    let ids: Vec<u16> = if block_part.starts_with(|c: char| c.is_ascii_digit()) {
        let list = RangeList::parse(block_part).ok()?;
        list.ranges()
            .iter()
            .flat_map(|range| range.min..=range.max)
            .filter_map(|id| u16::try_from(id).ok())
            .collect()
    } else {
        vec![block_id(&format!("{domain}:{block_part}"))?]
    };
    let metadata = match rest.get(1) {
        // Block-state properties (`color=red`) are not resolved.
        Some(meta) if !meta.starts_with(|c: char| c.is_ascii_digit()) => return None,
        Some(meta) => Some(RangeList::parse(meta).ok()?),
        None => None,
    };
    Some(
        ids.into_iter()
            .map(|id| BlockMatch {
                id,
                metadata: metadata.clone(),
            })
            .collect(),
    )
}

/// `matchTiles`: sprite names as atlas keys. A bare name is a block texture.
fn parse_match_tiles(text: &str) -> Vec<String> {
    text.split(' ')
        .filter(|token| !token.is_empty())
        .map(|token| {
            let token = token.strip_suffix(".png").unwrap_or(token);
            if token.contains('/') {
                token.strip_prefix("textures/").unwrap_or(token).to_string()
            } else {
                format!("blocks/{token}")
            }
        })
        .collect()
}

/// `tiles`: numbers, `a-b` ranges and names, as atlas keys.
fn parse_tile_names(text: &str, base: &str) -> Result<Vec<String>, CtmError> {
    let mut names = Vec::new();
    for token in text.split([' ', ',']).filter(|token| !token.is_empty()) {
        match expand_interval(token) {
            Some(Ok(range)) => names.extend(range.map(|n| n.to_string())),
            // A reversed interval is skipped, as OptiFine warns and moves on.
            Some(Err(())) => tracing::warn!(token, "ignoring a reversed tile interval"),
            None => names.push(token.to_string()),
        }
    }
    names.iter().map(|name| tile_key(name, base)).collect()
}

/// `a-b` with two non-negative numbers: the range, or `Err` when reversed.
/// `None` for anything else, which is a plain name.
fn expand_interval(token: &str) -> Option<Result<std::ops::RangeInclusive<u32>, ()>> {
    let mut parts = token.split('-').filter(|part| !part.is_empty());
    let (low, high) = (parts.next()?, parts.next()?);
    if parts.next().is_some() || !token.contains('-') {
        return None;
    }
    let (low, high) = (low.parse::<u32>().ok()?, high.parse::<u32>().ok()?);
    Some(if low > high { Err(()) } else { Ok(low..=high) })
}

/// The atlas key of one tile name (`parseTileNames`, `registerIcons`).
///
/// Names are relative to the folder of the properties file unless they start
/// with `textures/` or an `optifine/`/`mcpatcher/` folder. A pack tile under
/// `ctm/` gets the key `blocks/ctm/...`, which is where the client's atlas
/// build stores it; a block texture keeps its own key.
fn tile_key(name: &str, base: &str) -> Result<String, CtmError> {
    let invalid = || CtmError::InvalidTile(name.to_string());
    if name.contains("..") {
        return Err(invalid());
    }
    let name = name.strip_suffix(".png").unwrap_or(name);
    let path = if let Some(rest) = name.strip_prefix("./") {
        format!("{base}/{rest}")
    } else if let Some(rest) = name.strip_prefix("~/") {
        rest.to_string()
    } else if name.starts_with(base) && !base.is_empty()
        || name.starts_with("textures/")
        || name.starts_with("mcpatcher/")
        || name.starts_with("optifine/")
    {
        name.to_string()
    } else {
        format!("{base}/{name}")
    };
    let path = path.trim_start_matches('/');
    let path = path
        .strip_prefix("mcpatcher/")
        .or_else(|| path.strip_prefix("optifine/"))
        .unwrap_or(path);
    if let Some(block) = path.strip_prefix("textures/blocks/") {
        Ok(format!("blocks/{block}"))
    } else if path.starts_with("textures/") || path.is_empty() {
        Err(invalid())
    } else {
        Ok(format!("blocks/{path}"))
    }
}

/// The largest metadata a rule names (`getMetadataMax`), -1 when none.
fn metadata_max(metadata: Option<&RangeList>, blocks: Option<&[BlockMatch]>) -> i32 {
    let max_of = |list: Option<&RangeList>| {
        list.and_then(|list| list.ranges().iter().map(|range| range.max).max())
            .map_or(-1, |max| max as i32)
    };
    blocks
        .into_iter()
        .flatten()
        .map(|entry| max_of(entry.metadata.as_ref()))
        .chain([max_of(metadata)])
        .max()
        .unwrap_or(-1)
}
