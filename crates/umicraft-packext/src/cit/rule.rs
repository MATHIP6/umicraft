// SPDX-License-Identifier: MIT OR Apache-2.0

//! One `.properties` rule of Custom Items and its matching.

use std::collections::BTreeMap;

use thiserror::Error;

use super::nbt::{NbtCondition, NbtLookup};
use crate::cem::{resolve_path, PackPath};
use crate::{int_list, Properties, PropertiesError};

/// The highest number of a CIT range such as `5-` stands for.
const RANGE_END: u32 = 65_535;

/// Why a rule file was rejected. The rule is skipped; the others stay.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleError {
    /// The file is not a valid `.properties` file.
    #[error(transparent)]
    Properties(#[from] PropertiesError),
    /// `type` is none of `item`, `enchantment` and `armor`.
    #[error("unknown type {0:?}")]
    UnknownType(String),
    /// An item or armour rule names no item, and the file name is no item.
    #[error("no items given")]
    NoItems,
    /// There is neither `texture` nor `texture.<name>`.
    #[error("no texture given")]
    NoTexture,
    /// An enchantment rule without `enchantmentIDs`.
    #[error("no enchantment ids given")]
    NoEnchantments,
    /// A value is invalid: a range, a name condition.
    #[error("invalid {key}: {reason}")]
    Invalid {
        /// The key.
        key: &'static str,
        /// What is wrong.
        reason: String,
    },
}

/// What a rule replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CitKind {
    /// The item's picture in the GUI, the hand and the world.
    Item,
    /// The enchantment glint of the item.
    Enchantment,
    /// The armour texture worn by players and mobs.
    Armor,
}

/// Resolves the names a rule uses; the caller owns the registries.
pub struct Names<'a> {
    /// The id of an item name (`diamond_sword`, `minecraft:diamond_sword`).
    pub item: &'a dyn Fn(&str) -> Option<u16>,
    /// The id of an enchantment name (`flame`, `minecraft:fire_aspect`).
    pub enchantment: &'a dyn Fn(&str) -> Option<u16>,
}

/// Inclusive ranges where `n-` runs to 65535 and `-n` starts at 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ranges(Vec<(u32, u32)>);

impl Ranges {
    /// Parses `1 3-5 7- -2`. `None` when a token is invalid, as one bad token
    /// invalidates the list in OptiFine.
    fn parse(text: &str) -> Option<Self> {
        let ranges = text
            .split(' ')
            .filter(|token| !token.is_empty())
            .map(range)
            .collect::<Option<Vec<_>>>()?;
        Some(Self(ranges))
    }

    fn contains(&self, value: i64) -> bool {
        self.0
            .iter()
            .any(|&(low, high)| (i64::from(low)..=i64::from(high)).contains(&value))
    }
}

fn range(token: &str) -> Option<(u32, u32)> {
    if token.matches('-').count() > 1 {
        return None;
    }
    let number = |text: &str| text.parse::<i32>().ok().and_then(|n| u32::try_from(n).ok());
    match token.split_once('-') {
        None => number(token).map(|n| (n, n)),
        Some(("", high)) => number(high).map(|high| (0, high)),
        Some((low, "")) => number(low).map(|low| (low, RANGE_END)),
        Some((a, b)) => {
            let (a, b) = (number(a)?, number(b)?);
            Some((a.min(b), a.max(b)))
        }
    }
}

/// The damage condition: values, optionally percent of the maximum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damage {
    ranges: Ranges,
    percent: bool,
}

/// What a rule is asked about: one item stack.
pub struct ItemQuery<'a> {
    /// The item id.
    pub item_id: u16,
    /// The damage value (metadata for items without durability).
    pub damage: i32,
    /// The item's maximum damage, `0` without durability.
    pub max_damage: i32,
    /// The stack size.
    pub stack_size: i32,
    /// The enchantments as `(id, level)`.
    pub enchantments: &'a [(i32, i32)],
    /// The tags of the stack, see [`NbtLookup`].
    pub nbt: NbtLookup<'a>,
}

/// One rule of a `.properties` file.
#[derive(Debug, Clone)]
pub struct CitRule {
    /// The file name without extension.
    pub name: String,
    /// The folder of the file, relative to the pack-feature folder.
    pub base: String,
    /// What the rule replaces.
    pub kind: CitKind,
    /// The item ids it applies to; empty for an enchantment rule.
    pub items: Vec<u16>,
    /// The replacement texture, `None` when only `textures` is given.
    pub texture: Option<PackPath>,
    /// The `texture.<key>` replacements, by key (`bow_pulling_0`).
    pub textures: BTreeMap<String, PackPath>,
    /// The order key: higher weights come first.
    pub weight: i32,
    damage: Option<Damage>,
    damage_mask: u32,
    stack_size: Option<Ranges>,
    enchantment_ids: Option<Ranges>,
    enchantment_levels: Option<Ranges>,
    nbt: Vec<NbtCondition>,
}

impl CitRule {
    /// Parses the file at `path` (relative to the pack-feature folder, for
    /// example `cit/swords/excalibur.properties`).
    ///
    /// # Errors
    ///
    /// [`RuleError`] when the file is unusable.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_packext::cit::{CitKind, CitRule, Names};
    ///
    /// let names = Names {
    ///     item: &|name| (name == "diamond_sword").then_some(276),
    ///     enchantment: &|_| None,
    /// };
    /// let rule = CitRule::parse(
    ///     b"items=diamond_sword\ntexture=excalibur\ndamage=0-50%\n",
    ///     "cit/swords/excalibur.properties",
    ///     &names,
    /// ).expect("valid");
    /// assert_eq!(rule.kind, CitKind::Item);
    /// assert_eq!(rule.items, [276]);
    /// ```
    pub fn parse(bytes: &[u8], path: &str, names: &Names<'_>) -> Result<Self, RuleError> {
        let props = Properties::parse(bytes)?;
        let (base, file) = path.rsplit_once('/').unwrap_or(("", path));
        let name = file
            .rsplit_once('.')
            .map_or(file, |(stem, _)| stem)
            .to_string();
        let kind = parse_kind(props.get("type"))?;
        let textures = texture_map(&props, base);
        let texture = texture_of(&props, &name, base, kind, &textures);
        if texture.is_none() && textures.is_empty() {
            return Err(RuleError::NoTexture);
        }
        let enchantment_ids = enchantment_ids(&props, names)?;
        let items = items_of(&props, &name, names);
        if kind != CitKind::Enchantment && items.is_empty() {
            return Err(RuleError::NoItems);
        }
        if kind == CitKind::Enchantment && enchantment_ids.is_none() {
            return Err(RuleError::NoEnchantments);
        }
        Ok(Self {
            damage: damage_of(&props)?,
            damage_mask: number(&props, "damageMask").unwrap_or(0),
            stack_size: ranges(&props, "stackSize")?,
            enchantment_levels: ranges(&props, "enchantmentLevels")?,
            nbt: nbt_conditions(&props)?,
            weight: props
                .get("weight")
                .and_then(|w| w.parse().ok())
                .unwrap_or(0),
            name,
            base: base.to_string(),
            kind,
            items,
            texture,
            textures,
            enchantment_ids,
        })
    }

    /// Whether the stack satisfies every condition (`CustomItems.matchesProperties`).
    ///
    /// Like OptiFine, the enchantment id and level conditions are checked
    /// independently: some enchantment has an id in range and some enchantment
    /// has a level in range.
    #[must_use]
    pub fn matches(&self, query: &ItemQuery<'_>) -> bool {
        self.matches_damage(query)
            && self
                .stack_size
                .as_ref()
                .is_none_or(|sizes| sizes.contains(i64::from(query.stack_size)))
            && any_enchantment(&self.enchantment_ids, query.enchantments, |(id, _)| id)
            && any_enchantment(
                &self.enchantment_levels,
                query.enchantments,
                |(_, level)| level,
            )
            && self
                .nbt
                .iter()
                .all(|condition| condition.matches(query.nbt))
    }

    fn matches_damage(&self, query: &ItemQuery<'_>) -> bool {
        let Some(damage) = &self.damage else {
            return true;
        };
        let mut value = i64::from(query.damage);
        if self.damage_mask != 0 {
            value &= i64::from(self.damage_mask);
        }
        if damage.percent {
            value = match i64::from(query.max_damage) {
                0 => 0,
                max => value * 100 / max,
            };
        }
        damage.ranges.contains(value)
    }
}

/// `true` without a condition; with one, some enchantment's `pick` is in it.
fn any_enchantment(
    condition: &Option<Ranges>,
    enchantments: &[(i32, i32)],
    pick: impl Fn((i32, i32)) -> i32,
) -> bool {
    condition.as_ref().is_none_or(|ranges| {
        enchantments
            .iter()
            .any(|&pair| ranges.contains(i64::from(pick(pair))))
    })
}

fn parse_kind(text: Option<&str>) -> Result<CitKind, RuleError> {
    match text {
        None | Some("item") => Ok(CitKind::Item),
        Some("enchantment") => Ok(CitKind::Enchantment),
        Some("armor") => Ok(CitKind::Armor),
        Some(other) => Err(RuleError::UnknownType(other.to_string())),
    }
}

/// `items` (or `matchItems`): ids, id ranges and names; the file name when
/// absent (`diamond_sword.properties`). Unknown names are dropped with a
/// warning.
fn items_of(props: &Properties, name: &str, names: &Names<'_>) -> Vec<u16> {
    let Some(list) = props.get("items").or_else(|| props.get("matchItems")) else {
        return (names.item)(name).into_iter().collect();
    };
    let mut items = Vec::new();
    for token in list.split(' ').filter(|token| !token.is_empty()) {
        let ids = int_list::parse(token);
        if !ids.is_empty() && token.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
            items.extend(ids.into_iter().filter_map(|id| u16::try_from(id).ok()));
        } else if let Some(id) = (names.item)(token) {
            items.push(id);
        } else {
            tracing::warn!(item = token, "item not found, ignored");
        }
    }
    items.sort_unstable();
    items.dedup();
    items
}

/// The enchantment ids of `enchantmentIDs` (ids, ranges and names) as ranges.
fn enchantment_ids(props: &Properties, names: &Names<'_>) -> Result<Option<Ranges>, RuleError> {
    let Some(list) = props
        .get("enchantmentIDs")
        .or_else(|| props.get("enchantments"))
    else {
        return Ok(None);
    };
    let mut parsed = Vec::new();
    for token in list.split(' ').filter(|token| !token.is_empty()) {
        match range(token)
            .or_else(|| (names.enchantment)(token).map(|id| (u32::from(id), u32::from(id))))
        {
            Some(pair) => parsed.push(pair),
            None => tracing::warn!(enchantment = token, "enchantment not found, ignored"),
        }
    }
    Ok(Some(Ranges(parsed)))
}

/// `damage`, with a trailing `%` for a percentage.
fn damage_of(props: &Properties) -> Result<Option<Damage>, RuleError> {
    let Some(text) = props.get("damage") else {
        return Ok(None);
    };
    let percent = text.contains('%');
    let ranges = Ranges::parse(&text.replace('%', "")).ok_or_else(|| RuleError::Invalid {
        key: "damage",
        reason: text.to_string(),
    })?;
    Ok(Some(Damage { ranges, percent }))
}

fn ranges(props: &Properties, key: &'static str) -> Result<Option<Ranges>, RuleError> {
    props
        .get(key)
        .map(|text| {
            Ranges::parse(text).ok_or_else(|| RuleError::Invalid {
                key,
                reason: text.to_string(),
            })
        })
        .transpose()
}

fn number(props: &Properties, key: &str) -> Option<u32> {
    props.get(key).and_then(|text| text.parse().ok())
}

/// Every `nbt.<path>` key as a condition.
fn nbt_conditions(props: &Properties) -> Result<Vec<NbtCondition>, RuleError> {
    props
        .keys()
        .filter_map(|key| Some((key, key.strip_prefix("nbt.")?)))
        .map(|(key, path)| {
            let value = props.get(key).unwrap_or_default();
            NbtCondition::parse(path, value).map_err(|error| RuleError::Invalid {
                key: "nbt",
                reason: error.to_string(),
            })
        })
        .collect()
}

/// `texture.<key>=<path>` entries.
fn texture_map(props: &Properties, base: &str) -> BTreeMap<String, PackPath> {
    props
        .keys()
        .filter_map(|key| Some((key.strip_prefix("texture.")?, props.get(key)?)))
        .map(|(key, path)| (key.to_string(), resolve_path(base, path, ".png")))
        .collect()
}

/// `texture` (or `tile`, `source`); an item rule defaults to the file name,
/// or to `texture.bow_standby`.
fn texture_of(
    props: &Properties,
    name: &str,
    base: &str,
    kind: CitKind,
    textures: &BTreeMap<String, PackPath>,
) -> Option<PackPath> {
    let given = ["texture", "tile", "source"]
        .iter()
        .find_map(|key| props.get(key));
    match given {
        Some(path) => Some(resolve_path(base, path, ".png")),
        None if kind == CitKind::Armor => None,
        None => textures
            .get("bow_standby")
            .cloned()
            .or_else(|| Some(resolve_path(base, name, ".png"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names<'static> {
        Names {
            item: &|name| match name.trim_start_matches("minecraft:") {
                "diamond_sword" => Some(276),
                "stick" => Some(280),
                _ => None,
            },
            enchantment: &|name| match name.trim_start_matches("minecraft:") {
                "sharpness" => Some(16),
                "flame" => Some(50),
                _ => None,
            },
        }
    }

    fn parse(text: &str) -> Result<CitRule, RuleError> {
        CitRule::parse(text.as_bytes(), "cit/swords/excalibur.properties", &names())
    }

    fn query(damage: i32, enchantments: &[(i32, i32)]) -> ItemQuery<'_> {
        ItemQuery {
            item_id: 276,
            damage,
            max_damage: 1561,
            stack_size: 1,
            enchantments,
            nbt: &|_| Vec::new(),
        }
    }

    #[test]
    fn a_rule_reads_its_items_texture_and_order() {
        let rule =
            parse("items=diamond_sword 280 290-291\ntexture=blade\nweight=5\n").expect("valid");
        assert_eq!(rule.items, [276, 280, 290, 291]);
        assert_eq!(
            rule.texture,
            Some(PackPath::Feature("cit/swords/blade.png".to_string()))
        );
        assert_eq!(
            (rule.name.as_str(), rule.base.as_str(), rule.weight),
            ("excalibur", "cit/swords", 5)
        );
    }

    #[test]
    fn the_file_name_is_the_default_texture_and_item() {
        let rule = CitRule::parse(b"", "cit/diamond_sword.properties", &names()).expect("valid");
        assert_eq!(rule.items, [276]);
        assert_eq!(
            rule.texture,
            Some(PackPath::Feature("cit/diamond_sword.png".to_string()))
        );
    }

    #[test]
    fn texture_paths_follow_the_pack_path_rules() {
        let rule =
            parse("items=280\ntexture=textures/items/custom\ntexture.bow_pulling_0=./pull\n")
                .expect("valid");
        assert_eq!(
            rule.texture,
            Some(PackPath::Resource("textures/items/custom.png".to_string()))
        );
        assert_eq!(
            rule.textures.get("bow_pulling_0"),
            Some(&PackPath::Feature("cit/swords/pull.png".to_string()))
        );
    }

    #[test]
    fn invalid_rules_are_rejected_with_a_reason() {
        assert_eq!(
            parse("type=wings\n").err(),
            Some(RuleError::UnknownType("wings".to_string()))
        );
        assert_eq!(
            CitRule::parse(b"items=nothing\n", "cit/x.properties", &names()).err(),
            Some(RuleError::NoItems)
        );
        assert_eq!(
            CitRule::parse(
                b"type=enchantment\ntexture=glint\n",
                "cit/x.properties",
                &names()
            )
            .err(),
            Some(RuleError::NoEnchantments)
        );
        assert_eq!(
            CitRule::parse(b"type=armor\nitems=280\n", "cit/x.properties", &names()).err(),
            Some(RuleError::NoTexture)
        );
        assert!(matches!(
            parse("items=280\ndamage=a-b\n"),
            Err(RuleError::Invalid { key: "damage", .. })
        ));
        assert!(matches!(
            parse("items=280\nnbt.display.Name=regex:(\n"),
            Err(RuleError::Invalid { key: "nbt", .. })
        ));
    }

    #[test]
    fn open_ended_ranges_and_percent() {
        assert_eq!(
            Ranges::parse("5- -2 7-9"),
            Some(Ranges(vec![(5, 65535), (0, 2), (7, 9)]))
        );
        assert_eq!(Ranges::parse("1-2-3"), None);
        let rule = parse("items=276\ndamage=0-50%\n").expect("valid");
        assert!(rule.matches(&query(700, &[])), "700 of 1561 is 44 percent");
        assert!(!rule.matches(&query(800, &[])));
    }

    #[test]
    fn a_damage_mask_is_applied_before_the_range() {
        let rule = parse("items=373\ndamage=3\ndamageMask=15\n").expect("valid");
        assert!(
            rule.matches(&query(16387, &[])),
            "fire resistance, any flags"
        );
        assert!(!rule.matches(&query(16388, &[])));
    }

    #[test]
    fn stack_size_and_enchantments() {
        let rule =
            parse("items=276\nstackSize=1-4\nenchantments=sharpness flame\nenchantmentLevels=3-\n")
                .expect("valid");
        assert!(rule.matches(&query(0, &[(16, 5)])));
        assert!(rule.matches(&query(0, &[(50, 3)])));
        assert!(!rule.matches(&query(0, &[(16, 1)])), "level too low");
        assert!(!rule.matches(&query(0, &[(34, 5)])), "other enchantment");
        assert!(!rule.matches(&query(0, &[])), "not enchanted");
        let mut crowd = query(0, &[(16, 5)]);
        crowd.stack_size = 5;
        assert!(!rule.matches(&crowd));
    }

    #[test]
    fn nbt_conditions_must_all_hold() {
        let rule = parse("items=276\nnbt.display.Name=Excalibur\nnbt.display.Lore.*=!cursed\n")
            .expect("valid");
        let tags = |path: &[&str]| match path.join(".").as_str() {
            "display.Name" => vec!["Excalibur".to_string()],
            "display.Lore.*" => vec!["holy".to_string()],
            _ => Vec::new(),
        };
        let mut stack = query(0, &[]);
        stack.nbt = &tags;
        assert!(rule.matches(&stack));
        assert!(!rule.matches(&query(0, &[])), "no tags");
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_files_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..300)) {
            if let Ok(rule) = CitRule::parse(&bytes, "cit/a.properties", &names()) {
                let _ = rule.matches(&query(5, &[(1, 1)]));
            }
        }

        #[test]
        fn arbitrary_values_never_panic(damage in "[0-9 %-]{0,12}", stack in "[0-9 -]{0,12}", nbt in ".{0,12}") {
            let text = format!("items=276\ndamage={damage}\nstackSize={stack}\nnbt.a.b={nbt}\n");
            if let Ok(rule) = parse(&text) {
                let _ = rule.matches(&query(5, &[]));
            }
        }
    }
}
