// SPDX-License-Identifier: MIT OR Apache-2.0

//! Inventory and container screen layouts matching vanilla 1.8.
//!
//! The layouts replicate the exact pixel positions from Minecraft's
//! `ContainerPlayer` / `ContainerChest` and the `ScaledResolution` auto-scale
//! algorithm. Slot rectangles are 16×16 item areas (not the 18-pixel stride
//! that includes the texture border), placed at the same offsets the vanilla
//! GUI classes use, then multiplied by the GUI scale factor and centred in the
//! viewport.

use crate::geom::Rect;

// ---------------------------------------------------------------------------
// GUI scale — vanilla ScaledResolution auto-scale algorithm
// ---------------------------------------------------------------------------

/// Minimum scaled-width the vanilla algorithm enforces.
const MIN_SCALED_WIDTH: u32 = 320;
/// Minimum scaled-height the vanilla algorithm enforces.
const MIN_SCALED_HEIGHT: u32 = 240;

/// Computes the vanilla auto GUI scale factor for a physical viewport.
///
/// Replicates the `ScaledResolution` constructor with `guiScale == 0` (auto):
/// the scale starts at 1 and increments while the scaled dimensions stay
/// above 320×240.
#[must_use]
pub fn gui_scale(width: u32, height: u32) -> u32 {
    let mut scale: u32 = 1;
    loop {
        let next = scale + 1;
        if width / next >= MIN_SCALED_WIDTH && height / next >= MIN_SCALED_HEIGHT {
            scale = next;
        } else {
            break;
        }
    }
    scale
}

// ---------------------------------------------------------------------------
// Player inventory — vanilla ContainerPlayer slot offsets (176×166 texture)
// ---------------------------------------------------------------------------

/// Texture width of the player inventory GUI, in unscaled pixels.
const INV_WIDTH: f32 = 176.0;
/// Texture height of the player inventory GUI, in unscaled pixels.
const INV_HEIGHT: f32 = 166.0;
/// Item area inside a slot, in unscaled pixels.
const ITEM_SIZE: f32 = 16.0;
/// Slot stride (item + border), in unscaled pixels.
const SLOT_STRIDE: u32 = 18;

/// Vanilla slot positions for the 45-slot player inventory, in the 176×166
/// texture coordinate system.  Each entry is `(x, y)` of the 16×16 item area.
const PLAYER_SLOT_OFFSETS: [(u32, u32); 45] = {
    let mut offsets = [(0u32, 0u32); 45];

    // Slot 0: crafting output.
    offsets[0] = (144, 36);

    // Slots 1–4: 2×2 crafting grid starting at (88, 26), stride 18.
    let mut i = 0;
    while i < 4 {
        let col = i % 2;
        let row = i / 2;
        offsets[1 + i] = (88 + col as u32 * SLOT_STRIDE, 26 + row as u32 * SLOT_STRIDE);
        i += 1;
    }

    // Slots 5–8: 4 armor slots at x=8, starting y=8, stride 18.
    let mut k = 0;
    while k < 4 {
        offsets[5 + k] = (8, 8 + k as u32 * SLOT_STRIDE);
        k += 1;
    }

    // Slots 9–35: 3 rows of 9 main inventory at (8, 84), stride 18.
    let mut idx = 0;
    while idx < 27 {
        let col = idx % 9;
        let row = idx / 9;
        offsets[9 + idx] = (8 + col as u32 * SLOT_STRIDE, 84 + row as u32 * SLOT_STRIDE);
        idx += 1;
    }

    // Slots 36–44: 1 row of 9 hotbar at (8, 142), stride 18.
    let mut h = 0;
    while h < 9 {
        offsets[36 + h] = (8 + h as u32 * SLOT_STRIDE, 142);
        h += 1;
    }

    offsets
};

/// The layout of a player inventory screen.
#[derive(Debug, Clone)]
pub struct InventoryLayout {
    /// The 45 slot rectangles (16×16 item areas), indexed as the protocol does.
    pub slots: [Rect; 45],
    /// The background rectangle covering the 176×166 texture, scaled and centred.
    pub background: Rect,
}

/// Computes the layout of the player inventory screen (45 slots).
///
/// Uses the vanilla `ScaledResolution` algorithm for the GUI scale factor and
/// the exact slot offsets from `ContainerPlayer`.
#[must_use]
pub fn player_inventory_layout(viewport: Rect) -> InventoryLayout {
    let scale = gui_scale(viewport.width as u32, viewport.height as u32);
    let sf = scale as f32;

    let bg_w = INV_WIDTH * sf;
    let bg_h = INV_HEIGHT * sf;
    let gui_left = viewport.x + (viewport.width - bg_w) * 0.5;
    let gui_top = viewport.y + (viewport.height - bg_h) * 0.5;

    let item = ITEM_SIZE * sf;
    let mut slots = [Rect::ZERO; 45];
    for (i, &(ox, oy)) in PLAYER_SLOT_OFFSETS.iter().enumerate() {
        slots[i] = Rect::new(
            gui_left + ox as f32 * sf,
            gui_top + oy as f32 * sf,
            item,
            item,
        );
    }

    let background = Rect::new(gui_left, gui_top, bg_w, bg_h);
    InventoryLayout { slots, background }
}

// ---------------------------------------------------------------------------
// Container — vanilla ContainerChest slot offsets (176 × variable height)
// ---------------------------------------------------------------------------

/// Texture width of all container GUIs, in unscaled pixels.
const CONTAINER_WIDTH: f32 = 176.0;

/// The layout of a container screen.
#[derive(Debug, Clone)]
pub struct ContainerLayout {
    /// The container's own slot rectangles.
    pub container_slots: Vec<Rect>,
    /// The player's 36 main+hotbar slot rectangles (27 main + 9 hotbar),
    /// mapped to the player inventory indices (9–44).
    pub player_slots: [(usize, Rect); 36],
    /// The background rectangle covering the texture, scaled and centred.
    pub background: Rect,
}

/// Computes the layout of a container screen (chest-like: rows of 9 slots).
///
/// Uses the vanilla `GuiChest` / `ContainerChest` slot offsets:
/// - Container slots at `(8 + col*18, 18 + row*18)`
/// - Player main at `(8 + col*18, 103 + row*18 + offset)` where offset = `(rows - 4) * 18`
/// - Hotbar at `(8 + col*18, 161 + offset)`
/// - Background height: `114 + rows * 18`
#[must_use]
pub fn container_layout(viewport: Rect, container_slot_count: usize) -> ContainerLayout {
    let scale = gui_scale(viewport.width as u32, viewport.height as u32);
    let sf = scale as f32;

    let rows = container_slot_count.div_ceil(9);
    let offset = (rows as i32 - 4) * SLOT_STRIDE as i32;
    let bg_h = (114 + rows as u32 * SLOT_STRIDE) as f32;

    let bg_w = CONTAINER_WIDTH * sf;
    let bg_h_scaled = bg_h * sf;
    let gui_left = viewport.x + (viewport.width - bg_w) * 0.5;
    let gui_top = viewport.y + (viewport.height - bg_h_scaled) * 0.5;

    let item = ITEM_SIZE * sf;

    // Container slots.
    let mut container_slots = Vec::with_capacity(container_slot_count);
    for i in 0..container_slot_count {
        let col = (i % 9) as u32;
        let row = (i / 9) as u32;
        let x = 8 + col * SLOT_STRIDE;
        let y = 18 + row * SLOT_STRIDE;
        container_slots.push(Rect::new(
            gui_left + x as f32 * sf,
            gui_top + y as f32 * sf,
            item,
            item,
        ));
    }

    // Player main inventory: slots 9–35 (3 rows of 9).
    let mut player_slots = [(0usize, Rect::ZERO); 36];
    for (i, entry) in player_slots[..27].iter_mut().enumerate() {
        let col = (i % 9) as u32;
        let row = (i / 9) as u32;
        let x = 8 + col * SLOT_STRIDE;
        let y = (103 + row as i32 * SLOT_STRIDE as i32 + offset) as u32;
        *entry = (
            9 + i,
            Rect::new(
                gui_left + x as f32 * sf,
                gui_top + y as f32 * sf,
                item,
                item,
            ),
        );
    }

    // Hotbar: slots 36–44.
    for (i, entry) in player_slots[27..].iter_mut().enumerate() {
        let col = i as u32;
        let x = 8 + col * SLOT_STRIDE;
        let y = (161 + offset) as u32;
        *entry = (
            36 + i,
            Rect::new(
                gui_left + x as f32 * sf,
                gui_top + y as f32 * sf,
                item,
                item,
            ),
        );
    }

    let background = Rect::new(gui_left, gui_top, bg_w, bg_h_scaled);

    ContainerLayout {
        container_slots,
        player_slots,
        background,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_scale_matches_vanilla_scaled_resolution() {
        // 640×480 → scale 2 (640/2=320≥320, 480/2=240≥240; 640/3<320 → stop)
        assert_eq!(gui_scale(640, 480), 2);
        // 1280×720 → scale 3
        assert_eq!(gui_scale(1280, 720), 3);
        // 1920×1080 → scale 4
        assert_eq!(gui_scale(1920, 1080), 4);
        // Tiny window → scale 1
        assert_eq!(gui_scale(400, 300), 1);
    }

    #[test]
    fn player_inventory_layout_has_45_slots() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = player_inventory_layout(viewport);
        assert_eq!(layout.slots.len(), 45);
        for (i, slot) in layout.slots.iter().enumerate() {
            assert!(slot.width > 0.0 && slot.height > 0.0, "slot {i} is empty");
        }
    }

    #[test]
    fn player_inventory_background_matches_texture_size() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = player_inventory_layout(viewport);
        let scale = gui_scale(1280, 720) as f32;
        assert!((layout.background.width - 176.0 * scale).abs() < 0.1);
        assert!((layout.background.height - 166.0 * scale).abs() < 0.1);
    }

    #[test]
    fn player_inventory_is_centred_in_the_viewport() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = player_inventory_layout(viewport);
        let cx = viewport.x + viewport.width / 2.0;
        let cy = viewport.y + viewport.height / 2.0;
        let bg_cx = layout.background.x + layout.background.width / 2.0;
        let bg_cy = layout.background.y + layout.background.height / 2.0;
        assert!((cx - bg_cx).abs() < 1.0);
        assert!((cy - bg_cy).abs() < 1.0);
    }

    #[test]
    fn container_layout_maps_27_chest_slots_plus_36_player_slots() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = container_layout(viewport, 27);
        assert_eq!(layout.container_slots.len(), 27);
        assert_eq!(layout.player_slots.len(), 36);
        assert_eq!(layout.player_slots[0].0, 9);
        assert_eq!(layout.player_slots[35].0, 44);
    }

    #[test]
    fn the_hotbar_sits_below_the_main_inventory() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = player_inventory_layout(viewport);
        assert!(layout.slots[36].y > layout.slots[35].y);
    }

    #[test]
    fn container_layout_is_centred_in_the_viewport() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = container_layout(viewport, 27);
        let cx = viewport.x + viewport.width / 2.0;
        let bg_cx = layout.background.x + layout.background.width / 2.0;
        assert!((cx - bg_cx).abs() < 1.0);
    }

    #[test]
    fn slots_sit_inside_the_background() {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let layout = player_inventory_layout(viewport);
        let bg = &layout.background;
        for (i, slot) in layout.slots.iter().enumerate() {
            assert!(
                slot.x >= bg.x
                    && slot.y >= bg.y
                    && slot.x + slot.width <= bg.x + bg.width
                    && slot.y + slot.height <= bg.y + bg.height,
                "slot {i} at ({}, {}) size {}×{} escapes bg ({}, {}) size {}×{}",
                slot.x,
                slot.y,
                slot.width,
                slot.height,
                bg.x,
                bg.y,
                bg.width,
                bg.height,
            );
        }
    }
}
