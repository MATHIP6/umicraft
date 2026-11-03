// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of the inventory and container screens from a [`WindowView`].
//!
//! The layout comes from [`umicraft_ui::screens`]; this module only maps the
//! window snapshot onto it (which [`ScreenKind`], which stack sits in which
//! window slot) and issues the draw calls. It runs on the render thread and
//! never touches the game state.

use umicraft_assets::GuiAtlas;
use umicraft_game::inventory::{
    resolve_container_slot, ContainerView, DragPreview, SlotTarget, WindowKind, WindowView,
};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::PORTRAIT_KEY;
use umicraft_ui::screens::{Decoration, ScreenKind, ScreenLayout, SpriteRect};
use umicraft_ui::widgets::slot;
use umicraft_ui::{draw_item, FontAtlas, Rect, Ui};

use crate::inventory_input::Context;
use crate::item_icons::{slot_item, ItemView};
use crate::portrait::{self, Spot};
use crate::tooltip_draw::draw_tooltip;

/// Number of backpack and hotbar slots after a container's own slots.
const PLAYER_PART_LEN: usize = 36;
/// Item icon edge in unscaled GUI pixels.
const ICON_SIZE: f32 = 16.0;
/// Dimming drawn behind every modal screen.
pub(crate) const DIMMING: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// Fill used when the GUI texture is missing.
pub(crate) const FALLBACK_BACKGROUND: [f32; 4] = [0.12, 0.12, 0.15, 0.92];
pub(crate) const WHITE: [f32; 4] = [1.0; 4];
/// Tint over the slots of a drag preview (vanilla 0x80FFFFFF).
const DRAG_HIGHLIGHT: [f32; 4] = [1.0, 1.0, 1.0, 0.5];

/// Chooses the screen of an open window.
///
/// A window kind whose screen is not drawn yet falls back to a chest layout
/// with enough rows for its own slots, so it stays usable.
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
        WindowKind::BrewingStand => ScreenKind::BrewingStand,
        WindowKind::EnchantingTable => ScreenKind::EnchantingTable,
        WindowKind::Anvil => ScreenKind::Anvil,
        WindowKind::Beacon => ScreenKind::Beacon,
        WindowKind::Villager => ScreenKind::Villager,
        WindowKind::Horse => ScreenKind::Horse {
            own_slots: u8::try_from(container.slots.len()).unwrap_or(u8::MAX),
        },
        WindowKind::Chest => ScreenKind::Chest {
            rows: rows(container.slots.len()),
        },
    }
}

/// The creative game mode id of `Change Game State`.
pub(crate) const CREATIVE_GAMEMODE: u8 = 1;

/// The layout of the open screen in `viewport` and what the input layer needs
/// to know about the window (`gamemode` is the wire id of the game mode).
pub fn screen_input(viewport: Rect, view: &WindowView, gamemode: u8) -> (ScreenLayout, Context) {
    let layout = ScreenLayout::new(viewport, screen_kind(view.container.as_ref()));
    let context = Context {
        cursor_has_item: view.cursor.is_some(),
        creative: gamemode == CREATIVE_GAMEMODE,
    };
    (layout, context)
}

/// The dynamic sprites of `kind` for the open window.
fn screen_decorations(view: &WindowView, kind: ScreenKind, properties: &[i16]) -> Vec<Decoration> {
    match (kind, &view.container) {
        (ScreenKind::Anvil, Some(container)) => {
            let filled = |i: usize| container.slots.get(i).is_some_and(Option::is_some);
            ScreenKind::anvil_decorations(filled(0), filled(1), filled(2))
        }
        _ => kind.decorations(properties),
    }
}

/// The number of slots the container adds in front of the player part.
fn own_slot_count(layout: &ScreenLayout, kind: ScreenKind) -> usize {
    if kind == ScreenKind::Player {
        0
    } else {
        layout.slots.len().saturating_sub(PLAYER_PART_LEN)
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
/// stack on the cursor. Returns where the player model of the survival
/// inventory goes: the caller draws it into the portrait block of the icon
/// atlas before rendering the frame.
pub(crate) fn draw_inventory_overlay(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    view: &WindowView,
    gui_atlas: Option<&GuiAtlas>,
    items: ItemView<'_>,
    drag: Option<&DragPreview>,
) -> Option<Spot> {
    ui.draw().rect(viewport, DIMMING);
    let kind = screen_kind(view.container.as_ref());
    let layout = ScreenLayout::new(viewport, kind);
    let region = gui_atlas.and_then(|atlas| atlas.region(layout.spec.texture));

    match region {
        Some(region) => {
            let properties = view.container.as_ref().map_or(&[][..], |c| &c.properties);
            let decorations = screen_decorations(view, kind, properties);
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
    let spot = (kind == ScreenKind::Player).then_some(Spot {
        origin: (layout.background.x, layout.background.y),
        gui_scale: layout.scale,
        placement: portrait::INVENTORY,
    });
    if let Some(spot) = &spot {
        ui.draw().item(PORTRAIT_KEY, spot.rect(), false);
    }

    let own_len = own_slot_count(&layout, kind);
    let textured = region.is_some();
    let mut label = String::with_capacity(12);
    for &(index, rect) in &layout.slots {
        label.clear();
        std::fmt::Write::write_fmt(&mut label, format_args!("win{index}")).ok();
        let id = ui.id(&label);
        let previewed = drag.and_then(|d| d.slots.iter().find(|(i, _)| *i == index));
        let stack = previewed
            .map(|(_, s)| s)
            .or_else(|| stack_at(view, own_len, index));
        let item = stack.map(|stack| slot_item(items, stack));
        slot(ui, id, rect, atlas, item.as_ref(), textured);
        if previewed.is_some() {
            ui.draw().rect(rect, DRAG_HIGHLIGHT);
        }
    }
    let cursor = drag.map_or(view.cursor.as_ref(), |d| d.cursor.as_ref());
    draw_cursor_item(ui, atlas, layout.scale, cursor, items);
    if view.cursor.is_none() {
        draw_hovered_tooltip(
            ui,
            atlas,
            viewport,
            &layout,
            hovered_stack(ui, &layout, view, own_len),
        );
    }
    spot
}

/// The stack under the pointer, if any.
fn hovered_stack<'a>(
    ui: &Ui,
    layout: &ScreenLayout,
    view: &'a WindowView,
    own_len: usize,
) -> Option<&'a ItemStack> {
    let [x, y] = ui.pointer()?;
    stack_at(view, own_len, layout.slot_at((x, y))?)
}

/// Draws the tooltip of `stack` next to the pointer.
fn draw_hovered_tooltip(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    layout: &ScreenLayout,
    stack: Option<&ItemStack>,
) {
    let (Some(stack), Some(pointer)) = (stack, ui.pointer()) else {
        return;
    };
    let lines = umicraft_game::inventory::tooltip::tooltip(stack);
    draw_tooltip(ui, atlas, viewport, layout.scale as f32, pointer, &lines);
}

/// Draws one texture sub-rectangle.
fn draw_sprite(ui: &mut Ui, region: &umicraft_assets::GuiRegion, sprite: &SpriteRect) {
    let (x, y, w, h) = sprite.src;
    ui.draw()
        .image(region.sub_region(x, y, w, h), sprite.dst, WHITE);
}

/// Draws the stack held by the cursor centred on the pointer, like vanilla.
pub(crate) fn draw_cursor_item(
    ui: &mut Ui,
    atlas: &FontAtlas,
    scale: u32,
    cursor: Option<&ItemStack>,
    items: ItemView<'_>,
) {
    let (Some(stack), Some([x, y])) = (cursor, ui.pointer()) else {
        return;
    };
    let size = ICON_SIZE * scale as f32;
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
    fn every_window_kind_has_its_own_screen() {
        let kind = |k, n| screen_kind(Some(&container(k, n)));
        assert_eq!(kind(WindowKind::BrewingStand, 4), ScreenKind::BrewingStand);
        assert_eq!(
            kind(WindowKind::EnchantingTable, 2),
            ScreenKind::EnchantingTable
        );
        assert_eq!(kind(WindowKind::Anvil, 3), ScreenKind::Anvil);
        assert_eq!(kind(WindowKind::Beacon, 1), ScreenKind::Beacon);
        assert_eq!(kind(WindowKind::Villager, 3), ScreenKind::Villager);
        assert_eq!(
            kind(WindowKind::Horse, 17),
            ScreenKind::Horse { own_slots: 17 }
        );
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
