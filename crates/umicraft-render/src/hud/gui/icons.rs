// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `icons.png` layer: crosshair, health, food and armour rows, the
//! experience bar and level.

use umicraft_ui::{DrawList, FontAtlas, Rect, SpriteSheet, TextStyle};

use crate::hud::HudState;
use crate::hud_hearts;

/// Draws every `icons.png` element of the HUD.
pub(super) fn draw_icons_layer(
    list: &mut DrawList,
    state: &HudState,
    gw: f32,
    gh: f32,
    sf: f32,
    font: &FontAtlas,
    icons: &SpriteSheet,
) {
    // Crosshair: icons (0, 0, 16, 16) at (W/2 - 7, H/2 - 7)
    if state.crosshair {
        let cx = (gw * 0.5 - 7.0) * sf;
        let cy = (gh * 0.5 - 7.0) * sf;
        let src = icons.sub(0, 0, 16, 16);
        let dst = Rect::new(cx, cy, 16.0 * sf, 16.0 * sf);
        list.image(src, dst, [1.0; 4]);
    }

    // Health: y = gh - 39
    let hearts = hud_hearts::layout(&state.hearts, state.health, gw, gh);
    for sprite in &hearts.sprites {
        let dst = Rect::new(
            sprite.x as f32 * sf,
            sprite.y as f32 * sf,
            9.0 * sf,
            9.0 * sf,
        );
        list.image(icons.sub(sprite.u, sprite.v, 9, 9), dst, [1.0; 4]);
    }

    // Food: y = gh - 39, x = gw / 2 + 91 - i * 8 - 9
    let food_y = gh - 39.0;
    let ceil_food = state.food.ceil() as i32;
    for i in 0..10 {
        let x = gw * 0.5 + 91.0 - i as f32 * 8.0 - 9.0;
        let dst = Rect::new(x * sf, food_y * sf, 9.0 * sf, 9.0 * sf);
        // Container
        list.image(icons.sub(16, 27, 9, 9), dst, [1.0; 4]);
        let val = i * 2 + 1;
        if val < ceil_food {
            list.image(icons.sub(52, 27, 9, 9), dst, [1.0; 4]);
        } else if val == ceil_food {
            list.image(icons.sub(61, 27, 9, 9), dst, [1.0; 4]);
        }
    }

    // Armor: y = gh - 49 if armor > 0
    if state.armor > 0.0 {
        let armor_y = hearts.armor_y as f32;
        let ceil_armor = state.armor.ceil() as i32;
        for i in 0..10 {
            let x = gw * 0.5 - 91.0 + i as f32 * 8.0;
            let dst = Rect::new(x * sf, armor_y * sf, 9.0 * sf, 9.0 * sf);
            let val = i * 2 + 1;
            let sub = if val < ceil_armor {
                icons.sub(34, 9, 9, 9)
            } else if val == ceil_armor {
                icons.sub(25, 9, 9, 9)
            } else {
                icons.sub(16, 9, 9, 9)
            };
            list.image(sub, dst, [1.0; 4]);
        }
    }

    // Experience bar: y = gh - 29
    let xp_x = gw * 0.5 - 91.0;
    let xp_y = gh - 29.0;
    list.image(
        icons.sub(0, 64, 182, 5),
        Rect::new(xp_x * sf, xp_y * sf, 182.0 * sf, 5.0 * sf),
        [1.0; 4],
    );
    let fill_w = ((state.experience * 183.0).floor() as u32).min(182);
    if fill_w > 0 {
        list.image(
            icons.sub(0, 69, fill_w, 5),
            Rect::new(xp_x * sf, xp_y * sf, fill_w as f32 * sf, 5.0 * sf),
            [1.0; 4],
        );
    }

    // Experience level: y = gh - 35
    if state.level > 0 {
        let s = state.level.to_string();
        let text_w = font.measure(&s, 1.0);
        let lx = gw * 0.5 - text_w * 0.5;
        let ly = gh - 35.0;
        let outline = [0.0, 0.0, 0.0, 1.0];
        let green = [0x80 as f32 / 255.0, 1.0, 0x20 as f32 / 255.0, 1.0];
        list.text([(lx + 1.0) * sf, ly * sf], &s, TextStyle::new(sf, outline));
        list.text([(lx - 1.0) * sf, ly * sf], &s, TextStyle::new(sf, outline));
        list.text([lx * sf, (ly + 1.0) * sf], &s, TextStyle::new(sf, outline));
        list.text([lx * sf, (ly - 1.0) * sf], &s, TextStyle::new(sf, outline));
        list.text([lx * sf, ly * sf], &s, TextStyle::new(sf, green));
    }
}
