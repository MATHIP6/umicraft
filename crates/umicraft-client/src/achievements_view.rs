// SPDX-License-Identifier: MIT OR Apache-2.0

//! The data of the achievements screen, built from the `Statistics` (0x37)
//! answer (MCP `AchievementList`, `StatFileWriter`).
//!
//! An achievement is unlocked when its statistic `achievement.<id>` is above
//! zero. The table of cells, icons and parents is game data in
//! [`umicraft_game::achievements`]; this module resolves the names with the
//! player's language and the icons with the item resources. It also finds
//! which achievements a new answer unlocked, for the "Achievement get!" toast.
//!
//! Everything here is pure, so it is tested without a window.

use std::collections::HashMap;

use umicraft_core::lang::Lang;
use umicraft_game::achievements::{stat_name, AchievementDef, ACHIEVEMENTS};
use umicraft_game::items;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_ui::menus::{AchievementNode, AchievementsData};
use umicraft_ui::SlotItem;

use crate::item_icons::{slot_item, ItemView};

/// What the builder needs besides the numbers.
#[derive(Clone, Copy)]
pub struct AchievementsContext<'a> {
    /// The language of names and descriptions.
    pub lang: &'a Lang,
    /// The icon source.
    pub items: ItemView<'a>,
    /// The name of the inventory key, for "Press 'E' to open your inventory".
    pub inventory_key: &'a str,
}

/// Whether `stats` has the achievement `id` unlocked.
fn is_unlocked(stats: &HashMap<&str, i32>, id: &str) -> bool {
    stats
        .get(stat_name(id).as_str())
        .is_some_and(|value| *value > 0)
}

fn lookup(stats: &[(String, i32)]) -> HashMap<&str, i32> {
    stats
        .iter()
        .map(|(name, value)| (name.as_str(), *value))
        .collect()
}

/// Builds the screen's data in `AchievementList` order.
#[must_use]
pub fn build(stats: &[(String, i32)], context: AchievementsContext<'_>) -> AchievementsData {
    let values = lookup(stats);
    let nodes = ACHIEVEMENTS
        .iter()
        .map(|def| node(def, &values, context))
        .collect();
    AchievementsData { nodes }
}

fn node(
    def: &AchievementDef,
    values: &HashMap<&str, i32>,
    context: AchievementsContext<'_>,
) -> AchievementNode {
    let key = format!("achievement.{}", def.id);
    let description = context
        .lang
        .tr_args(&format!("{key}.desc"), &[context.inventory_key]);
    let item_id = items::all()
        .iter()
        .find(|item| item.name == def.icon)
        .map_or(0, |item| i16::try_from(item.id).unwrap_or(i16::MAX));
    let stack = ItemStack {
        item_id,
        stack_size: 1,
        damage: def.icon_damage,
        nbt: None,
    };
    AchievementNode {
        column: i32::from(def.column),
        row: i32::from(def.row),
        special: def.special,
        icon: slot_item(context.items, &stack),
        name: context.lang.tr(&key).to_string(),
        description,
        parent: def
            .parent
            .and_then(|parent| ACHIEVEMENTS.iter().position(|other| other.id == parent)),
        unlocked: is_unlocked(values, def.id),
    }
}

/// The name and icon of achievement `id`, for its toast.
#[must_use]
pub fn toast_content(id: &str, context: AchievementsContext<'_>) -> Option<(String, SlotItem)> {
    let def = ACHIEVEMENTS.iter().find(|def| def.id == id)?;
    let built = node(def, &HashMap::new(), context);
    Some((built.name, built.icon))
}

/// `String.hashCode()` of `text`: the seed of the map tiles is the hash of the
/// player id, so each player sees the same ground every time.
#[must_use]
pub fn java_string_hash(text: &str) -> i32 {
    text.encode_utf16().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

/// The ids newly unlocked by `after` compared with `before`.
#[must_use]
pub fn newly_unlocked(before: &[(String, i32)], after: &[(String, i32)]) -> Vec<&'static str> {
    let (before, after) = (lookup(before), lookup(after));
    ACHIEVEMENTS
        .iter()
        .filter(|def| is_unlocked(&after, def.id) && !is_unlocked(&before, def.id))
        .map(|def| def.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dials::DialFrames;
    use umicraft_assets::ACHIEVEMENT_TILES;
    use umicraft_ui::menus::{MapTile, Progress};

    fn stats(names: &[&str]) -> Vec<(String, i32)> {
        names
            .iter()
            .map(|name| (format!("achievement.{name}"), 1))
            .collect()
    }

    fn build_with(stats: &[(String, i32)]) -> AchievementsData {
        let lang = Lang::from_texts(
            "en_US",
            Some("achievement.openInventory.desc=Press '%1$s' to open your inventory.\n"),
            None,
        );
        let context = AchievementsContext {
            lang: &lang,
            items: ItemView::new(None, DialFrames::default()),
            inventory_key: "E",
        };
        build(stats, context)
    }

    #[test]
    fn every_achievement_becomes_a_node_with_its_parent_index() {
        let data = build_with(&[]);
        assert_eq!(data.nodes.len(), ACHIEVEMENTS.len());
        assert_eq!(data.nodes[0].parent, None);
        // mineWood hangs on openInventory
        assert_eq!(data.nodes[1].parent, Some(0));
        assert!(data.nodes.iter().all(|node| !node.unlocked));
    }

    #[test]
    fn a_positive_statistic_unlocks_the_achievement() {
        let data = build_with(&stats(&["openInventory", "mineWood"]));
        assert_eq!(data.unlocked_count(), 2);
        assert_eq!(data.progress(2), Progress::Available);
        // a zero value is not unlocked
        let zero = vec![("achievement.openInventory".to_string(), 0)];
        assert_eq!(build_with(&zero).unlocked_count(), 0);
    }

    #[test]
    fn the_inventory_description_names_the_key() {
        let data = build_with(&[]);
        assert_eq!(
            data.nodes[0].description,
            "Press 'E' to open your inventory."
        );
    }

    #[test]
    fn newly_unlocked_lists_only_the_changes() {
        let before = stats(&["openInventory"]);
        let after = stats(&["openInventory", "mineWood", "buildWorkBench"]);
        assert_eq!(
            newly_unlocked(&before, &after),
            ["mineWood", "buildWorkBench"]
        );
        assert!(newly_unlocked(&after, &after).is_empty());
    }

    #[test]
    fn a_toast_has_the_name_of_its_achievement() {
        let lang = Lang::from_texts("en_US", Some("achievement.mineWood=Getting Wood\n"), None);
        let context = AchievementsContext {
            lang: &lang,
            items: ItemView::new(None, DialFrames::default()),
            inventory_key: "E",
        };
        let (name, _) = toast_content("mineWood", context).expect("known id");
        assert_eq!(name, "Getting Wood");
        assert!(toast_content("nope", context).is_none());
    }

    #[test]
    fn the_string_hash_is_javas() {
        assert_eq!(java_string_hash(""), 0);
        assert_eq!(java_string_hash("a"), 97);
        assert_eq!(java_string_hash("hello"), 99_162_322);
    }

    #[test]
    fn the_map_tiles_are_the_textures_the_assets_load() {
        assert_eq!(MapTile::TEXTURES, ACHIEVEMENT_TILES);
    }
}
