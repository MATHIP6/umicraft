// SPDX-License-Identifier: MIT OR Apache-2.0

//! The random names of the enchanting table rows (MCP `EnchantmentNameParts`).
//!
//! `GuiEnchantment` reseeds the generator with the seed of window property 3
//! every frame and draws one name per row, three or four words long, in the
//! standard galactic alphabet. The words carry no meaning; only the seed
//! decides them, so every client shows the same names for the same table.

use umicraft_core::java_random::JavaRandom;

/// The words of `EnchantmentNameParts.namePartsArray`, in its order (the
/// trailing empty string of the Java `split` is dropped there too).
pub const WORDS: [&str; 55] = [
    "the",
    "elder",
    "scrolls",
    "klaatu",
    "berata",
    "niktu",
    "xyzzy",
    "bless",
    "curse",
    "light",
    "darkness",
    "fire",
    "air",
    "earth",
    "water",
    "hot",
    "dry",
    "cold",
    "wet",
    "ignite",
    "snuff",
    "embiggen",
    "twist",
    "shorten",
    "stretch",
    "fiddle",
    "destroy",
    "imbue",
    "galvanize",
    "enchant",
    "free",
    "limited",
    "range",
    "of",
    "towards",
    "inside",
    "sphere",
    "cube",
    "self",
    "other",
    "ball",
    "mental",
    "physical",
    "grow",
    "shrink",
    "demon",
    "elemental",
    "spirit",
    "animal",
    "creature",
    "beast",
    "humanoid",
    "undead",
    "fresh",
    "stale",
];

/// The most words a name has (`rand.nextInt(2) + 3`).
const MAX_WORDS: usize = 4;

/// One generated name: three or four indices into [`WORDS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantName {
    words: [u8; MAX_WORDS],
    len: u8,
}

impl EnchantName {
    /// The words of the name, in order.
    pub fn words(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.words[..usize::from(self.len)]
            .iter()
            .map(|&index| WORDS[usize::from(index)])
    }
}

/// The names of the three rows for the table seed (window property 3, the
/// `xpSeed` the server truncated to a short), as `generateNewRandomName`
/// draws them one after the other from `new Random(seed)`.
#[must_use]
pub fn row_names(seed: i16) -> [EnchantName; 3] {
    // `reseedRandomGenerator((long) xpSeed)`: the short is sign-extended.
    let mut random = JavaRandom::new(i64::from(seed) as u64);
    std::array::from_fn(|_| next_name(&mut random))
}

/// `generateNewRandomName`.
fn next_name(random: &mut JavaRandom) -> EnchantName {
    let len = random.next_int(2) + 3;
    let mut words = [0; MAX_WORDS];
    for word in words.iter_mut().take(len as usize) {
        // Fewer than 256 words, so the index fits a byte.
        *word = random.next_int(WORDS.len() as u32) as u8;
    }
    EnchantName {
        words,
        len: len as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(seed: i16) -> Vec<String> {
        row_names(seed)
            .iter()
            .map(|name| name.words().collect::<Vec<_>>().join(" "))
            .collect()
    }

    /// Expected values from an independent port of `java.util.Random`.
    #[test]
    fn the_names_follow_the_java_generator() {
        assert_eq!(
            names(0),
            [
                "galvanize ignite imbue beast",
                "elemental xyzzy stretch",
                "scrolls scrolls earth imbue"
            ]
        );
        assert_eq!(
            names(-16),
            [
                "elemental the water",
                "destroy enchant shrink",
                "free grow klaatu curse"
            ]
        );
    }
}
