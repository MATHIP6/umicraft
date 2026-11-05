// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 achievements: their place on the achievement screen, their icon
//! and their parent (MCP `AchievementList`).
//!
//! The table is game data, the same for every client: `column` and `row` are
//! in 24 pixel cells from the "Taking Inventory" cell, `parent` is the
//! achievement that must be unlocked first, and `special` selects the rounded
//! frame. The order is the registration order of `AchievementList`, which is
//! also the draw order of `GuiAchievements`. The statistic name the server
//! sends is `achievement.<id>`. See `docs/sources.md`.

/// One achievement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AchievementDef {
    /// The id: the language key is `achievement.<id>` (and `.desc`).
    pub id: &'static str,
    /// The column of the cell, relative to the first achievement.
    pub column: i8,
    /// The row of the cell.
    pub row: i8,
    /// The registry name of the icon item (blocks use their item).
    pub icon: &'static str,
    /// The damage value of the icon stack.
    pub icon_damage: i16,
    /// The id of the achievement to unlock first.
    pub parent: Option<&'static str>,
    /// Whether the achievement has the special (rounded) frame.
    pub special: bool,
}

const fn def(
    id: &'static str,
    (column, row): (i8, i8),
    icon: &'static str,
    parent: Option<&'static str>,
) -> AchievementDef {
    AchievementDef {
        id,
        column,
        row,
        icon,
        icon_damage: 0,
        parent,
        special: false,
    }
}

const fn special(
    id: &'static str,
    cell: (i8, i8),
    icon: &'static str,
    parent: Option<&'static str>,
) -> AchievementDef {
    let mut achievement = def(id, cell, icon, parent);
    achievement.special = true;
    achievement
}

const fn with_damage(mut achievement: AchievementDef, damage: i16) -> AchievementDef {
    achievement.icon_damage = damage;
    achievement
}

/// Every achievement, in `AchievementList` order.
pub const ACHIEVEMENTS: &[AchievementDef] = &[
    def("openInventory", (0, 0), "book", None),
    def("mineWood", (2, 1), "log", Some("openInventory")),
    def(
        "buildWorkBench",
        (4, -1),
        "crafting_table",
        Some("mineWood"),
    ),
    def(
        "buildPickaxe",
        (4, 2),
        "wooden_pickaxe",
        Some("buildWorkBench"),
    ),
    def("buildFurnace", (3, 4), "furnace", Some("buildPickaxe")),
    def("acquireIron", (1, 4), "iron_ingot", Some("buildFurnace")),
    def("buildHoe", (2, -3), "wooden_hoe", Some("buildWorkBench")),
    def("makeBread", (-1, -3), "bread", Some("buildHoe")),
    def("bakeCake", (0, -5), "cake", Some("buildHoe")),
    def(
        "buildBetterPickaxe",
        (6, 2),
        "stone_pickaxe",
        Some("buildPickaxe"),
    ),
    def("cookFish", (2, 6), "cooked_fish", Some("buildFurnace")),
    special("onARail", (2, 3), "rail", Some("acquireIron")),
    def(
        "buildSword",
        (6, -1),
        "wooden_sword",
        Some("buildWorkBench"),
    ),
    def("killEnemy", (8, -1), "bone", Some("buildSword")),
    def("killCow", (7, -3), "leather", Some("buildSword")),
    special("flyPig", (9, -3), "saddle", Some("killCow")),
    special("snipeSkeleton", (7, 0), "bow", Some("killEnemy")),
    def("diamonds", (-1, 5), "diamond_ore", Some("acquireIron")),
    def("diamondsToYou", (-1, 2), "diamond", Some("diamonds")),
    def("portal", (-1, 7), "obsidian", Some("diamonds")),
    special("ghast", (-4, 8), "ghast_tear", Some("portal")),
    def("blazeRod", (0, 9), "blaze_rod", Some("portal")),
    def("potion", (2, 8), "potion", Some("blazeRod")),
    special("theEnd", (3, 10), "ender_eye", Some("blazeRod")),
    special("theEnd2", (4, 13), "dragon_egg", Some("theEnd")),
    def(
        "enchantments",
        (-4, 4),
        "enchanting_table",
        Some("diamonds"),
    ),
    special("overkill", (-4, 1), "diamond_sword", Some("enchantments")),
    def("bookcase", (-3, 6), "bookshelf", Some("enchantments")),
    def("breedCow", (7, -5), "wheat", Some("killCow")),
    with_damage(def("spawnWither", (7, 12), "skull", Some("theEnd2")), 1),
    def("killWither", (7, 10), "nether_star", Some("spawnWither")),
    special("fullBeacon", (7, 8), "beacon", Some("killWither")),
    special("exploreAllBiomes", (4, 8), "diamond_boots", Some("theEnd")),
    with_damage(
        special(
            "overpowered",
            (6, 4),
            "golden_apple",
            Some("buildBetterPickaxe"),
        ),
        1,
    ),
];

/// The statistic name the server sends for achievement `id`.
#[must_use]
pub fn stat_name(id: &str) -> String {
    format!("achievement.{id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items;

    #[test]
    fn the_table_has_the_34_achievements_of_1_8() {
        assert_eq!(ACHIEVEMENTS.len(), 34);
    }

    #[test]
    fn ids_are_unique_and_parents_come_first() {
        for (index, achievement) in ACHIEVEMENTS.iter().enumerate() {
            let earlier = &ACHIEVEMENTS[..index];
            assert!(earlier.iter().all(|other| other.id != achievement.id));
            if let Some(parent) = achievement.parent {
                assert!(earlier.iter().any(|other| other.id == parent), "{parent}");
            }
        }
        assert_eq!(
            ACHIEVEMENTS.iter().filter(|a| a.parent.is_none()).count(),
            1
        );
    }

    #[test]
    fn every_icon_is_a_registered_item() {
        for achievement in ACHIEVEMENTS {
            let known = items::all()
                .iter()
                .any(|item| item.name == achievement.icon);
            assert!(known, "{} uses {}", achievement.id, achievement.icon);
        }
    }

    #[test]
    fn no_two_achievements_share_a_cell() {
        for (index, a) in ACHIEVEMENTS.iter().enumerate() {
            for b in &ACHIEVEMENTS[index + 1..] {
                assert_ne!((a.column, a.row), (b.column, b.row), "{} {}", a.id, b.id);
            }
        }
    }

    #[test]
    fn the_extent_matches_the_reference() {
        let columns = ACHIEVEMENTS.iter().map(|a| a.column);
        let rows = ACHIEVEMENTS.iter().map(|a| a.row);
        assert_eq!((columns.clone().min(), columns.max()), (Some(-4), Some(9)));
        assert_eq!((rows.clone().min(), rows.max()), (Some(-5), Some(13)));
    }
}
