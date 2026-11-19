// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resolves the pictures the built-in mods name ([`HudIcon`]) into what the
//! widget layer draws ([`ModIcon`]): an item icon key for armor and the held
//! item, a rectangle of `container/inventory.png` for a potion status icon.
//!
//! The mods only say "the helmet" or "Speed"; the sprites come from the assets
//! the player downloaded (spec §3), so nothing here is bundled.

use umicraft_assets::GuiAtlas;
use umicraft_builtin_mods::HudIcon;
use umicraft_game::inventory::effect_list::status_icon;
use umicraft_game::inventory::HOTBAR_START;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_ui::mod_widget::ModIcon;

use crate::resources::item_icons::{slot_item, ItemView};

/// The first window slot of the armor in the player window (the helmet).
const ARMOR_START: usize = 5;
/// The texture of the potion status icons.
const INVENTORY_TEXTURE: &str = "container/inventory";
/// The grid of status icons in `container/inventory.png`: from row 198, eight
/// icons of 18 pixels per row (the same grid as the inventory effect list).
const STATUS_GRID_V: u32 = 198;
const STATUS_COLUMNS: u32 = 8;
const STATUS_SIZE: u32 = 18;
/// The icon key the item resolver gives an item without a sprite model.
const NO_SPRITE: u64 = u64::MAX;

/// What the pictures of a frame are made from.
#[derive(Clone, Copy, Default)]
pub struct ModIcons<'a> {
    /// The 45 slots of the player window, when a session runs.
    pub slots: Option<&'a [Option<ItemStack>]>,
    /// The selected hotbar slot.
    pub selected: u8,
    /// The item resources, to turn a stack into an icon key.
    pub items: Option<ItemView<'a>>,
    /// The GUI textures, for the status icons.
    pub gui_atlas: Option<&'a GuiAtlas>,
}

impl ModIcons<'_> {
    /// The picture for `icon`; [`ModIcon::None`] when what it needs is missing
    /// (no session, an empty slot, no assets), so the row just has no picture.
    #[must_use]
    pub fn resolve(&self, icon: HudIcon) -> ModIcon {
        match icon {
            HudIcon::None => ModIcon::None,
            HudIcon::Armor(slot) => self.item(ARMOR_START + usize::from(slot)),
            HudIcon::Held => self.item(HOTBAR_START + usize::from(self.selected.min(8))),
            HudIcon::Effect(id) => self.status(id),
        }
    }

    fn item(&self, slot: usize) -> ModIcon {
        let (Some(slots), Some(items)) = (self.slots, self.items) else {
            return ModIcon::None;
        };
        let Some(stack) = slots.get(slot).and_then(Option::as_ref) else {
            return ModIcon::None;
        };
        let item = slot_item(items, stack);
        if item.key == NO_SPRITE {
            return ModIcon::None;
        }
        ModIcon::Item {
            key: item.key,
            glint: item.glint,
        }
    }

    fn status(&self, effect_id: u8) -> ModIcon {
        let sheet = self
            .gui_atlas
            .and_then(|atlas| atlas.region(INVENTORY_TEXTURE));
        let (Some(sheet), Some(index)) = (sheet, status_icon(effect_id)) else {
            return ModIcon::None;
        };
        let index = u32::from(index);
        ModIcon::Image {
            src: sheet.sub_region(
                index % STATUS_COLUMNS * STATUS_SIZE,
                STATUS_GRID_V + index / STATUS_COLUMNS * STATUS_SIZE,
                STATUS_SIZE,
                STATUS_SIZE,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_session_or_assets_nothing_is_drawn() {
        let icons = ModIcons::default();
        for icon in [HudIcon::Armor(0), HudIcon::Held, HudIcon::Effect(1)] {
            assert_eq!(icons.resolve(icon), ModIcon::None);
        }
        assert_eq!(icons.resolve(HudIcon::None), ModIcon::None);
    }

    #[test]
    fn an_item_row_without_item_resources_has_no_picture() {
        let slots = vec![None; 45];
        let icons = ModIcons {
            slots: Some(&slots),
            ..ModIcons::default()
        };
        assert_eq!(icons.resolve(HudIcon::Armor(0)), ModIcon::None);
    }
}
