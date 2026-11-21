// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rules of a `.properties` file of Random Entities.
//!
//! Rule `N` (from 1) is made of `skins.N` (or `textures.N`), and the optional
//! `weights.N`, `biomes.N`, `heights.N` (or `minHeight.N` and `maxHeight.N`),
//! `name.N` and `professions.N`. The first rule whose conditions hold picks a
//! texture among its list, by weight when there are weights.
//!
//! One deviation from OptiFine, documented in `docs/sources.md`: its
//! `RandomMobsRule.matches` returns right after the height test, so a rule
//! with `heights` ignores its `name` and `professions`. Here every condition
//! must hold.

use thiserror::Error;

use crate::int_list;
use crate::{BiomeFilter, Properties, PropertiesError, RangeList, TextMatch};

/// The highest rule number tried; OptiFine tries as many as the file has
/// entries, and a file cannot meaningfully hold more than this.
const MAX_RULES: usize = 1024;

/// Why a rules file was rejected as a whole. A broken rule inside it is
/// dropped with a warning instead.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RandomRulesError {
    /// The file is not a valid `.properties` file.
    #[error(transparent)]
    Properties(#[from] PropertiesError),
    /// No rule survived: the file chooses nothing.
    #[error("no valid rule in the file")]
    NoRule,
}

/// A villager profession with optional careers (`farmer`, `smith:armor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profession {
    id: i32,
    /// Empty: every career of the profession.
    careers: Vec<i32>,
}

impl Profession {
    /// Parses one `profession[:career,career]` entry; `None` when a name is
    /// unknown (OptiFine `ConnectedParser.parseProfession`).
    fn parse(entry: &str) -> Option<Self> {
        let entry = entry.to_lowercase();
        let mut parts = entry.split(':');
        let id = profession_id(parts.next()?)?;
        let careers = match parts.next() {
            Some(list) => list
                .split(',')
                .map(|career| career_id(id, career))
                .collect::<Option<Vec<_>>>()?,
            None => Vec::new(),
        };
        parts.next().is_none().then_some(Self { id, careers })
    }

    fn matches(&self, profession: i32, career: Option<i32>) -> bool {
        self.id == profession
            && (self.careers.is_empty() || career.is_some_and(|c| self.careers.contains(&c)))
    }
}

fn profession_id(name: &str) -> Option<i32> {
    name.parse().ok().filter(|id| *id >= 0).or(match name {
        "farmer" => Some(0),
        "librarian" => Some(1),
        "priest" => Some(2),
        "blacksmith" => Some(3),
        "butcher" => Some(4),
        "nitwit" => Some(5),
        _ => None,
    })
}

fn career_id(profession: i32, name: &str) -> Option<i32> {
    if let Some(id) = name.parse::<i32>().ok().filter(|id| *id >= 0) {
        return Some(id);
    }
    match (profession, name) {
        (0, "farmer")
        | (1, "librarian")
        | (2, "cleric")
        | (3, "armor")
        | (4, "butcher")
        | (5, "nitwit") => Some(1),
        (0, "fisherman") | (1, "cartographer") | (3, "weapon") | (4, "leather") => Some(2),
        (0, "shepherd") | (3, "tool") => Some(3),
        (0, "fletcher") => Some(4),
        _ => None,
    }
}

/// What the rules know about an entity, recorded when it is first seen.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spawn<'a> {
    /// The display name of the biome it spawned in ("Extreme Hills").
    pub biome: Option<&'a str>,
    /// The block height it spawned at.
    pub height: Option<i32>,
    /// Its custom name, without formatting resolved.
    pub name: Option<&'a str>,
    /// The villager profession, `None` for an entity that is not a villager.
    pub profession: Option<i32>,
    /// The villager career, `None` when unknown (the 1.8 client never learns
    /// it, so a rule that names a career does not match).
    pub career: Option<i32>,
}

/// One rule.
#[derive(Debug, Clone)]
struct Rule {
    /// The texture numbers it chooses among.
    skins: Vec<u32>,
    /// Running totals of the weights, as long as `skins`.
    cumulative: Option<Vec<u64>>,
    biomes: Option<BiomeFilter>,
    heights: Option<RangeList>,
    name: Option<TextMatch>,
    professions: Option<Vec<Profession>>,
}

impl Rule {
    fn matches(&self, spawn: &Spawn<'_>) -> bool {
        self.biomes
            .as_ref()
            .is_none_or(|filter| spawn.biome.is_some_and(|biome| filter.matches(biome)))
            && self.heights.as_ref().is_none_or(|heights| {
                spawn
                    .height
                    .is_some_and(|height| heights.contains(i64::from(height)))
            })
            && self.name.as_ref().is_none_or(|name| {
                spawn
                    .name
                    .map_or_else(|| name.matches_absent(), |text| name.matches(text))
            })
            && self.professions.as_ref().is_none_or(|list| {
                spawn.profession.is_none_or(|profession| {
                    list.iter().any(|p| p.matches(profession, spawn.career))
                })
            })
    }

    fn pick(&self, random_id: u32) -> u32 {
        let index = match &self.cumulative {
            None => random_id as usize % self.skins.len(),
            Some(totals) => {
                let sum = totals.last().copied().filter(|sum| *sum > 0).unwrap_or(1);
                let roll = u64::from(random_id) % sum;
                totals.iter().position(|total| *total > roll).unwrap_or(0)
            }
        };
        self.skins[index]
    }
}

/// The rules of one file, in order.
#[derive(Debug, Clone)]
pub struct RandomRules {
    rules: Vec<Rule>,
}

impl RandomRules {
    /// Parses a rules file. `exists(n)` tells whether texture number `n`
    /// exists; a rule that lists a missing texture is dropped with a warning,
    /// as OptiFine invalidates it. Number 1 is the vanilla texture and always
    /// exists.
    ///
    /// # Errors
    ///
    /// [`RandomRulesError`] when the file is not `.properties` text or no rule
    /// is usable.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_packext::random_entities::{RandomRules, Spawn};
    ///
    /// let rules = RandomRules::parse(b"skins.1=2 3\nheights.1=0-40\n", |_| true).expect("valid");
    /// let deep = Spawn { height: Some(12), ..Spawn::default() };
    /// assert_eq!(rules.pick(&deep, 0), Some(2));
    /// assert_eq!(rules.pick(&Spawn::default(), 0), None);
    /// ```
    pub fn parse(bytes: &[u8], exists: impl Fn(u32) -> bool) -> Result<Self, RandomRulesError> {
        let properties = Properties::parse(bytes)?;
        let rules: Vec<Rule> = (1..=MAX_RULES.min(properties.len().max(1)))
            .filter_map(|index| parse_rule(&properties, index, &exists))
            .collect();
        if rules.is_empty() {
            return Err(RandomRulesError::NoRule);
        }
        Ok(Self { rules })
    }

    /// The texture number chosen by the first rule that matches, `None` when
    /// no rule does.
    #[must_use]
    pub fn pick(&self, spawn: &Spawn<'_>, random_id: u32) -> Option<u32> {
        self.rules
            .iter()
            .find(|rule| rule.matches(spawn))
            .map(|rule| rule.pick(random_id))
    }
}

/// Rule `index`, or `None` when it is absent or invalid.
fn parse_rule(
    properties: &Properties,
    index: usize,
    exists: &impl Fn(u32) -> bool,
) -> Option<Rule> {
    let get = |key: &str| properties.get(&format!("{key}.{index}"));
    let skins = int_list::parse(get("skins").or_else(|| get("textures"))?);
    if skins.is_empty() {
        tracing::warn!(index, "random entity rule without skins, ignored");
        return None;
    }
    if let Some(missing) = skins.iter().find(|skin| **skin > 1 && !exists(**skin)) {
        tracing::warn!(
            index,
            skin = missing,
            "random entity texture not found, rule ignored"
        );
        return None;
    }
    let professions = match get("professions").map(parse_professions) {
        Some(None) => {
            tracing::warn!(index, "invalid professions, rule ignored");
            return None;
        }
        Some(list) => list,
        None => None,
    };
    Some(Rule {
        cumulative: get("weights").map(|text| cumulative(&int_list::parse(text), skins.len())),
        skins,
        biomes: get("biomes").map(BiomeFilter::parse),
        heights: heights(get("heights"), get("minHeight"), get("maxHeight")),
        name: get("name").and_then(parse_name),
        professions,
    })
}

/// The running totals of `weights`, cut or padded with their average to
/// `len` entries (OptiFine `RandomMobsRule.isValid`).
fn cumulative(weights: &[u32], len: usize) -> Vec<u64> {
    let average = match weights.len() {
        0 => 0,
        n => weights.iter().map(|w| u64::from(*w)).sum::<u64>() / n as u64,
    };
    let mut total = 0;
    (0..len)
        .map(|i| {
            total += weights.get(i).map_or(average, |w| u64::from(*w));
            total
        })
        .collect()
}

/// `heights.N`, else the legacy `minHeight.N` and `maxHeight.N` (default 0
/// and 256). A malformed value drops the condition with a warning.
fn heights(list: Option<&str>, min: Option<&str>, max: Option<&str>) -> Option<RangeList> {
    if let Some(text) = list {
        return RangeList::parse(text)
            .map_err(|error| tracing::warn!(%error, "invalid heights, ignored"))
            .ok();
    }
    if min.is_none() && max.is_none() {
        return None;
    }
    let bound = |text: Option<&str>, default: u32| match text {
        None => Some(default),
        Some(text) => text.parse::<u32>().ok(),
    };
    let (Some(low), Some(high)) = (bound(min, 0), bound(max, 256)) else {
        tracing::warn!("invalid minHeight or maxHeight, ignored");
        return None;
    };
    RangeList::parse(&format!("{low}-{high}")).ok()
}

fn parse_name(text: &str) -> Option<TextMatch> {
    TextMatch::parse(text)
        .map_err(|error| tracing::warn!(%error, "invalid name condition, ignored"))
        .ok()
}

/// `None` when an entry is unknown. An empty list is no condition.
fn parse_professions(text: &str) -> Option<Vec<Profession>> {
    let list = text
        .split(' ')
        .filter(|entry| !entry.is_empty())
        .map(Profession::parse)
        .collect::<Option<Vec<_>>>()?;
    (!list.is_empty()).then_some(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> RandomRules {
        RandomRules::parse(text.as_bytes(), |n| n <= 6).expect("valid")
    }

    fn at(height: i32) -> Spawn<'static> {
        Spawn {
            height: Some(height),
            ..Spawn::default()
        }
    }

    #[test]
    fn the_first_matching_rule_wins() {
        let rules = rules("skins.1=2\nheights.1=0-30\nskins.2=3\nheights.2=0-100\nskins.3=4\n");
        assert_eq!(rules.pick(&at(10), 0), Some(2));
        assert_eq!(rules.pick(&at(60), 0), Some(3));
        assert_eq!(rules.pick(&Spawn::default(), 0), Some(4));
    }

    #[test]
    fn without_weights_the_id_picks_modulo() {
        let rules = rules("textures.1=2-4\n");
        let picks: Vec<_> = (0..4).map(|id| rules.pick(&at(0), id)).collect();
        assert_eq!(picks, [Some(2), Some(3), Some(4), Some(2)]);
    }

    #[test]
    fn weights_pick_by_running_total() {
        let rules = rules("skins.1=2 3\nweights.1=1 3\n");
        let picks: Vec<_> = (0..8)
            .map(|id| rules.pick(&at(0), id).expect("rule"))
            .collect();
        assert_eq!(picks, [2, 3, 3, 3, 2, 3, 3, 3]);
    }

    #[test]
    fn short_weights_are_padded_with_their_average_and_long_ones_cut() {
        assert_eq!(cumulative(&[2, 4], 4), [2, 6, 9, 12]);
        assert_eq!(cumulative(&[5, 1, 1], 2), [5, 6]);
        assert_eq!(cumulative(&[], 2), [0, 0]);
    }

    #[test]
    fn all_zero_weights_pick_the_first_skin() {
        let rules = rules("skins.1=2 3\nweights.1=0 0\n");
        assert_eq!(rules.pick(&at(0), 5), Some(2));
    }

    #[test]
    fn biomes_need_a_known_biome() {
        let rules = rules("skins.1=2\nbiomes.1=Plains Desert\n");
        let plains = Spawn {
            biome: Some("Plains"),
            ..Spawn::default()
        };
        assert_eq!(rules.pick(&plains, 0), Some(2));
        assert_eq!(rules.pick(&Spawn::default(), 0), None);
    }

    #[test]
    fn min_and_max_height_are_the_legacy_spelling() {
        let rules = rules("skins.1=2\nminHeight.1=10\nmaxHeight.1=20\n");
        assert_eq!(rules.pick(&at(15), 0), Some(2));
        assert_eq!(rules.pick(&at(21), 0), None);
        let open = self::rules("skins.1=2\nminHeight.1=60\n");
        assert_eq!(open.pick(&at(200), 0), Some(2));
    }

    #[test]
    fn name_and_height_must_both_hold() {
        let rules = rules("skins.1=2\nname.1=Pinky\nheights.1=0-100\n");
        let pinky = Spawn {
            name: Some("Pinky"),
            height: Some(64),
            ..Spawn::default()
        };
        assert_eq!(rules.pick(&pinky, 0), Some(2));
        assert_eq!(rules.pick(&at(64), 0), None, "OptiFine would match here");
    }

    #[test]
    fn a_negated_name_matches_an_unnamed_entity() {
        let rules = rules("skins.1=2\nname.1=!Pinky\n");
        assert_eq!(rules.pick(&Spawn::default(), 0), Some(2));
    }

    #[test]
    fn professions_and_careers() {
        let rules = rules("skins.1=2\nprofessions.1=farmer librarian:cartographer\n");
        let villager = |profession, career| Spawn {
            profession: Some(profession),
            career,
            ..Spawn::default()
        };
        assert_eq!(rules.pick(&villager(0, Some(3)), 0), Some(2));
        assert_eq!(rules.pick(&villager(1, Some(2)), 0), Some(2));
        assert_eq!(rules.pick(&villager(1, Some(1)), 0), None);
        assert_eq!(rules.pick(&villager(1, None), 0), None);
        assert_eq!(rules.pick(&villager(2, None), 0), None);
        assert_eq!(rules.pick(&Spawn::default(), 0), Some(2), "not a villager");
    }

    #[test]
    fn a_rule_with_a_missing_texture_or_a_bad_profession_is_dropped() {
        let rules = rules("skins.1=9\nskins.2=3\nskins.3=2\nprofessions.3=wizard\nskins.4=4\n");
        assert_eq!(rules.pick(&Spawn::default(), 0), Some(3));
    }

    #[test]
    fn a_file_with_no_usable_rule_is_an_error() {
        assert_eq!(
            RandomRules::parse(b"skins.1=9\n", |_| false).err(),
            Some(RandomRulesError::NoRule)
        );
        assert_eq!(
            RandomRules::parse(b"nothing=here\n", |_| true).err(),
            Some(RandomRulesError::NoRule)
        );
        assert!(matches!(
            RandomRules::parse(b"a=\\u12", |_| true),
            Err(RandomRulesError::Properties(_))
        ));
    }

    #[test]
    fn skin_one_is_always_available() {
        let rules = RandomRules::parse(b"skins.1=1 2\n", |_| false);
        assert!(rules.is_err(), "2 is missing");
        assert!(RandomRules::parse(b"skins.1=1\n", |_| false).is_ok());
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_files_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..300)) {
            if let Ok(rules) = RandomRules::parse(&bytes, |n| n % 2 == 0) {
                let _ = rules.pick(&Spawn::default(), 7);
            }
        }

        #[test]
        fn arbitrary_values_never_panic(
            skins in "[0-9 ,-]{0,20}",
            weights in "[0-9 ,-]{0,20}",
            name in ".{0,10}",
        ) {
            let text = format!("skins.1={skins}\nweights.1={weights}\nname.1={name}\nbiomes.1=!x\n");
            if let Ok(rules) = RandomRules::parse(text.as_bytes(), |_| true) {
                let _ = rules.pick(&at(5), 123);
            }
        }
    }
}
