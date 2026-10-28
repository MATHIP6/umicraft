// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of the inventory and container screens from a [`WindowView`].
//!
//! The layout comes from [`umicraft_ui::screens`]; this module only maps the
//! window snapshot onto it (which [`ScreenKind`], which stack sits in which
//! window slot) and issues the draw calls. It runs on the render thread and
//! never touches the game state.

use umicraft_assets::GuiAtlas;
use umicraft_game::inventory::{
    resolve_container_slot, ContainerView, SlotTarget, WindowKind, WindowView,
};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_ui::screens::{ScreenKind, ScreenLayout, SpriteRect};
use umicraft_ui::widgets::slot;
use umicraft_ui::{draw_item, FontAtlas, Rect, Ui};

use crate::item_icons::{slot_item, ItemView};

/// Number of backpack and hotbar slots after a container's own slots.
const PLAYER_PART_LEN: usize = 36;
/// Item icon edge in unscaled GUI pixels.
const ICON_SIZE: f32 = 16.0;
/// Dimming drawn behind every modal screen.
const DIMMING: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// Fill used when the GUI texture is missing.
const FALLBACK_BACKGROUND: [f32; 4] = [0.12, 0.12, 0.15, 0.92];
const WHITE: [f32; 4] = [1.0; 4];

/// Chooses the screen of an open window.
///
/// Windows that have no dedicated screen yet (brewing, enchanting, anvil,
/// beacon, villager, horse) fall back to a chest layout with enough rows for
/// their own slots, so they stay usable.
pub fn screen_kind(container: Option<&ContainerView>) -> ScreenKind {
    let Some(container) = container else {
        return ScreenKind::Player;
    };
    let rows = |len: usize| u8::try_from(len.div_ceil(9).clamp(1, 6)).unwrap_or(6);
    match container.kind {
        WindowKind::CraftingTable => ScreenKind::CraftingTable,
        WindowKind::Furnace => ScreenKind::Furnace,
        WindowKind::Dispenser | WindowKind::Dropper => ScreenKind::Dispenser,
        WindowKind::Hopper => ScreenKind::Hopper,
        _ => ScreenKind::Chest {
            rows: rows(container.slots.len()),
        },
    }
}

/// The stack shown in window slot `index` of a layout with `own_len`
/// container slots (0 for the player window).
fn stack_at(view: &WindowView, own_len: usize, index: usize) -> Option<&ItemStack> {
    let Some(container) = &view.container else {
        return view.player_slots.get(index)?.as_ref();
    };
    match resolve_container_slot(index, own_len)? {
        SlotTarget::Container(i) => container.slots.get(i)?.as_ref(),
        SlotTarget::Player(i) => view.player_slots.get(i)?.as_ref(),
    }
}

/// Draws the inventory or container screen as a modal overlay, including the
/// stack on the cursor.
pub fn draw_inventory_overlay(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    view: &WindowView,
    gui_atlas: Option<&GuiAtlas>,
    items: ItemView<'_>,
) {
    ui.draw().rect(viewport, DIMMING);
    let kind = screen_kind(view.container.as_ref());
    let layout = ScreenLayout::new(viewport, kind);
    let region = gui_atlas.and_then(|atlas| atlas.region(layout.spec.texture));

    match region {
        Some(region) => {
            let properties = view.container.as_ref().map_or(&[][..], |c| &c.properties);
            let decorations = kind.decorations(properties);
            for sprite in layout
                .background_sprites()
                .iter()
                .chain(&layout.sprites(&decorations))
            {
                draw_sprite(ui, region, sprite);
            }
        }
        None => ui.draw().rect(layout.background, FALLBACK_BACKGROUND),
    }

    let own_len = layout.slots.len().saturating_sub(PLAYER_PART_LEN);
    let own_len = if kind == ScreenKind::Player {
        0
    } else {
        own_len
    };
    let textured = region.is_some();
    let mut label = String::with_capacity(12);
    for &(index, rect) in &layout.slots {
        label.clear();
        std::fmt::Write::write_fmt(&mut label, format_args!("win{index}")).ok();
        let id = ui.id(&label);
        let item = stack_at(view, own_len, index).map(|stack| slot_item(items, stack));
        slot(ui, id, rect, atlas, item.as_ref(), textured);
    }
    draw_cursor_item(ui, atlas, &layout, view.cursor.as_ref(), items);
}

/// Draws one texture sub-rectangle.
fn draw_sprite(ui: &mut Ui, region: &umicraft_assets::GuiRegion, sprite: &SpriteRect) {
    let (x, y, w, h) = sprite.src;
    ui.draw()
        .image(region.sub_region(x, y, w, h), sprite.dst, WHITE);
}

/// Draws the stack held by the cursor centred on the pointer, like vanilla.
fn draw_cursor_item(
    ui: &mut Ui,
    atlas: &FontAtlas,
    layout: &ScreenLayout,
    cursor: Option<&ItemStack>,
    items: ItemView<'_>,
) {
    let (Some(stack), Some([x, y])) = (cursor, ui.pointer()) else {
        return;
    };
    let size = ICON_SIZE * layout.scale as f32;
    let rect = Rect::new(x - size * 0.5, y - size * 0.5, size, size);
    draw_item(ui.draw(), atlas, rect, &slot_item(items, stack));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(kind: WindowKind, slots: usize) -> ContainerView {
        ContainerView {
            window_id: 1,
            kind,
            title: String::new(),
            slots: vec![None; slots],
            properties: Vec::new(),
        }
    }

    #[test]
    fn no_container_is_the_player_screen() {
        assert_eq!(screen_kind(None), ScreenKind::Player);
    }

    #[test]
    fn known_windows_map_to_their_screens() {
        let kind = |k, n| screen_kind(Some(&container(k, n)));
        assert_eq!(kind(WindowKind::Furnace, 3), ScreenKind::Furnace);
        assert_eq!(kind(WindowKind::Dropper, 9), ScreenKind::Dispenser);
        assert_eq!(kind(WindowKind::Dispenser, 9), ScreenKind::Dispenser);
        assert_eq!(kind(WindowKind::Hopper, 5), ScreenKind::Hopper);
        assert_eq!(
            kind(WindowKind::CraftingTable, 10),
            ScreenKind::CraftingTable
        );
        assert_eq!(kind(WindowKind::Chest, 27), ScreenKind::Chest { rows: 3 });
        assert_eq!(kind(WindowKind::Chest, 54), ScreenKind::Chest { rows: 6 });
    }

    #[test]
    fn unsupported_windows_fall_back_to_a_chest_layout() {
        let kind = |k, n| screen_kind(Some(&container(k, n)));
        assert_eq!(
            kind(WindowKind::BrewingStand, 4),
            ScreenKind::Chest { rows: 1 }
        );
        assert_eq!(kind(WindowKind::Horse, 17), ScreenKind::Chest { rows: 2 });
        assert_eq!(kind(WindowKind::Horse, 0), ScreenKind::Chest { rows: 1 });
    }

    fn stack(id: i16) -> ItemStack {
        ItemStack {
            item_id: id,
            stack_size: 1,
            damage: 0,
            nbt: None,
        }
    }

    #[test]
    fn container_windows_resolve_player_slots_after_their_own() {
        let mut view = WindowView {
            player_slots: std::array::from_fn(|_| None),
            container: Some(container(WindowKind::Furnace, 3)),
            cursor: None,
            selected_slot: 0,
        };
        view.player_slots[9] = Some(stack(1));
        view.player_slots[36] = Some(stack(2));
        if let Some(c) = view.container.as_mut() {
            c.slots[2] = Some(stack(3));
        }
        assert_eq!(stack_at(&view, 3, 2).map(|s| s.item_id), Some(3));
        assert_eq!(stack_at(&view, 3, 3).map(|s| s.item_id), Some(1));
        assert_eq!(stack_at(&view, 3, 30).map(|s| s.item_id), Some(2));
        assert!(stack_at(&view, 3, 39).is_none());
    }

    #[test]
    fn the_player_window_indexes_player_slots_directly() {
        let mut view = WindowView {
            player_slots: std::array::from_fn(|_| None),
            container: None,
            cursor: None,
            selected_slot: 0,
        };
        view.player_slots[44] = Some(stack(5));
        assert_eq!(stack_at(&view, 0, 44).map(|s| s.item_id), Some(5));
    }
}
