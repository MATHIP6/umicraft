// SPDX-License-Identifier: MIT OR Apache-2.0

//! All the rules of the packs, ordered, with the lookup of a stack.

use std::collections::HashMap;

use super::rule::{CitKind, CitRule, ItemQuery, Names};
use crate::Properties;

/// The rules of every pack and the global switches.
///
/// # Example
///
/// ```
/// use umicraft_packext::cit::{CitSet, ItemQuery, Names, CitKind};
///
/// let names = Names { item: &|name| (name == "stick").then_some(280), enchantment: &|_| None };
/// let set = CitSet::parse(
///     None,
///     [("cit/stick.properties", &b"weight=1\n"[..])],
///     &names,
/// );
/// let query = ItemQuery { item_id: 280, damage: 0, max_damage: 0, stack_size: 1, enchantments: &[], nbt: &|_| Vec::new() };
/// assert_eq!(set.find(CitKind::Item, &query).map(|rule| rule.name.as_str()), Some("stick"));
/// ```
#[derive(Debug, Clone)]
pub struct CitSet {
    /// Sorted by weight (high first), folder, name.
    rules: Vec<CitRule>,
    /// Indices into `rules` of the item and armour rules, by item id.
    by_item: HashMap<u16, Vec<usize>>,
    use_glint: bool,
}

impl Default for CitSet {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            by_item: HashMap::new(),
            use_glint: true,
        }
    }
}

impl CitSet {
    /// Reads `cit.properties` (`global`) and the rule files. `files` are
    /// `(path relative to the pack-feature folder, bytes)`; only
    /// `cit/**.properties` count. A broken file is skipped with a warning.
    pub fn parse<'a>(
        global: Option<&[u8]>,
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        names: &Names<'_>,
    ) -> Self {
        let use_glint = global
            .and_then(|bytes| Properties::parse(bytes).ok())
            .and_then(|props| props.get("useGlint").map(|v| v != "false"))
            .unwrap_or(true);
        let mut rules: Vec<CitRule> = files
            .into_iter()
            .filter(|(path, _)| path.starts_with("cit/") && path.ends_with(".properties"))
            .filter_map(|(path, bytes)| {
                CitRule::parse(bytes, path, names)
                    .map_err(|error| tracing::warn!(%path, %error, "custom item rule ignored"))
                    .ok()
            })
            .collect();
        rules.sort_by(|a, b| {
            b.weight
                .cmp(&a.weight)
                .then_with(|| a.base.cmp(&b.base))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut by_item: HashMap<u16, Vec<usize>> = HashMap::new();
        for (index, rule) in rules.iter().enumerate() {
            for item in &rule.items {
                by_item.entry(*item).or_default().push(index);
            }
        }
        Self {
            rules,
            by_item,
            use_glint,
        }
    }

    /// The rules in match order.
    #[must_use]
    pub fn rules(&self) -> &[CitRule] {
        &self.rules
    }

    /// Whether the default glint stays when no enchantment rule matches
    /// (`useGlint` of `cit.properties`, default true).
    #[must_use]
    pub fn use_glint(&self) -> bool {
        self.use_glint
    }

    /// Whether some item or armour rule names `item_id`.
    #[must_use]
    pub fn has_item(&self, item_id: u16) -> bool {
        self.by_item.contains_key(&item_id)
    }

    /// Whether there is no rule at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The index in [`Self::rules`] of the first rule of `kind` that matches
    /// the stack.
    #[must_use]
    pub fn find_index(&self, kind: CitKind, query: &ItemQuery<'_>) -> Option<usize> {
        self.by_item
            .get(&query.item_id)?
            .iter()
            .copied()
            .find(|&index| self.rules[index].kind == kind && self.rules[index].matches(query))
    }

    /// The first rule of `kind` that matches the stack.
    #[must_use]
    pub fn find(&self, kind: CitKind, query: &ItemQuery<'_>) -> Option<&CitRule> {
        self.find_index(kind, query).map(|index| &self.rules[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names<'static> {
        Names {
            item: &|name| match name {
                "stick" => Some(280),
                "bow" => Some(261),
                _ => None,
            },
            enchantment: &|_| None,
        }
    }

    fn query(item_id: u16, damage: i32) -> ItemQuery<'static> {
        ItemQuery {
            item_id,
            damage,
            max_damage: 0,
            stack_size: 1,
            enchantments: &[],
            nbt: &|_| Vec::new(),
        }
    }

    fn set(files: &[(&str, &str)]) -> CitSet {
        CitSet::parse(
            None,
            files.iter().map(|(path, text)| (*path, text.as_bytes())),
            &names(),
        )
    }

    #[test]
    fn higher_weight_wins_then_folder_then_name() {
        let set = set(&[
            ("cit/b/two.properties", "items=stick\n"),
            ("cit/a/zed.properties", "items=stick\n"),
            ("cit/a/one.properties", "items=stick\n"),
            ("cit/c/heavy.properties", "items=stick\nweight=3\n"),
        ]);
        let order: Vec<_> = set.rules().iter().map(|rule| rule.name.as_str()).collect();
        assert_eq!(order, ["heavy", "one", "zed", "two"]);
        assert_eq!(
            set.find(CitKind::Item, &query(280, 0))
                .map(|r| r.name.as_str()),
            Some("heavy")
        );
    }

    #[test]
    fn the_first_matching_rule_wins_and_the_kind_is_respected() {
        let set = set(&[
            (
                "cit/damaged.properties",
                "items=stick\ndamage=5-\nweight=2\n",
            ),
            ("cit/plain.properties", "items=stick\n"),
            (
                "cit/worn.properties",
                "type=armor\nitems=stick\ntexture.x=y\nweight=9\n",
            ),
        ]);
        let name = |q: ItemQuery<'_>, kind| set.find(kind, &q).map(|r| r.name.clone());
        assert_eq!(
            name(query(280, 7), CitKind::Item).as_deref(),
            Some("damaged")
        );
        assert_eq!(name(query(280, 1), CitKind::Item).as_deref(), Some("plain"));
        assert_eq!(name(query(280, 1), CitKind::Armor).as_deref(), Some("worn"));
        assert_eq!(name(query(261, 1), CitKind::Item), None);
    }

    #[test]
    fn broken_and_foreign_files_are_skipped() {
        let set = set(&[
            ("cit/bad.properties", "type=wings\n"),
            ("cit/good.properties", "items=bow\n"),
            ("ctm/other.properties", "items=bow\n"),
            ("cit/readme.txt", "items=bow\n"),
        ]);
        assert_eq!(set.rules().len(), 1);
    }

    #[test]
    fn use_glint_defaults_to_true() {
        assert!(CitSet::default().use_glint());
        let off = CitSet::parse(Some(b"useGlint=false\n"), [], &names());
        assert!(!off.use_glint());
        assert!(CitSet::parse(Some(b"useGlint=true\n"), [], &names()).use_glint());
        assert!(CitSet::parse(Some(b"a=\\u12"), [], &names()).use_glint());
    }
}
