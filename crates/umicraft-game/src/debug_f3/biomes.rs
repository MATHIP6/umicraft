// SPDX-License-Identifier: MIT OR Apache-2.0

//! The biome names the F3 `Biome:` line prints (MCP `BiomeGenBase.biomeName`).
//!
//! The base biomes 0 to 39 come from the `setBiomeName` calls of
//! `BiomeGenBase`; the mutations (`id + 128`) are named by
//! `BiomeGenMutated` (`base + " M"`) and the overrides in `BiomeGenForest`,
//! `BiomeGenTaiga`, `BiomeGenMesa` and the static block of `BiomeGenBase`.

/// The names of the base biomes, indexed by id.
const BASE: [&str; 40] = [
    "Ocean",
    "Plains",
    "Desert",
    "Extreme Hills",
    "Forest",
    "Taiga",
    "Swampland",
    "River",
    "Hell",
    "The End",
    "FrozenOcean",
    "FrozenRiver",
    "Ice Plains",
    "Ice Mountains",
    "MushroomIsland",
    "MushroomIslandShore",
    "Beach",
    "DesertHills",
    "ForestHills",
    "TaigaHills",
    "Extreme Hills Edge",
    "Jungle",
    "JungleHills",
    "JungleEdge",
    "Deep Ocean",
    "Stone Beach",
    "Cold Beach",
    "Birch Forest",
    "Birch Forest Hills",
    "Roofed Forest",
    "Cold Taiga",
    "Cold Taiga Hills",
    "Mega Taiga",
    "Mega Taiga Hills",
    "Extreme Hills+",
    "Savanna",
    "Savanna Plateau",
    "Mesa",
    "Mesa Plateau F",
    "Mesa Plateau",
];

/// Biomes that have a mutation (`createMutation()` calls of `BiomeGenBase`).
const MUTATED: [u8; 20] = [
    1, 2, 4, 5, 6, 12, 21, 23, 30, 35, 36, 37, 38, 39, 27, 28, 29, 32, 3, 34,
];

/// The id offset of a mutation (`biomeID + 128`).
const MUTATION_OFFSET: u8 = 128;

/// The F3 name of a biome id, or `None` for an id the reference does not
/// register (it prints the name of a null biome there, which crashes).
#[must_use]
pub fn biome_name(id: u8) -> Option<String> {
    if let Some(name) = BASE.get(usize::from(id)) {
        return Some((*name).to_owned());
    }
    let base = id.checked_sub(MUTATION_OFFSET)?;
    match base {
        4 => Some("Flower Forest".to_owned()),
        32 => Some("Mega Spruce Taiga".to_owned()),
        33 => Some("Redwood Taiga Hills M".to_owned()),
        37 => Some("Mesa (Bryce)".to_owned()),
        _ if MUTATED.contains(&base) => Some(format!("{} M", BASE[usize::from(base)])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_biomes_keep_their_reference_spelling() {
        assert_eq!(biome_name(1).as_deref(), Some("Plains"));
        assert_eq!(biome_name(10).as_deref(), Some("FrozenOcean"));
        assert_eq!(biome_name(34).as_deref(), Some("Extreme Hills+"));
        assert_eq!(biome_name(39).as_deref(), Some("Mesa Plateau"));
    }

    #[test]
    fn mutations_append_m_except_the_overrides() {
        assert_eq!(biome_name(129).as_deref(), Some("Plains M"));
        assert_eq!(biome_name(132).as_deref(), Some("Flower Forest"));
        assert_eq!(biome_name(160).as_deref(), Some("Mega Spruce Taiga"));
        assert_eq!(biome_name(161).as_deref(), Some("Redwood Taiga Hills M"));
        assert_eq!(biome_name(165).as_deref(), Some("Mesa (Bryce)"));
        assert_eq!(biome_name(162).as_deref(), Some("Extreme Hills+ M"));
    }

    #[test]
    fn unregistered_ids_have_no_name() {
        assert_eq!(biome_name(40), None);
        assert_eq!(biome_name(128), None);
        assert_eq!(biome_name(138), None);
        assert_eq!(biome_name(255), None);
    }
}
