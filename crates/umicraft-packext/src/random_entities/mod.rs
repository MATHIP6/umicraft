// SPDX-License-Identifier: MIT OR Apache-2.0

//! Random Entities: several textures for one kind of mob (F6 phase 7).
//!
//! A pack puts alternative textures next to the vanilla one, under
//! `mob/` (MCPatcher's folder) or `random/entity/` (the current OptiFine
//! folder), both inside a pack-feature folder. For
//! `textures/entity/creeper/creeper.png` they are `creeper2.png`,
//! `creeper3.png`, and so on; the vanilla texture is number 1. An optional
//! `creeper.properties` next to them picks among the textures with rules.
//!
//! This module has no file access. It names the paths to look for
//! ([`variant_paths`], [`properties_paths`]), reads the rules
//! ([`RandomRules::parse`]) and chooses a texture number ([`RandomSkins::pick`])
//! from a stable per-entity number ([`random_id`]); the client reads the files.
//!
//! Behaviour from OptiFine's `RandomMobs`, `RandomMobsProperties` and
//! `RandomMobsRule` (decompiled 1.8.9 build, ADR-0088) and the public
//! OptiFineDoc `random_entities.properties`.

mod rules;

pub use rules::{Profession, RandomRules, RandomRulesError, Spawn};

/// The folders (inside a pack-feature folder) that hold the alternative
/// textures, with the part of the vanilla path they replace.
const FOLDERS: [&str; 2] = ["mob/", "random/entity/"];
/// Where a vanilla entity texture lives.
const VANILLA_PREFIX: &str = "textures/entity/";
/// The suffixes of textures that share the rules of their base texture:
/// `wolf_tame.png` uses `wolf.properties` (OptiFine `DEPENDANT_SUFFIXES`).
const DEPENDANT_SUFFIXES: [&str; 9] = [
    "_armor",
    "_eyes",
    "_exploding",
    "_shooting",
    "_fur",
    "_invulnerable",
    "_angry",
    "_tame",
    "_collar",
];
/// How many missing numbers in a row end the search for variants; OptiFine's
/// loop `i < size + 10` tolerates gaps of up to nine.
const MAX_GAP: u32 = 10;

/// The texture number of the vanilla texture.
pub const VANILLA: u32 = 1;

/// The paths, relative to a pack-feature folder, where variant `number` of the
/// vanilla texture `texture` (`textures/entity/...png`) may be. Empty for
/// number 1 and for a texture that is not an entity texture.
///
/// A name that ends in a digit gets a `.` before the number
/// (`pulse_2.png` gives `pulse_2.2.png`), as the documentation says.
///
/// # Example
///
/// ```
/// use umicraft_packext::random_entities::variant_paths;
///
/// assert_eq!(
///     variant_paths("textures/entity/creeper/creeper.png", 3),
///     ["mob/creeper/creeper3.png", "random/entity/creeper/creeper3.png"]
/// );
/// ```
#[must_use]
pub fn variant_paths(texture: &str, number: u32) -> Vec<String> {
    let Some(rest) = texture.strip_prefix(VANILLA_PREFIX) else {
        return Vec::new();
    };
    if number <= VANILLA {
        return Vec::new();
    }
    FOLDERS
        .iter()
        .map(|folder| indexed(&format!("{folder}{rest}"), number))
        .collect()
}

/// `path` with `number` before the extension.
fn indexed(path: &str, number: u32) -> String {
    let (stem, extension) = path.rsplit_once('.').unwrap_or((path, ""));
    let separator = if stem.ends_with(|c: char| c.is_ascii_digit()) {
        "."
    } else {
        ""
    };
    format!("{stem}{separator}{number}.{extension}")
}

/// The paths where the rules of the vanilla texture `texture` may be, best
/// first: its own `.properties`, then that of its base texture when it has a
/// dependant suffix.
#[must_use]
pub fn properties_paths(texture: &str) -> Vec<String> {
    let Some(rest) = texture.strip_prefix(VANILLA_PREFIX) else {
        return Vec::new();
    };
    let stem = rest.strip_suffix(".png").unwrap_or(rest);
    let parent = DEPENDANT_SUFFIXES
        .iter()
        .find_map(|suffix| stem.strip_suffix(suffix));
    let stems = std::iter::once(stem).chain(parent);
    stems
        .flat_map(|stem| {
            FOLDERS
                .iter()
                .map(move |folder| format!("{folder}{stem}.properties"))
        })
        .collect()
}

/// The texture numbers that exist, found by asking `exists` for 2, 3, ...
/// and giving up after [`MAX_GAP`] misses in a row. Always starts with
/// [`VANILLA`].
pub fn probe_variants(mut exists: impl FnMut(u32) -> bool) -> Vec<u32> {
    let mut found = vec![VANILLA];
    let (mut number, mut misses) = (VANILLA + 1, 0);
    while misses < MAX_GAP {
        if exists(number) {
            found.push(number);
            misses = 0;
        } else {
            misses += 1;
        }
        number += 1;
    }
    found
}

/// The number that decides which texture an entity wears: the low 31 bits of
/// the least significant half of its UUID. Entities that have no UUID in the
/// 1.8 protocol (mobs are spawned without one) pass `fallback`, any stable
/// per-entity number.
#[must_use]
pub fn random_id(uuid: Option<[u8; 16]>, fallback: u64) -> u32 {
    let low = uuid.map_or(fallback, |uuid| {
        let mut half = [0; 8];
        half.copy_from_slice(&uuid[8..]);
        u64::from_be_bytes(half)
    });
    (low & 0x7fff_ffff) as u32
}

/// The textures one kind of mob may wear and how one is chosen.
///
/// # Example
///
/// ```
/// use umicraft_packext::random_entities::{RandomSkins, Spawn};
///
/// let skins = RandomSkins::from_variants(vec![1, 2, 3]);
/// assert_eq!(skins.pick(&Spawn::default(), 7), 2);
/// ```
#[derive(Debug, Clone)]
pub struct RandomSkins {
    /// The numbers that exist, used without rules.
    variants: Vec<u32>,
    /// The rules of the `.properties` file, if there is one.
    rules: Option<RandomRules>,
}

impl RandomSkins {
    /// Entities that pick among `variants` by `id % len`, no rules.
    #[must_use]
    pub fn from_variants(variants: Vec<u32>) -> Self {
        Self {
            variants,
            rules: None,
        }
    }

    /// Entities that follow `rules`; one that matches no rule wears the
    /// vanilla texture.
    #[must_use]
    pub fn from_rules(rules: RandomRules) -> Self {
        Self {
            variants: Vec::new(),
            rules: Some(rules),
        }
    }

    /// Whether the set can ever choose something other than the vanilla
    /// texture.
    #[must_use]
    pub fn is_useful(&self) -> bool {
        self.rules.is_some() || self.variants.len() > 1
    }

    /// The texture number for an entity with this `spawn` and `random_id`.
    #[must_use]
    pub fn pick(&self, spawn: &Spawn<'_>, random_id: u32) -> u32 {
        if let Some(rules) = &self.rules {
            return rules.pick(spawn, random_id).unwrap_or(VANILLA);
        }
        match self.variants.len() {
            0 => VANILLA,
            len => self.variants[random_id as usize % len],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_paths_cover_both_folders() {
        assert_eq!(
            variant_paths("textures/entity/zombie/zombie.png", 2),
            ["mob/zombie/zombie2.png", "random/entity/zombie/zombie2.png"]
        );
        assert!(variant_paths("textures/entity/zombie/zombie.png", 1).is_empty());
        assert!(variant_paths("textures/misc/shadow.png", 2).is_empty());
    }

    #[test]
    fn a_name_ending_in_a_digit_gets_a_dot() {
        assert_eq!(
            variant_paths("textures/entity/warden/pulse_2.png", 3)[0],
            "mob/warden/pulse_2.3.png"
        );
    }

    #[test]
    fn dependant_textures_fall_back_to_the_base_rules() {
        assert_eq!(
            properties_paths("textures/entity/wolf/wolf_tame.png"),
            [
                "mob/wolf/wolf_tame.properties",
                "random/entity/wolf/wolf_tame.properties",
                "mob/wolf/wolf.properties",
                "random/entity/wolf/wolf.properties",
            ]
        );
        assert_eq!(properties_paths("textures/entity/pig/pig.png").len(), 2);
    }

    #[test]
    fn probing_tolerates_gaps_but_not_forever() {
        let present = [2, 3, 8];
        assert_eq!(probe_variants(|n| present.contains(&n)), [1, 2, 3, 8]);
        assert_eq!(probe_variants(|_| false), [1]);
        assert_eq!(
            probe_variants(|n| n == 12),
            [1],
            "ten misses end the search"
        );
    }

    #[test]
    fn the_random_id_is_the_low_31_bits_of_the_least_significant_half() {
        let mut uuid = [0xaa; 16];
        uuid[8..].copy_from_slice(&0xffff_ffff_8000_0005_u64.to_be_bytes());
        assert_eq!(random_id(Some(uuid), 99), 5);
        assert_eq!(random_id(None, 0x1_0000_0009), 9);
    }

    #[test]
    fn without_rules_the_id_picks_modulo_the_count() {
        let skins = RandomSkins::from_variants(vec![1, 2, 5]);
        let picks: Vec<_> = (0..4).map(|id| skins.pick(&Spawn::default(), id)).collect();
        assert_eq!(picks, [1, 2, 5, 1]);
        assert!(skins.is_useful());
        assert!(!RandomSkins::from_variants(vec![1]).is_useful());
    }
}
