// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of the creative inventory (MCP `GuiContainerCreative`:
//! `drawGuiContainerBackgroundLayer`, `drawGuiContainerForegroundLayer`,
//! `func_147051_a`, `drawScreen`).
//!
//! The window is built from three textures of the GUI atlas, all 256 by 256:
//! `container/creative_inventory/tabs` (tab buttons and scroll handles) and
//! the background of the selected tab (`tab_items`, `tab_item_search` or
//! `tab_inventory`). Positions come from the pure game layer
//! ([`CreativeTab::draw_origin`], [`CreativeSlot::position`]) and
//! [`CreativeLayout`] scales them to the display. The slots hold the grid from
//! the [`CreativeView`] snapshot and the player's slots from the
//! [`WindowView`].
//!
//! Not drawn: the player model of the inventory tab (`drawEntityOnScreen` at
//! 43, 45 needs the 3D player render, the area stays empty), the search box
//! caret and the tab name line vanilla adds to item tooltips on the search
//! tab.

use umicraft_assets::{GuiAtlas, GuiRegion};
use umicraft_game::inventory::creative::scroll::{
    HANDLE_HEIGHT, HANDLE_U_ACTIVE, SCROLLBAR_X, SCROLLBAR_Y,
};
use umicraft_game::inventory::creative::tabs::{
    TAB_HEIGHT, TAB_WIDTH, WINDOW_HEIGHT, WINDOW_WIDTH,
};
use umicraft_game::inventory::creative::{visible_slots, CreativeSlot, CreativeTab};
use umicraft_game::inventory::tooltip::{tooltip, TooltipColor, TooltipLine, TooltipText};
use umicraft_game::inventory::{WindowView, HOTBAR_START};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_ui::widgets::slot;
use umicraft_ui::{draw_item, FontAtlas, Rect, TextStyle, Ui};

use crate::creative_layout::CreativeLayout;
use crate::creative_view::CreativeView;
use crate::inventory_screen::{draw_cursor_item, DIMMING, FALLBACK_BACKGROUND, WHITE};
use crate::item_icons::{slot_item, ItemView};
use crate::tooltip_draw::{draw_tooltip, resolve, text_color};

/// Atlas name of the tab buttons and scroll handles.
const TABS_TEXTURE: &str = "container/creative_inventory/tabs";
/// Prefix of the background atlas names.
const BACKGROUND_PREFIX: &str = "container/creative_inventory/";
/// The window title colour (`4210752`).
const TITLE_COLOR: u32 = 0x40_40_40;
/// The search text colour (`setTextColor(16777215)`).
const SEARCH_COLOR: u32 = 0xFF_FF_FF;
/// Where the title is drawn (`drawString(..., 8, 6, ...)`).
const TITLE_POSITION: (i32, i32) = (8, 6);
/// Where the search text starts (`new GuiTextField(..., guiLeft + 82, guiTop + 6, ...)`).
const SEARCH_POSITION: (i32, i32) = (82, 6);
/// Offset of a tab's icon from the button (`l + 6`, `i1 + 8 + (top ? 1 : -1)`).
const ICON_OFFSET_X: i32 = 6;
const ICON_OFFSET_Y: i32 = 8;
/// Side of an item area.
const ITEM_SIZE: i32 = 16;
/// Width of the scroll handle slice.
const HANDLE_WIDTH: i32 = 12;
/// Distance of the tab hover box from the button (`isPointInRegion(j + 3, k + 3, 23, 27)`).
const TAB_HOVER_INSET: i32 = 3;
const TAB_HOVER_SIZE: (i32, i32) = (23, 27);
/// Language key of the hover text of the delete slot.
const BIN_SLOT_KEY: &str = "inventory.binSlot";

/// What the creative screen draws, gathered from the two snapshots.
pub struct CreativeScene<'a> {
    /// The player's slots and the cursor stack.
    pub window: &'a WindowView,
    /// The tab, scroll position, search text and grid.
    pub creative: &'a CreativeView,
    /// The GUI texture atlas, when the textures are installed.
    pub gui_atlas: Option<&'a GuiAtlas>,
    /// The item icons.
    pub items: ItemView<'a>,
}

/// Where the tab button of `tab` is drawn, as scaled pixels relative to the
/// window.
fn tab_rect(tab: CreativeTab) -> (i32, i32, i32, i32) {
    let (x, y) = tab.draw_origin();
    (x, y, TAB_WIDTH, TAB_HEIGHT)
}

/// Where the icon of `tab` is drawn (`func_147051_a`: one pixel lower in the
/// top row, one higher in the bottom row).
fn tab_icon_position(tab: CreativeTab) -> (i32, i32) {
    let (x, y) = tab.draw_origin();
    let nudge = if tab.in_top_row() { 1 } else { -1 };
    (x + ICON_OFFSET_X, y + ICON_OFFSET_Y + nudge)
}

/// The tab whose hover text a point shows (`renderCreativeInventoryHoveringText`).
fn hovered_tab(x: i32, y: i32) -> Option<CreativeTab> {
    CreativeTab::ALL.into_iter().find(|tab| {
        let (left, top) = tab.hit_origin();
        let (left, top) = (left + TAB_HOVER_INSET, top + TAB_HOVER_INSET);
        (left..left + TAB_HOVER_SIZE.0).contains(&x) && (top..top + TAB_HOVER_SIZE.1).contains(&y)
    })
}

/// Whether a point is over the delete slot's 16 by 16 area
/// (`isPointInRegion(slot.x, slot.y, 16, 16)`, without the slot margin).
fn over_bin(x: i32, y: i32) -> bool {
    CreativeSlot::Trash.position().is_some_and(|(left, top)| {
        (left..left + ITEM_SIZE).contains(&x) && (top..top + ITEM_SIZE).contains(&y)
    })
}

/// The `(u, v)` of the scroll handle in `tabs.png`: the second variant is the
/// greyed one drawn when nothing can scroll.
fn handle_origin(needs_scrollbar: bool) -> (i32, i32) {
    (
        HANDLE_U_ACTIVE + if needs_scrollbar { 0 } else { HANDLE_WIDTH },
        0,
    )
}

/// The stack shown in a slot of the window.
fn stack_in<'a>(scene: &'a CreativeScene<'_>, slot: CreativeSlot) -> Option<&'a ItemStack> {
    match slot {
        CreativeSlot::Grid(cell) => scene.creative.grid.get(cell)?.as_ref(),
        CreativeSlot::Hotbar(index) => scene
            .window
            .player_slots
            .get(HOTBAR_START + index)?
            .as_ref(),
        CreativeSlot::Player(index) => scene.window.player_slots.get(index)?.as_ref(),
        CreativeSlot::Trash => None,
    }
}

/// Draws a slice of a texture region onto a window-relative rectangle.
fn draw_slice(ui: &mut Ui, region: &GuiRegion, source: (i32, i32, i32, i32), target: Rect) {
    let (u, v, width, height) = source;
    let to_pixels = |value: i32| u32::try_from(value).unwrap_or(0);
    let uv = region.sub_region(
        to_pixels(u),
        to_pixels(v),
        to_pixels(width),
        to_pixels(height),
    );
    ui.draw().image(uv, target, WHITE);
}

/// Draws one tab button with its icon.
fn draw_tab(
    ui: &mut Ui,
    atlas: &FontAtlas,
    layout: &CreativeLayout,
    scene: &CreativeScene<'_>,
    tabs: Option<&GuiRegion>,
    tab: CreativeTab,
) {
    let selected = tab == scene.creative.tab;
    if let Some(region) = tabs {
        let (u, v) = tab.texture_origin(selected);
        let (x, y, width, height) = tab_rect(tab);
        draw_slice(
            ui,
            region,
            (u, v, width, height),
            layout.rect(x, y, width, height),
        );
    }
    let (x, y) = tab_icon_position(tab);
    let (item_id, damage) = tab.icon();
    let icon = ItemStack {
        item_id,
        stack_size: 1,
        damage,
        nbt: None,
    };
    let rect = layout.rect(x, y, ITEM_SIZE, ITEM_SIZE);
    draw_item(ui.draw(), atlas, rect, &slot_item(scene.items, &icon));
}

/// Draws the window background of the selected tab, or a flat fill while the
/// texture is missing; returns whether the texture was drawn.
fn draw_background(ui: &mut Ui, layout: &CreativeLayout, scene: &CreativeScene<'_>) -> bool {
    let name = format!(
        "{BACKGROUND_PREFIX}{}",
        scene.creative.tab.background_texture()
    );
    let target = layout.rect(0, 0, WINDOW_WIDTH, WINDOW_HEIGHT);
    match scene.gui_atlas.and_then(|atlas| atlas.region(&name)) {
        Some(region) => {
            draw_slice(ui, region, (0, 0, WINDOW_WIDTH, WINDOW_HEIGHT), target);
            true
        }
        None => {
            ui.draw().rect(target, FALLBACK_BACKGROUND);
            false
        }
    }
}

/// Draws the scroll handle of tabs that have a scrollbar.
fn draw_handle(
    ui: &mut Ui,
    layout: &CreativeLayout,
    scene: &CreativeScene<'_>,
    tabs: Option<&GuiRegion>,
) {
    let Some(region) = tabs.filter(|_| scene.creative.tab.has_scrollbar()) else {
        return;
    };
    let (u, v) = handle_origin(scene.creative.needs_scrollbar);
    let target = layout.rect(
        SCROLLBAR_X,
        SCROLLBAR_Y + scene.creative.handle_offset,
        HANDLE_WIDTH,
        HANDLE_HEIGHT,
    );
    draw_slice(ui, region, (u, v, HANDLE_WIDTH, HANDLE_HEIGHT), target);
}

/// Draws the slots of the selected tab with their stacks.
fn draw_slots(
    ui: &mut Ui,
    atlas: &FontAtlas,
    layout: &CreativeLayout,
    scene: &CreativeScene<'_>,
    textured: bool,
) {
    let mut label = String::with_capacity(16);
    for (index, slot_id) in visible_slots(scene.creative.tab).into_iter().enumerate() {
        let Some((x, y)) = slot_id.position() else {
            continue;
        };
        label.clear();
        std::fmt::Write::write_fmt(&mut label, format_args!("creative{index}")).ok();
        let id = ui.id(&label);
        let item = stack_in(scene, slot_id).map(|stack| slot_item(scene.items, stack));
        slot(
            ui,
            id,
            layout.rect(x, y, ITEM_SIZE, ITEM_SIZE),
            atlas,
            item.as_ref(),
            textured,
        );
    }
}

/// Draws a line of text at a window-relative position, with an optional
/// shadow (`drawString` against `drawStringWithShadow`).
fn draw_text(
    ui: &mut Ui,
    layout: &CreativeLayout,
    at: (i32, i32),
    text: String,
    rgb: u32,
    shadow: bool,
) {
    let scale = layout.scale() as f32;
    let [x, y] = layout.position(at.0, at.1);
    if shadow {
        let dark = text_color(rgb);
        let style = TextStyle::new(scale, [dark[0] * 0.25, dark[1] * 0.25, dark[2] * 0.25, 1.0]);
        ui.draw().text([x + scale, y + scale], text.clone(), style);
    }
    ui.draw()
        .text([x, y], text, TextStyle::new(scale, text_color(rgb)));
}

/// The language-key text of a one-line tooltip.
fn key_line(key: &str) -> TooltipLine {
    TooltipLine {
        text: TooltipText::Translation {
            key: key.to_owned(),
            args: Vec::new(),
        },
        color: TooltipColor::White,
        italic: false,
    }
}

/// The tooltip under the pointer: a tab name, the delete slot or the hovered
/// item (the item only while the cursor is empty, like `GuiContainer`).
fn hover_lines(scene: &CreativeScene<'_>, point: (i32, i32)) -> Vec<TooltipLine> {
    if let Some(tab) = hovered_tab(point.0, point.1) {
        return vec![key_line(&tab.translation_key())];
    }
    if scene.creative.tab.shows_player_inventory() && over_bin(point.0, point.1) {
        return vec![key_line(BIN_SLOT_KEY)];
    }
    if scene.window.cursor.is_some() {
        return Vec::new();
    }
    umicraft_game::inventory::creative::slot_at(scene.creative.tab, point.0, point.1)
        .and_then(|slot| stack_in(scene, slot))
        .map(tooltip)
        .unwrap_or_default()
}

/// Draws the creative inventory as a modal overlay: the window, its slots,
/// the stack on the cursor and the tooltip under the pointer.
pub fn draw_creative_overlay(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    layout: &CreativeLayout,
    scene: &CreativeScene<'_>,
) {
    ui.draw().rect(viewport, DIMMING);
    let tabs = scene.gui_atlas.and_then(|atlas| atlas.region(TABS_TEXTURE));
    let selected = scene.creative.tab;
    for tab in CreativeTab::ALL.into_iter().filter(|tab| *tab != selected) {
        draw_tab(ui, atlas, layout, scene, tabs, tab);
    }
    let textured = draw_background(ui, layout, scene);
    draw_handle(ui, layout, scene, tabs);
    draw_tab(ui, atlas, layout, scene, tabs, selected);
    if scene.creative.tab.shows_search_box() {
        let text = scene.creative.search.clone();
        draw_text(ui, layout, SEARCH_POSITION, text, SEARCH_COLOR, true);
    }
    draw_slots(ui, atlas, layout, scene, textured);
    if scene.creative.tab.shows_title() {
        let title = resolve(&TooltipText::Translation {
            key: scene.creative.tab.translation_key(),
            args: Vec::new(),
        });
        draw_text(ui, layout, TITLE_POSITION, title, TITLE_COLOR, false);
    }
    draw_cursor_item(
        ui,
        atlas,
        layout.scale(),
        scene.window.cursor.as_ref(),
        scene.items,
    );
    if let Some(pointer) = ui.pointer() {
        let lines = hover_lines(scene, layout.local((pointer[0], pointer[1])));
        draw_tooltip(ui, atlas, viewport, layout.scale() as f32, pointer, &lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(id: i16) -> ItemStack {
        ItemStack {
            item_id: id,
            stack_size: 1,
            damage: 0,
            nbt: None,
        }
    }

    fn view(tab: CreativeTab) -> CreativeView {
        let mut grid = vec![None; 45];
        grid[4] = Some(stack(7));
        CreativeView {
            tab,
            handle_offset: 0,
            needs_scrollbar: true,
            search: String::new(),
            grid,
        }
    }

    fn window() -> WindowView {
        let mut player_slots: [Option<ItemStack>; 45] = std::array::from_fn(|_| None);
        player_slots[36] = Some(stack(1));
        player_slots[9] = Some(stack(2));
        WindowView {
            player_slots,
            container: None,
            cursor: None,
            selected_slot: 0,
        }
    }

    fn scene_of<'a>(window: &'a WindowView, creative: &'a CreativeView) -> CreativeScene<'a> {
        CreativeScene {
            window,
            creative,
            gui_atlas: None,
            items: ItemView::new(None, Default::default()),
        }
    }

    fn id_in(scene: &CreativeScene<'_>, slot: CreativeSlot) -> Option<i16> {
        stack_in(scene, slot).map(|s| s.item_id)
    }

    #[test]
    fn slots_read_the_grid_the_hotbar_and_the_player_window() {
        let (window, creative) = (window(), view(CreativeTab::BuildingBlocks));
        let scene = CreativeScene {
            window: &window,
            creative: &creative,
            gui_atlas: None,
            items: ItemView::new(None, Default::default()),
        };
        assert_eq!(id_in(&scene, CreativeSlot::Grid(4)), Some(7));
        assert_eq!(id_in(&scene, CreativeSlot::Grid(5)), None);
        assert_eq!(id_in(&scene, CreativeSlot::Hotbar(0)), Some(1));
        assert_eq!(id_in(&scene, CreativeSlot::Player(9)), Some(2));
        assert_eq!(id_in(&scene, CreativeSlot::Trash), None);
    }

    #[test]
    fn tab_icons_sit_below_the_top_edge_and_above_the_bottom_one() {
        // draw_origin (0, -28) and (0, 132) plus the icon offsets.
        assert_eq!(tab_icon_position(CreativeTab::BuildingBlocks), (6, -19));
        assert_eq!(tab_icon_position(CreativeTab::Food), (6, 139));
        assert_eq!(tab_rect(CreativeTab::Search), (167, -28, 28, 32));
    }

    #[test]
    fn the_greyed_handle_is_twelve_pixels_to_the_right() {
        assert_eq!(handle_origin(true), (232, 0));
        assert_eq!(handle_origin(false), (244, 0));
    }

    #[test]
    fn hover_boxes_are_inset_from_the_tab_buttons() {
        assert_eq!(hovered_tab(3, -29), Some(CreativeTab::BuildingBlocks));
        assert_eq!(hovered_tab(2, -29), None);
        assert_eq!(hovered_tab(33, 140), Some(CreativeTab::Tools));
        assert_eq!(hovered_tab(100, 60), None);
    }

    #[test]
    fn the_bin_hover_box_is_exactly_the_item_area() {
        assert!(over_bin(173, 112));
        assert!(over_bin(188, 127));
        assert!(!over_bin(172, 112));
        assert!(!over_bin(189, 112));
    }

    #[test]
    fn hover_text_names_tabs_and_the_bin_and_items_only_with_an_empty_cursor() {
        let mut window = window();
        let creative = view(CreativeTab::Inventory);
        let hover = |window: &WindowView, point| hover_lines(&scene_of(window, &creative), point);
        assert_eq!(hover(&window, (180, 120)).len(), 1);
        let (x, y) = CreativeSlot::Player(9).position().unwrap_or_default();
        assert!(!hover(&window, (x + 2, y + 2)).is_empty());
        window.cursor = Some(stack(3));
        assert!(hover(&window, (x + 2, y + 2)).is_empty());
        assert_eq!(hover(&window, (5, -20)).len(), 1);
    }
}
