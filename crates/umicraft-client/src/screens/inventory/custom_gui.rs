// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which picture a container screen is drawn with when a pack retextures it
//! (Custom GUIs, F6 phase 7; `resources::custom_guis`).
//!
//! The reference matches a rule against the container block (its position,
//! its tile entity) or the entity it was opened on. The client does not keep
//! that: it knows the window (kind, title, slot count, properties) and where
//! the player stands. So the height is the player's, the biome and the facts
//! that only the world knows (a trapped or ender chest, a villager's
//! profession, a horse's species) stay unknown, and a rule that asks for
//! them does not match.

use umicraft_game::inventory::{ContainerView, WindowKind, WindowView};
use umicraft_packext::guis::{Container, GuiContext};
use umicraft_ui::screens::ScreenKind;

use super::draw::screen_kind;
use crate::resources::custom_guis::CustomGuis;

/// Slots of a double chest.
const DOUBLE_CHEST_SLOTS: usize = 54;

/// The atlas region that replaces the picture of the open screen, `None` to
/// keep the vanilla picture. `height` is the block height of the player.
pub(crate) fn custom_region(guis: &CustomGuis, view: &WindowView, height: i32) -> Option<String> {
    if guis.is_empty() {
        return None;
    }
    let kind = screen_kind(view.container.as_ref());
    let title = view.container.as_ref().map(|container| {
        umicraft_game::chat::unformatted_text(&container.title, crate::language::current().table())
    });
    let context = GuiContext {
        name: title.as_deref(),
        height: Some(height),
        ..context_of(container_of(kind)?, view.container.as_ref())
    };
    guis.region_for(&context, kind.spec().texture)
        .map(str::to_string)
}

/// The container and the facts the window itself carries.
fn context_of<'a>(kind: Container, container: Option<&ContainerView>) -> GuiContext<'a> {
    let mut context = GuiContext::of(kind);
    let Some(container) = container else {
        return context;
    };
    match container.kind {
        WindowKind::Chest => context.large = Some(container.slots.len() == DOUBLE_CHEST_SLOTS),
        WindowKind::Dispenser => context.variant = Some("dispenser"),
        WindowKind::Dropper => context.variant = Some("dropper"),
        WindowKind::Beacon => {
            context.levels = container
                .properties
                .first()
                .map(|levels| i32::from(*levels));
        }
        _ => {}
    }
    context
}

fn container_of(kind: ScreenKind) -> Option<Container> {
    Some(match kind {
        ScreenKind::Player => Container::Inventory,
        ScreenKind::Chest { .. } => Container::Chest,
        ScreenKind::CraftingTable => Container::Crafting,
        ScreenKind::Furnace => Container::Furnace,
        ScreenKind::Dispenser => Container::Dispenser,
        ScreenKind::Hopper => Container::Hopper,
        ScreenKind::BrewingStand => Container::BrewingStand,
        ScreenKind::EnchantingTable => Container::Enchantment,
        ScreenKind::Anvil => Container::Anvil,
        ScreenKind::Beacon => Container::Beacon,
        ScreenKind::Villager => Container::Villager,
        ScreenKind::Horse { .. } => Container::Horse,
        // A screen the format has no container for.
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use umicraft_assets::{PackFiles, ResourcePack};

    use super::*;
    use crate::resources::game_files::{test_support::png, PackFeatures};

    fn guis(rules: &[(&str, &str)]) -> CustomGuis {
        let mut files = std::collections::BTreeMap::new();
        for (name, text) in rules {
            files.insert(
                format!("assets/minecraft/optifine/gui/container/{name}.properties"),
                text.as_bytes().to_vec(),
            );
        }
        files.insert(
            "assets/minecraft/optifine/gui/container/pic.png".to_string(),
            png(256, 256),
        );
        let pack = ResourcePack::from_parts("guis", None, None, files);
        CustomGuis::scan(&PackFiles::from_packs([pack]), PackFeatures::ALL).0
    }

    fn view(kind: WindowKind, slots: usize, title: &str) -> WindowView {
        WindowView {
            player_slots: std::array::from_fn(|_| None),
            container: Some(ContainerView {
                window_id: 1,
                kind,
                title: title.to_string(),
                slots: vec![None; slots],
                properties: Vec::new(),
                trades: Vec::new(),
                selected_trade: 0,
                horse: None,
            }),
            cursor: None,
            selected_slot: 0,
        }
    }

    #[test]
    fn a_chest_rule_by_size_and_height() {
        let guis = guis(&[(
            "chest",
            "container=chest\ntexture=pic\nlarge=true\nheights=0-40\n",
        )]);
        let big = view(WindowKind::Chest, 54, "");
        assert_eq!(
            custom_region(&guis, &big, 20).as_deref(),
            Some("optifine_custom/0")
        );
        assert_eq!(custom_region(&guis, &big, 70), None, "too high");
        assert_eq!(
            custom_region(&guis, &view(WindowKind::Chest, 27, ""), 20),
            None,
            "a small chest"
        );
    }

    #[test]
    fn the_name_comes_from_the_title() {
        let guis = guis(&[("c", "container=chest\ntexture=pic\nname=Loot\n")]);
        let titled = |text: &str| view(WindowKind::Chest, 27, &format!(r#"{{"text":"{text}"}}"#));
        assert!(custom_region(&guis, &titled("Loot"), 0).is_some());
        assert!(custom_region(&guis, &titled("Other"), 0).is_none());
    }

    #[test]
    fn dispenser_and_dropper_are_variants() {
        let guis = guis(&[("d", "container=dispenser\ntexture=pic\nvariants=dropper\n")]);
        assert!(custom_region(&guis, &view(WindowKind::Dropper, 9, ""), 0).is_some());
        assert!(custom_region(&guis, &view(WindowKind::Dispenser, 9, ""), 0).is_none());
    }

    #[test]
    fn beacon_levels_come_from_the_window_properties() {
        let guis = guis(&[("b", "container=beacon\ntexture=pic\nlevels=4\n")]);
        let mut beacon = view(WindowKind::Beacon, 1, "");
        if let Some(container) = beacon.container.as_mut() {
            container.properties = vec![4, 1, 0];
        }
        assert!(custom_region(&guis, &beacon, 0).is_some());
        if let Some(container) = beacon.container.as_mut() {
            container.properties = vec![2];
        }
        assert!(custom_region(&guis, &beacon, 0).is_none());
    }

    #[test]
    fn the_player_inventory_is_the_inventory_container() {
        let guis = guis(&[("i", "container=inventory\ntexture=pic\n")]);
        let player = WindowView {
            container: None,
            ..view(WindowKind::Chest, 0, "")
        };
        assert!(custom_region(&guis, &player, 0).is_some());
        assert!(custom_region(&CustomGuis::default(), &player, 0).is_none());
    }
}
