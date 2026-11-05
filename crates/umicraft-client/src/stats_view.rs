// SPDX-License-Identifier: MIT OR Apache-2.0

//! The data of the statistics screen, built from the `Statistics` (0x37)
//! answer: the general counters, the per-item and per-block counters and the
//! per-mob kills (MCP `StatList`, `GuiStats`).
//!
//! The packet carries `name -> value` pairs. The general counters are named by
//! their language key (`stat.walkOneCm`). The per-object counters append the
//! registry name with the colon turned into a dot
//! (`stat.mineBlock.minecraft.stone`, `StatList.func_180204_a`), and the mob
//! counters append the entity name (`stat.killEntity.Zombie`,
//! `stat.entityKilledBy.Zombie`, `EntityList`). Achievements
//! (`achievement.*`) belong to [`crate::achievements_view`].
//!
//! Everything here is pure, so it is tested without a window.

use std::collections::HashMap;

use umicraft_core::lang::Lang;
use umicraft_game::inventory::tooltip::tooltip;
use umicraft_game::items::{self, ItemDef, ItemKind};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_ui::menus::{MobRow, ObjectRow, StatCell, StatRow, StatsData};

use crate::item_icons::{slot_item, ItemView};
use crate::tooltip_draw::resolve_in;

/// `StatList.generalStats` in registration order: the statistic id and the
/// key of its label (the workbench label is the one name that differs).
const GENERAL: &[(&str, &str)] = &[
    ("stat.leaveGame", "stat.leaveGame"),
    ("stat.playOneMinute", "stat.playOneMinute"),
    ("stat.timeSinceDeath", "stat.timeSinceDeath"),
    ("stat.walkOneCm", "stat.walkOneCm"),
    ("stat.crouchOneCm", "stat.crouchOneCm"),
    ("stat.sprintOneCm", "stat.sprintOneCm"),
    ("stat.swimOneCm", "stat.swimOneCm"),
    ("stat.fallOneCm", "stat.fallOneCm"),
    ("stat.climbOneCm", "stat.climbOneCm"),
    ("stat.flyOneCm", "stat.flyOneCm"),
    ("stat.diveOneCm", "stat.diveOneCm"),
    ("stat.minecartOneCm", "stat.minecartOneCm"),
    ("stat.boatOneCm", "stat.boatOneCm"),
    ("stat.pigOneCm", "stat.pigOneCm"),
    ("stat.horseOneCm", "stat.horseOneCm"),
    ("stat.jump", "stat.jump"),
    ("stat.drop", "stat.drop"),
    ("stat.damageDealt", "stat.damageDealt"),
    ("stat.damageTaken", "stat.damageTaken"),
    ("stat.deaths", "stat.deaths"),
    ("stat.mobKills", "stat.mobKills"),
    ("stat.animalsBred", "stat.animalsBred"),
    ("stat.playerKills", "stat.playerKills"),
    ("stat.fishCaught", "stat.fishCaught"),
    ("stat.junkFished", "stat.junkFished"),
    ("stat.treasureFished", "stat.treasureFished"),
    ("stat.talkedToVillager", "stat.talkedToVillager"),
    ("stat.tradedWithVillager", "stat.tradedWithVillager"),
    ("stat.cakeSlicesEaten", "stat.cakeSlicesEaten"),
    ("stat.cauldronFilled", "stat.cauldronFilled"),
    ("stat.cauldronUsed", "stat.cauldronUsed"),
    ("stat.armorCleaned", "stat.armorCleaned"),
    ("stat.bannerCleaned", "stat.bannerCleaned"),
    (
        "stat.brewingstandInteraction",
        "stat.brewingstandInteraction",
    ),
    ("stat.beaconInteraction", "stat.beaconInteraction"),
    ("stat.dropperInspected", "stat.dropperInspected"),
    ("stat.hopperInspected", "stat.hopperInspected"),
    ("stat.dispenserInspected", "stat.dispenserInspected"),
    ("stat.noteblockPlayed", "stat.noteblockPlayed"),
    ("stat.noteblockTuned", "stat.noteblockTuned"),
    ("stat.flowerPotted", "stat.flowerPotted"),
    ("stat.trappedChestTriggered", "stat.trappedChestTriggered"),
    ("stat.enderchestOpened", "stat.enderchestOpened"),
    ("stat.itemEnchanted", "stat.itemEnchanted"),
    ("stat.recordPlayed", "stat.recordPlayed"),
    ("stat.furnaceInteraction", "stat.furnaceInteraction"),
    ("stat.craftingTableInteraction", "stat.workbenchInteraction"),
    ("stat.chestOpened", "stat.chestOpened"),
];

/// The entities with a spawn egg, in `EntityList.entityEggs` order: the
/// mobs of the Mobs tab.
const MOBS: &[&str] = &[
    "Creeper",
    "Skeleton",
    "Spider",
    "Zombie",
    "Slime",
    "Ghast",
    "PigZombie",
    "Enderman",
    "CaveSpider",
    "Silverfish",
    "Blaze",
    "LavaSlime",
    "Bat",
    "Witch",
    "Endermite",
    "Guardian",
    "Pig",
    "Sheep",
    "Cow",
    "Chicken",
    "Squid",
    "Wolf",
    "MushroomCow",
    "Ozelot",
    "EntityHorse",
    "Rabbit",
    "Villager",
];

/// What the player has done with one item or block.
#[derive(Debug, Default, Clone, Copy)]
struct ObjectCounts {
    crafted: Option<i32>,
    used: Option<i32>,
    mined: Option<i32>,
    broken: Option<i32>,
}

/// Whether any of the counters of a tab is above zero.
fn any_positive(counters: [Option<i32>; 3]) -> bool {
    counters.iter().flatten().any(|&value| value > 0)
}

/// What the builder needs besides the numbers.
#[derive(Clone, Copy)]
pub struct StatsContext<'a> {
    /// The language of the item names.
    pub lang: &'a Lang,
    /// The icon source.
    pub items: ItemView<'a>,
}

/// Builds the four tabs from the statistics the server sent.
#[must_use]
pub fn build(stats: &[(String, i32)], context: StatsContext<'_>) -> StatsData {
    let values: HashMap<&str, i32> = stats.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let mut counts: HashMap<&'static str, ObjectCounts> = HashMap::new();
    for (name, value) in stats {
        record_object(&mut counts, name, *value);
    }
    let (blocks, items) = object_rows(&counts, context);
    StatsData {
        general: general_rows(&values),
        blocks,
        items,
        mobs: mob_rows(&values),
    }
}

/// Files a `stat.<kind>.minecraft.<item>` counter under its item.
fn record_object(counts: &mut HashMap<&'static str, ObjectCounts>, name: &str, value: i32) {
    let Some((kind, registry)) = name
        .strip_prefix("stat.")
        .and_then(|rest| rest.split_once('.'))
    else {
        return;
    };
    let Some(item) = registry
        .strip_prefix("minecraft.")
        .and_then(|short| items::all().iter().find(|def| def.name == short))
    else {
        return;
    };
    let entry = counts.entry(item.name).or_default();
    match kind {
        "craftItem" => entry.crafted = Some(value),
        "useItem" => entry.used = Some(value),
        "mineBlock" => entry.mined = Some(value),
        "breakItem" => entry.broken = Some(value),
        _ => {}
    }
}

fn general_rows(values: &HashMap<&str, i32>) -> Vec<StatRow> {
    GENERAL
        .iter()
        .map(|&(id, label)| {
            let value = values.get(id).copied().unwrap_or(0);
            StatRow::new(label, format_stat(id, value))
        })
        .collect()
}

fn mob_rows(values: &HashMap<&str, i32>) -> Vec<MobRow> {
    let read = |prefix: &str, entity: &str| {
        values
            .get(format!("{prefix}{entity}").as_str())
            .copied()
            .unwrap_or(0)
    };
    MOBS.iter()
        .map(|&entity| MobRow {
            entity: entity.to_string(),
            kills: read("stat.killEntity.", entity),
            killed_by: read("stat.entityKilledBy.", entity),
        })
        .filter(|row| row.kills > 0 || row.killed_by > 0)
        .collect()
}

/// The Blocks and Items rows (`StatsBlock`, `StatsItem`): an item is listed
/// when any of its counters of that tab is above zero.
fn object_rows(
    counts: &HashMap<&'static str, ObjectCounts>,
    context: StatsContext<'_>,
) -> (Vec<ObjectRow>, Vec<ObjectRow>) {
    let (mut blocks, mut items) = (Vec::new(), Vec::new());
    for def in items::all() {
        let Some(count) = counts.get(def.name) else {
            continue;
        };
        if matches!(def.kind, ItemKind::Block) {
            let cells = [
                count.crafted,
                Some(count.used.unwrap_or(0)),
                Some(count.mined.unwrap_or(0)),
            ];
            if any_positive(cells) {
                blocks.push(object_row(def, cells, context));
            }
        } else {
            let broken = def.is_damageable().then(|| count.broken.unwrap_or(0));
            let cells = [broken, count.crafted, Some(count.used.unwrap_or(0))];
            if any_positive(cells) {
                items.push(object_row(def, cells, context));
            }
        }
    }
    (blocks, items)
}

fn object_row(def: &ItemDef, cells: [Option<i32>; 3], context: StatsContext<'_>) -> ObjectRow {
    let stack = ItemStack {
        item_id: i16::try_from(def.id).unwrap_or(i16::MAX),
        stack_size: 1,
        damage: 0,
        nbt: None,
    };
    let name = tooltip(&stack)
        .first()
        .map(|line| resolve_in(context.lang, &line.text))
        .unwrap_or_default();
    ObjectRow {
        id: i32::from(def.id),
        icon: slot_item(context.items, &stack),
        name,
        cells: cells.map(|cell| {
            cell.map(|raw| StatCell {
                raw,
                text: group_thousands(raw),
            })
        }),
    }
}

/// A statistic value as vanilla formats it (`StatBase`): distances in
/// cm, m or km, times from ticks, damage in tenths, other counters as integers
/// with thousands separators.
#[must_use]
pub fn format_stat(name: &str, value: i32) -> String {
    match name {
        "stat.playOneMinute" | "stat.timeSinceDeath" => format_time(value),
        "stat.damageDealt" | "stat.damageTaken" => format!("{:.2}", f64::from(value) * 0.1),
        _ if name.ends_with("OneCm") => format_distance(value),
        _ => group_thousands(value),
    }
}

/// `StatBase.timeStatType`: 20 ticks per second, the largest unit above 0.5.
fn format_time(ticks: i32) -> String {
    let seconds = f64::from(ticks) / 20.0;
    let minutes = seconds / 60.0;
    let hours = minutes / 60.0;
    let days = hours / 24.0;
    let years = days / 365.0;
    for (amount, unit) in [(years, "y"), (days, "d"), (hours, "h"), (minutes, "m")] {
        if amount > 0.5 {
            return format!("{amount:.2} {unit}");
        }
    }
    format!("{seconds:?} s")
}

/// `StatBase.distanceStatType`: the value is in centimetres.
fn format_distance(centimetres: i32) -> String {
    let metres = f64::from(centimetres) / 100.0;
    let kilometres = metres / 1000.0;
    if kilometres > 0.5 {
        format!("{kilometres:.2} km")
    } else if metres > 0.5 {
        format!("{metres:.2} m")
    } else {
        format!("{centimetres} cm")
    }
}

/// `NumberFormat.getIntegerInstance(Locale.US)`: `1,234,567`.
fn group_thousands(value: i32) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if value < 0 {
        grouped.insert(0, '-');
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dials::DialFrames;

    fn data(stats: &[(&str, i32)]) -> StatsData {
        let stats: Vec<(String, i32)> = stats.iter().map(|(n, v)| ((*n).to_string(), *v)).collect();
        let lang = Lang::builtin();
        let context = StatsContext {
            lang: &lang,
            items: ItemView::new(None, DialFrames::default()),
        };
        build(&stats, context)
    }

    #[test]
    fn the_general_list_is_complete_and_in_vanilla_order() {
        let view = data(&[("stat.jump", 7)]);
        assert_eq!(view.general.len(), GENERAL.len());
        assert_eq!(view.general[0].key, "stat.leaveGame");
        let jump = view.general.iter().find(|row| row.key == "stat.jump");
        assert_eq!(jump.map(|row| row.value.as_str()), Some("7"));
        let zero = view.general.iter().find(|row| row.key == "stat.deaths");
        assert_eq!(zero.map(|row| row.value.as_str()), Some("0"));
    }

    #[test]
    fn the_workbench_label_is_the_one_renamed_key() {
        let view = data(&[]);
        assert!(view
            .general
            .iter()
            .any(|row| row.key == "stat.workbenchInteraction"));
    }

    #[test]
    fn block_counters_make_a_row_with_the_three_columns() {
        let view = data(&[
            ("stat.mineBlock.minecraft.stone", 12),
            ("stat.useItem.minecraft.stone", 3),
        ]);
        assert_eq!(view.blocks.len(), 1);
        let row = &view.blocks[0];
        assert_eq!(row.id, 1);
        // crafted is not a statistic of stone: `-`
        assert_eq!(row.cells[0], None);
        assert_eq!(row.cells[1].as_ref().map(|c| c.raw), Some(3));
        assert_eq!(row.cells[2].as_ref().map(|c| c.raw), Some(12));
        assert!(view.items.is_empty());
    }

    #[test]
    fn item_counters_make_an_items_row_and_only_damageable_items_deplete() {
        let view = data(&[
            ("stat.useItem.minecraft.iron_pickaxe", 1_500),
            ("stat.craftItem.minecraft.stick", 4),
            ("stat.useItem.minecraft.stick", 0),
        ]);
        let ids: Vec<i32> = view.items.iter().map(|row| row.id).collect();
        assert_eq!(ids, [257, 280], "sorted by item id");
        let pickaxe = &view.items[0];
        assert_eq!(pickaxe.cells[0].as_ref().map(|c| c.raw), Some(0));
        assert_eq!(
            pickaxe.cells[2].as_ref().map(|c| c.text.as_str()),
            Some("1,500")
        );
        let stick = &view.items[1];
        assert_eq!(stick.cells[0], None, "a stick has no durability");
    }

    #[test]
    fn a_row_needs_a_positive_counter_and_a_known_item() {
        let view = data(&[
            ("stat.useItem.minecraft.stone", 0),
            ("stat.useItem.minecraft.no_such_item", 5),
            ("stat.useItem.stone", 5),
        ]);
        assert!(view.blocks.is_empty() && view.items.is_empty());
    }

    #[test]
    fn mobs_list_kills_and_deaths_by_entity_name() {
        let view = data(&[
            ("stat.killEntity.Zombie", 3),
            ("stat.entityKilledBy.Creeper", 1),
            ("stat.killEntity.Unknown", 9),
        ]);
        let mobs: Vec<(&str, i32, i32)> = view
            .mobs
            .iter()
            .map(|m| (m.entity.as_str(), m.kills, m.killed_by))
            .collect();
        assert_eq!(mobs, [("Creeper", 0, 1), ("Zombie", 3, 0)]);
    }

    #[test]
    fn counters_are_grouped_like_the_reference() {
        assert_eq!(format_stat("stat.jump", 0), "0");
        assert_eq!(format_stat("stat.jump", 999), "999");
        assert_eq!(format_stat("stat.jump", 1_234_567), "1,234,567");
        assert_eq!(format_stat("stat.deaths", -1_000), "-1,000");
    }

    #[test]
    fn distances_pick_cm_m_or_km() {
        assert_eq!(format_stat("stat.walkOneCm", 40), "40 cm");
        assert_eq!(format_stat("stat.walkOneCm", 1_234), "12.34 m");
        assert_eq!(format_stat("stat.flyOneCm", 250_000), "2.50 km");
    }

    #[test]
    fn times_pick_the_largest_unit_above_half() {
        assert_eq!(format_stat("stat.playOneMinute", 100), "5.0 s");
        assert_eq!(format_stat("stat.playOneMinute", 20 * 90), "1.50 m");
        assert_eq!(format_stat("stat.timeSinceDeath", 20 * 3600 * 3), "3.00 h");
        assert_eq!(format_stat("stat.playOneMinute", 20 * 86_400 * 2), "2.00 d");
    }

    #[test]
    fn damage_is_counted_in_tenths_of_a_heart() {
        assert_eq!(format_stat("stat.damageDealt", 25), "2.50");
    }
}
