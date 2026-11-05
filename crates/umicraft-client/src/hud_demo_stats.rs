// SPDX-License-Identifier: MIT OR Apache-2.0

//! The statistics and achievements scenes of `--hud-demo`: faked server
//! answers and the item icons from the user's own jar.
//!
//! The demo opens no socket, so the numbers below stand in for a `Statistics`
//! (0x37) packet. The item icons need the baked item models and the block
//! atlas on the renderer; without a jar the icons stay empty.

use std::sync::Arc;

use umicraft_render::Renderer;

use crate::item_icons::ItemResources;
use crate::model_resources::ModelResources;

/// The views that show item icons and so need the item resources.
pub(crate) fn needs_items(view: &str) -> bool {
    matches!(
        view,
        "stats" | "stats-blocks" | "stats-items" | "achievements"
    )
}

/// A believable early-game `Statistics` answer.
pub(crate) fn sample_statistics() -> Vec<(String, i32)> {
    let numbers: &[(&str, i32)] = &[
        ("stat.leaveGame", 3),
        ("stat.playOneMinute", 20 * 60 * 95),
        ("stat.timeSinceDeath", 20 * 60 * 12),
        ("stat.walkOneCm", 1_523_400),
        ("stat.sprintOneCm", 842_000),
        ("stat.swimOneCm", 9_100),
        ("stat.fallOneCm", 41_200),
        ("stat.jump", 1_480),
        ("stat.damageDealt", 3_120),
        ("stat.damageTaken", 1_450),
        ("stat.deaths", 2),
        ("stat.mobKills", 31),
        ("stat.animalsBred", 4),
        ("stat.fishCaught", 6),
        ("stat.chestOpened", 18),
        ("stat.craftItem.minecraft.planks", 64),
        ("stat.craftItem.minecraft.stick", 96),
        ("stat.craftItem.minecraft.crafting_table", 2),
        ("stat.craftItem.minecraft.furnace", 1),
        ("stat.craftItem.minecraft.stone_pickaxe", 1),
        ("stat.craftItem.minecraft.iron_pickaxe", 1),
        ("stat.craftItem.minecraft.torch", 128),
        ("stat.useItem.minecraft.iron_pickaxe", 412),
        ("stat.useItem.minecraft.stone_pickaxe", 233),
        ("stat.useItem.minecraft.torch", 97),
        ("stat.useItem.minecraft.bread", 12),
        ("stat.breakItem.minecraft.stone_pickaxe", 1),
        ("stat.useItem.minecraft.cobblestone", 1_206),
        ("stat.useItem.minecraft.planks", 320),
        ("stat.useItem.minecraft.furnace", 3),
        ("stat.mineBlock.minecraft.stone", 2_413),
        ("stat.mineBlock.minecraft.dirt", 612),
        ("stat.mineBlock.minecraft.log", 85),
        ("stat.mineBlock.minecraft.iron_ore", 41),
        ("stat.mineBlock.minecraft.coal_ore", 66),
        ("stat.mineBlock.minecraft.diamond_ore", 3),
        ("stat.killEntity.Zombie", 14),
        ("stat.killEntity.Skeleton", 7),
        ("stat.killEntity.Creeper", 5),
        ("stat.killEntity.Spider", 3),
        ("stat.killEntity.Cow", 9),
        ("stat.entityKilledBy.Creeper", 1),
        ("stat.entityKilledBy.Skeleton", 1),
        ("achievement.openInventory", 1),
        ("achievement.mineWood", 1),
        ("achievement.buildWorkBench", 1),
        ("achievement.buildPickaxe", 1),
        ("achievement.buildFurnace", 1),
        ("achievement.acquireIron", 1),
        ("achievement.buildHoe", 1),
        ("achievement.diamonds", 1),
    ];
    numbers
        .iter()
        .map(|(name, value)| ((*name).to_string(), *value))
        .collect()
}

/// Loads the item models of the user's jar and gives `renderer` the block
/// atlas their icons are drawn from; `None` without a jar.
pub(crate) fn load_items(renderer: &mut Renderer) -> Option<Arc<ItemResources>> {
    let resources = ModelResources::load().ok()?;
    let atlas = resources.atlas();
    renderer
        .set_block_atlas(
            atlas.width(),
            atlas.height(),
            atlas.layers(),
            atlas.pixels(),
        )
        .ok()?;
    Some(resources.items())
}
