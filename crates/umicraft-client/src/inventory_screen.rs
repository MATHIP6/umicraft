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
use umicraft_ui::screens::{
    Hotspot, Label, ScreenKind, ScreenLayout, ScreenState, Sheet, SpriteRect, TRADE_OFFER_ITEMS,
};
use umicraft_ui::widgets::slot;
use umicraft_ui::{draw_item, FontAtlas, Rect, TextStyle, Ui};

use crate::anvil_field::FieldView;
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

/// What the window thread knows that the look of a container screen depends
/// on, besides the window itself.
#[derive(Debug, Default)]
pub(crate) struct ScreenFacts {
    /// The drag split in progress.
    pub drag: Option<DragPreview>,
    /// The experience level.
    pub xp_level: i32,
    /// Whether the player is in creative mode.
    pub creative: bool,
    /// The name field of an anvil screen.
    pub anvil: Option<FieldView>,
    /// Whether the caret of the name field is in its visible half-period.
    pub caret_visible: bool,
}

/// Left edge, top edge and caret height of the anvil name text, in GUI pixels
/// (`GuiRepair`: `guiLeft + 62`, `guiTop + 24`).
const NAME_TEXT: (f32, f32, f32) = (62.0, 24.0, 9.0);
/// Colours of the name text: enabled and disabled (`GuiTextField`).
const NAME_ENABLED: u32 = 0xE0_E0_E0;
const NAME_DISABLED: u32 = 0x70_70_70;

/// Texture holding the potion status icons the beacon buttons use.
const INVENTORY_TEXTURE: &str = "container/inventory";

/// The state the dynamic sprites of the open window depend on.
pub(crate) fn screen_state<'a>(
    view: &'a WindowView,
    player: &ScreenFacts,
    hover: Option<Hotspot>,
) -> ScreenState<'a> {
    let Some(container) = &view.container else {
        return ScreenState::default();
    };
    let own = |i: usize| container.slots.get(i).and_then(Option::as_ref);
    let trades = container.trades.len();
    ScreenState {
        properties: &container.properties,
        filled: [0, 1, 2].map(|i| own(i).is_some()),
        lapis: own(1).map_or(0, |stack| u8::try_from(stack.stack_size).unwrap_or(0)),
        xp_level: player.xp_level,
        creative: player.creative,
        hover,
        trade_previous: container.selected_trade > 0,
        trade_next: container.selected_trade + 1 < trades,
    }
}

/// Draws the labels of a screen with the vanilla drop shadow.
fn draw_labels(ui: &mut Ui, atlas: &FontAtlas, layout: &ScreenLayout, labels: &[Label]) {
    let scale = layout.scale as f32;
    for label in labels {
        let width = atlas.measure(&label.text, scale);
        let x = layout.background.x + label.right as f32 * scale - width;
        let y = layout.background.y + label.y as f32 * scale;
        let color = rgb(label.color);
        let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, 1.0];
        ui.draw().text(
            [x + scale, y + scale],
            label.text.clone(),
            TextStyle::new(scale, shadow),
        );
        ui.draw()
            .text([x, y], label.text.clone(), TextStyle::new(scale, color));
    }
}

/// Draws the anvil name text and its caret.
fn draw_name_field(
    ui: &mut Ui,
    atlas: &FontAtlas,
    layout: &ScreenLayout,
    field: &FieldView,
    caret_visible: bool,
) {
    let scale = layout.scale as f32;
    let x = layout.background.x + NAME_TEXT.0 * scale;
    let y = layout.background.y + NAME_TEXT.1 * scale;
    let color = rgb(if field.enabled {
        NAME_ENABLED
    } else {
        NAME_DISABLED
    });
    ui.draw()
        .text([x, y], field.text.clone(), TextStyle::new(scale, color));
    if field.enabled && caret_visible {
        let before: String = field.text.chars().take(field.caret).collect();
        let caret_x = x + atlas.measure(&before, scale);
        let caret = Rect::new(caret_x, y - scale, scale, NAME_TEXT.2 * scale);
        ui.draw().rect(caret, color);
    }
}

/// `0xRRGGBB` as an opaque colour.
fn rgb(value: u32) -> [f32; 4] {
    let channel = |shift: u32| ((value >> shift) & 0xFF) as f32 / 255.0;
    [channel(16), channel(8), channel(0), 1.0]
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
    facts: &ScreenFacts,
) -> Option<Spot> {
    let drag = facts.drag.as_ref();
    ui.draw().rect(viewport, DIMMING);
    let kind = screen_kind(view.container.as_ref());
    let layout = ScreenLayout::new(viewport, kind);
    let region = gui_atlas.and_then(|atlas| atlas.region(layout.spec.texture));
    let hover = ui.pointer().and_then(|[x, y]| layout.hotspot_at((x, y)));
    let dynamic = kind.dynamic(&screen_state(view, facts, hover));

    match region {
        Some(region) => {
            let inventory = gui_atlas.and_then(|atlas| atlas.region(INVENTORY_TEXTURE));
            let sprites = layout.sprites(&dynamic.decorations);
            for sprite in layout.background_sprites().iter().chain(&sprites) {
                let sheet = match sprite.sheet {
                    Sheet::Screen => Some(region),
                    Sheet::Inventory => inventory,
                };
                if let Some(sheet) = sheet {
                    draw_sprite(ui, sheet, sprite);
                }
            }
        }
        None => ui.draw().rect(layout.background, FALLBACK_BACKGROUND),
    }
    draw_labels(ui, atlas, &layout, &dynamic.labels);
    if let Some(field) = &facts.anvil {
        draw_name_field(ui, atlas, &layout, field, facts.caret_visible);
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
    let offer = draw_trade_offer(ui, atlas, &layout, view, items);
    let cursor = drag.map_or(view.cursor.as_ref(), |d| d.cursor.as_ref());
    draw_cursor_item(ui, atlas, layout.scale, cursor, items);
    if view.cursor.is_none() {
        let hovered = hovered_stack(ui, &layout, view, own_len).or(offer);
        draw_hovered_tooltip(ui, atlas, viewport, &layout, hovered);
    }
    spot
}

/// Draws the villager offer on display: what it wants and what it gives
/// (`GuiMerchant.drawScreen`). Returns the stack of the offer under the
/// pointer, whose tooltip the caller shows.
fn draw_trade_offer<'a>(
    ui: &mut Ui,
    atlas: &FontAtlas,
    layout: &ScreenLayout,
    view: &'a WindowView,
    items: ItemView<'_>,
) -> Option<&'a ItemStack> {
    let container = view.container.as_ref()?;
    let trade = container.trades.get(container.selected_trade)?;
    let shown = [
        Some(&trade.buy),
        trade.buy_second.as_ref(),
        Some(&trade.sell),
    ];
    let pointer = ui.pointer();
    let scale = layout.scale as f32;
    let mut hovered = None;
    for (stack, &(x, y)) in shown.into_iter().zip(&TRADE_OFFER_ITEMS) {
        let Some(stack) = stack else {
            continue;
        };
        let cell = Rect::new(
            layout.background.x + x as f32 * scale,
            layout.background.y + y as f32 * scale,
            ICON_SIZE * scale,
            ICON_SIZE * scale,
        );
        draw_item(ui.draw(), atlas, cell, &slot_item(items, stack));
        if pointer.is_some_and(|point| cell.contains(point)) {
            hovered = Some(stack);
        }
    }
    hovered
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
            trades: Vec::new(),
            selected_trade: 0,
        }
    }

    fn view_of(container: ContainerView) -> WindowView {
        WindowView {
            player_slots: std::array::from_fn(|_| None),
            container: Some(container),
            cursor: None,
            selected_slot: 0,
        }
    }

    #[test]
    fn the_screen_state_reads_the_enchanting_slots() {
        let mut table = container(WindowKind::EnchantingTable, 2);
        table.slots[1] = Some(stack(351));
        table.slots[1].as_mut().unwrap().stack_size = 5;
        table.properties = vec![10, 20, 30];
        let view = view_of(table);
        let facts = ScreenFacts {
            xp_level: 25,
            ..ScreenFacts::default()
        };
        let state = screen_state(&view, &facts, Some(Hotspot::Enchant(1)));
        assert_eq!(state.filled, [false, true, false]);
        assert_eq!(state.lapis, 5);
        assert_eq!(
            (state.xp_level, state.hover),
            (25, Some(Hotspot::Enchant(1)))
        );
        assert_eq!(state.properties, [10, 20, 30]);
    }

    #[test]
    fn villager_arrows_follow_the_selected_offer() {
        let mut villager = container(WindowKind::Villager, 3);
        let trade = |n: i8| umicraft_game::inventory::payload::Trade {
            buy: ItemStack {
                stack_size: n,
                ..stack(1)
            },
            buy_second: None,
            sell: stack(2),
            disabled: false,
            uses: 0,
            max_uses: 7,
        };
        villager.trades = vec![trade(1), trade(2), trade(3)];
        villager.selected_trade = 1;
        let middle = view_of(villager.clone());
        let state = screen_state(&middle, &ScreenFacts::default(), None);
        assert!(state.trade_previous && state.trade_next);
        villager.selected_trade = 2;
        let last = view_of(villager);
        let state = screen_state(&last, &ScreenFacts::default(), None);
        assert!(state.trade_previous && !state.trade_next);
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
