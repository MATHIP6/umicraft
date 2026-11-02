// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar background and the selected-slot cursor (`widgets.png`).

use umicraft_ui::{DrawList, Rect, SpriteSheet};

use crate::hud::{HudState, HOTBAR_SLOTS};

/// Draws the hotbar and its selection cursor; the items are drawn by the UI layer.
pub(super) fn draw_hotbar(
    list: &mut DrawList,
    state: &HudState,
    gw: f32,
    gh: f32,
    sf: f32,
    widgets: &SpriteSheet,
) {
    let x = gw * 0.5 - 91.0;
    let y = gh - 22.0;

    // Hotbar background: (0, 0, 182, 22)
    let src = widgets.sub(0, 0, 182, 22);
    let dst = Rect::new(x * sf, y * sf, 182.0 * sf, 22.0 * sf);
    list.image(src, dst, [1.0; 4]);

    // Selected slot cursor: (0, 22, 24, 24) at (W/2 - 91 - 1 + slot * 20, H - 23)
    let slot = (state.selected_slot as usize).min(HOTBAR_SLOTS - 1) as f32;
    let sel_x = gw * 0.5 - 91.0 - 1.0 + slot * 20.0;
    let sel_y = gh - 23.0;
    let sel_src = widgets.sub(0, 22, 24, 24);
    let sel_dst = Rect::new(sel_x * sf, sel_y * sf, 24.0 * sf, 24.0 * sf);
    list.image(sel_src, sel_dst, [1.0; 4]);
    // The items and their counts are drawn by the UI layer over the hotbar.
}
