// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hearts, armour, food, mount hearts and air bubbles, drawn from a
//! [`VitalsView`] with the `icons.png` sprites (MCP
//! `GuiIngame.renderPlayerStats`).
//!
//! Sprite columns follow the reference's offsets: a heart family starts at
//! `16` (normal), `52` (poison) or `88` (wither); inside a family `+0` is the
//! container, `+36` a full heart, `+45` a half, `+54`/`+63` the ghost of the
//! old health and `+144`/`+153` the golden absorption hearts.

use umicraft_game::hud::{
    AirView, ArmorView, Fill, FoodIconView, HeartBody, HeartStyle, HeartView, MountHeartView,
    VitalsView,
};
use umicraft_ui::SpriteSheet;

use super::icons::{Canvas, Region};

/// Distance between two icons of a row.
const STEP: i32 = 8;
/// Edge of an icon.
const ICON: u32 = 9;
/// Container texture column of the normal hearts.
const HEART_BASE: u32 = 16;
/// Columns between two heart families.
const FAMILY: u32 = 36;
/// Texture row of the hardcore hearts (`9 * 5`).
const HARDCORE_V: u32 = 45;
/// Texture row of the food icons.
const FOOD_V: u32 = 27;
/// Texture row of the armour icons and the mount hearts.
const ARMOR_V: u32 = 9;
/// Texture row of the air bubbles.
const AIR_V: u32 = 18;
/// Container column of the mount hearts (`j5 = 52`).
const MOUNT_BASE: u32 = 52;

/// Draws every status row of `view`: armour, hearts, food or mount hearts,
/// then air (the order of the reference).
pub(super) fn draw_vitals(canvas: &mut Canvas<'_>, view: &VitalsView, icons: &SpriteSheet) {
    let baseline = canvas.height - 39;
    let left = canvas.width / 2 - 91;
    let right = canvas.width / 2 + 91;
    if let Some(armor) = &view.armor {
        draw_armor(canvas, icons, armor, left, baseline + view.upper_dy);
    }
    for heart in &view.hearts {
        draw_heart(canvas, icons, heart, left, baseline);
    }
    for food in &view.food {
        draw_food(canvas, icons, food, right, baseline);
    }
    for heart in &view.mount {
        draw_mount_heart(canvas, icons, heart, right, baseline);
    }
    if let Some(air) = view.air {
        draw_air(canvas, icons, air, right, baseline + view.upper_dy);
    }
}

fn icon(u: u32, v: u32) -> Region {
    (u, v, ICON, ICON)
}

fn draw_armor(canvas: &mut Canvas<'_>, icons: &SpriteSheet, armor: &ArmorView, x: i32, y: i32) {
    for (index, fill) in armor.icons.iter().enumerate() {
        let u = match fill {
            Fill::Full => 34,
            Fill::Half => 25,
            Fill::Empty => 16,
        };
        canvas.sprite(icons, icon(u, ARMOR_V), x + index as i32 * STEP, y);
    }
}

fn draw_heart(
    canvas: &mut Canvas<'_>,
    icons: &SpriteSheet,
    heart: &HeartView,
    left: i32,
    base: i32,
) {
    let family = HEART_BASE
        + match heart.style {
            HeartStyle::Normal => 0,
            HeartStyle::Poison => FAMILY,
            HeartStyle::Wither => 2 * FAMILY,
        };
    let v = if heart.hardcore { HARDCORE_V } else { 0 };
    let (x, y) = (left + i32::from(heart.col) * STEP, base + heart.dy);
    let container = HEART_BASE + if heart.blink { ICON } else { 0 };
    canvas.sprite(icons, icon(container, v), x, y);
    if heart.blink {
        draw_fill(
            canvas,
            icons,
            heart.ghost,
            (family + 54, family + 63),
            v,
            (x, y),
        );
    }
    match heart.body {
        HeartBody::Absorption { half } => {
            let u = family + if half { 153 } else { 144 };
            canvas.sprite(icons, icon(u, v), x, y);
        }
        HeartBody::Health(fill) => {
            draw_fill(canvas, icons, fill, (family + 36, family + 45), v, (x, y));
        }
    }
}

/// Draws the full or half sprite for `fill`, nothing for an empty icon.
fn draw_fill(
    canvas: &mut Canvas<'_>,
    icons: &SpriteSheet,
    fill: Fill,
    (full, half): (u32, u32),
    v: u32,
    (x, y): (i32, i32),
) {
    match fill {
        Fill::Full => canvas.sprite(icons, icon(full, v), x, y),
        Fill::Half => canvas.sprite(icons, icon(half, v), x, y),
        Fill::Empty => {}
    }
}

fn draw_food(
    canvas: &mut Canvas<'_>,
    icons: &SpriteSheet,
    food: &FoodIconView,
    right: i32,
    base: i32,
) {
    let (x, y) = (right - i32::from(food.col) * STEP - 9, base + food.dy);
    // Hunger shifts the family by 36 columns and the container by 13 icons.
    let (family, container) = if food.hunger {
        (HEART_BASE + FAMILY, HEART_BASE + 13 * ICON)
    } else {
        (HEART_BASE, HEART_BASE)
    };
    canvas.sprite(icons, icon(container, FOOD_V), x, y);
    draw_fill(
        canvas,
        icons,
        food.fill,
        (family + 36, family + 45),
        FOOD_V,
        (x, y),
    );
}

fn draw_mount_heart(
    canvas: &mut Canvas<'_>,
    icons: &SpriteSheet,
    heart: &MountHeartView,
    right: i32,
    base: i32,
) {
    let (x, y) = (right - i32::from(heart.col) * STEP - 9, base + heart.dy);
    canvas.sprite(icons, icon(MOUNT_BASE, ARMOR_V), x, y);
    draw_fill(
        canvas,
        icons,
        heart.fill,
        (MOUNT_BASE + 36, MOUNT_BASE + 45),
        ARMOR_V,
        (x, y),
    );
}

fn draw_air(canvas: &mut Canvas<'_>, icons: &SpriteSheet, air: AirView, right: i32, y: i32) {
    for index in 0..air.full + air.popping {
        let u = if index < air.full { 16 } else { 25 };
        canvas.sprite(icons, icon(u, AIR_V), right - index as i32 * STEP - 9, y);
    }
}

#[cfg(test)]
mod tests {
    use umicraft_game::hud::{vitals_view, FlashFrame, Riding, VitalsInput};
    use umicraft_ui::{DrawCommand, DrawList};

    use super::*;

    fn input() -> VitalsInput {
        VitalsInput::default()
    }

    /// The sources drawn, as `(u, v)` texture pixels of a 256-pixel sheet.
    fn sprites(input: &VitalsInput) -> Vec<(u32, u32)> {
        let mut list = DrawList::new();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_vitals(&mut canvas, &vitals_view(input), &sheet);
        list.commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Image { src, .. } => Some((
                    (src[0] * 256.0).round() as u32,
                    (src[1] * 256.0).round() as u32,
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn full_health_and_food_use_the_vanilla_sprites() {
        let drawn = sprites(&input());
        // 10 hearts: container (16, 0) + full (52, 0); 10 food: (16, 27) + (52, 27).
        assert_eq!(drawn.iter().filter(|s| **s == (16, 0)).count(), 10);
        assert_eq!(drawn.iter().filter(|s| **s == (52, 0)).count(), 10);
        assert_eq!(drawn.iter().filter(|s| **s == (16, 27)).count(), 10);
        assert_eq!(drawn.iter().filter(|s| **s == (52, 27)).count(), 10);
        assert_eq!(drawn.len(), 40);
    }

    #[test]
    fn poison_wither_and_hardcore_pick_their_sprite_families() {
        let poison = sprites(&VitalsInput {
            poison: true,
            ..input()
        });
        assert!(poison.contains(&(88, 0)));
        let wither = sprites(&VitalsInput {
            wither: true,
            health: 19.0,
            ..input()
        });
        assert!(wither.contains(&(124, 0)) && wither.contains(&(133, 0)));
        let hardcore = sprites(&VitalsInput {
            hardcore: true,
            ..input()
        });
        assert!(hardcore.contains(&(16, 45)) && hardcore.contains(&(52, 45)));
    }

    #[test]
    fn absorption_hearts_use_the_golden_columns() {
        let drawn = sprites(&VitalsInput {
            absorption: 3.0,
            ..input()
        });
        assert!(drawn.contains(&(160, 0)) && drawn.contains(&(169, 0)));
    }

    #[test]
    fn the_blink_draws_the_highlighted_container_and_the_ghost() {
        let drawn = sprites(&VitalsInput {
            health: 10.0,
            flash: FlashFrame {
                blink: true,
                last_health: 14,
            },
            ..input()
        });
        assert!(drawn.contains(&(25, 0)), "highlighted container");
        assert!(drawn.contains(&(70, 0)), "ghost, full");
    }

    #[test]
    fn hunger_uses_its_container_and_family() {
        let drawn = sprites(&VitalsInput {
            hunger: true,
            ..input()
        });
        assert!(drawn.contains(&(133, 27)));
        assert!(drawn.contains(&(88, 27)));
    }

    #[test]
    fn armour_icons_follow_the_value() {
        let drawn = sprites(&VitalsInput {
            armor: 3,
            ..input()
        });
        assert_eq!(drawn.iter().filter(|s| **s == (34, 9)).count(), 1);
        assert_eq!(drawn.iter().filter(|s| **s == (25, 9)).count(), 1);
        assert_eq!(drawn.iter().filter(|s| **s == (16, 9)).count(), 8);
    }

    #[test]
    fn bubbles_and_mount_hearts_replace_food() {
        let drawn = sprites(&VitalsInput {
            air: Some(272),
            riding: Riding::Mob {
                health: 5.0,
                max_health: 20.0,
            },
            ..input()
        });
        assert_eq!(drawn.iter().filter(|s| **s == (16, 18)).count(), 9);
        assert_eq!(drawn.iter().filter(|s| **s == (25, 18)).count(), 1);
        assert_eq!(drawn.iter().filter(|s| **s == (52, 9)).count(), 10);
        assert!(!drawn.contains(&(16, 27)), "no food while riding");
    }
}
