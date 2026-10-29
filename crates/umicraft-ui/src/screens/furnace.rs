// SPDX-License-Identifier: MIT OR Apache-2.0

//! The furnace, after `ContainerFurnace` / `GuiFurnace`.

use super::decoration::{Decoration, Fill};
use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Height of the flame sprite (`getBurnLeftScaled(13)` plus the 1px base).
const FLAME_HEIGHT: u32 = 13;
/// Width of the progress arrow sprite (`getCookProgressScaled(24)` + 1).
const ARROW_WIDTH: u32 = 25;
/// Burn time vanilla assumes when the server has not sent property 1 yet.
const DEFAULT_BURN_TIME: i32 = 200;

/// Builds the furnace screen: input, fuel, output, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![
        SlotSpec {
            index: 0,
            x: 56,
            y: 17,
        },
        SlotSpec {
            index: 1,
            x: 56,
            y: 53,
        },
        SlotSpec {
            index: 2,
            x: 116,
            y: 35,
        },
    ];
    slots.extend(player_inventory_slots(3, 84));
    ScreenSpec::single("container/furnace", (176, 166), slots)
}

/// The flame and arrow for the furnace window properties
/// `[burn time left, current item burn time, cook time, total cook time]`.
///
/// Missing properties count as 0, like a furnace that has not started.
pub(super) fn decorations(properties: &[i16]) -> Vec<Decoration> {
    let property = |id: usize| i32::from(properties.get(id).copied().unwrap_or(0)).max(0);
    let (burn_left, burn_total, cook, cook_total) =
        (property(0), property(1), property(2), property(3));

    let mut decorations = Vec::with_capacity(2);
    if burn_left > 0 {
        let total = if burn_total == 0 {
            DEFAULT_BURN_TIME
        } else {
            burn_total
        };
        let k = (burn_left * 13 / total).clamp(0, 12) as u32;
        decorations.push(Decoration {
            src: (176, 0),
            size: (14, FLAME_HEIGHT),
            dst: (56, 36),
            fill: Fill::Up((k + 1) as f32 / FLAME_HEIGHT as f32),
        });
    }
    let l = if cook_total != 0 && cook != 0 {
        (cook * 24 / cook_total).clamp(0, 24) as u32
    } else {
        0
    };
    decorations.push(Decoration {
        src: (176, 14),
        size: (ARROW_WIDTH, 16),
        dst: (79, 34),
        fill: Fill::Right((l + 1) as f32 / ARROW_WIDTH as f32),
    });
    decorations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(d: &Decoration) -> (u32, u32, u32, u32) {
        d.sprite_rect((0.0, 0.0), 1).unwrap().src
    }

    #[test]
    fn matches_container_furnace_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 39);
        assert_eq!((slots[0].x, slots[0].y), (56, 17));
        assert_eq!((slots[1].x, slots[1].y), (56, 53));
        assert_eq!((slots[2].x, slots[2].y), (116, 35));
        assert_eq!((slots[3].x, slots[3].y), (8, 84));
        assert_eq!((slots[38].x, slots[38].y), (152, 142));
    }

    #[test]
    fn idle_furnace_draws_only_the_one_pixel_arrow_edge() {
        let d = decorations(&[0, 0, 0, 200]);
        assert_eq!(d.len(), 1);
        assert_eq!(rect(&d[0]), (176, 14, 1, 16));
    }

    #[test]
    fn full_burn_draws_the_whole_flame() {
        // k = 13 * 100 / 100 = 13 clamps to 12: rows 0..=12, as vanilla's 12 - k.
        let d = decorations(&[100, 100, 0, 200]);
        assert_eq!(d.len(), 2);
        assert_eq!(rect(&d[0]), (176, 0, 14, 13));
    }

    #[test]
    fn half_burn_matches_vanilla_formula() {
        // k = 50 * 13 / 100 = 6 -> src y = 12 - 6, height 7, dst y = 36 + 12 - 6.
        let d = decorations(&[50, 100, 0, 200]);
        let sprite = d[0].sprite_rect((0.0, 0.0), 1).unwrap();
        assert_eq!(sprite.src, (176, 6, 14, 7));
        assert_eq!(sprite.dst.y, 42.0);
    }

    #[test]
    fn unknown_burn_total_defaults_to_200() {
        // k = 100 * 13 / 200 = 6.
        let d = decorations(&[100, 0, 0, 0]);
        assert_eq!(rect(&d[0]), (176, 6, 14, 7));
    }

    #[test]
    fn cook_progress_widens_the_arrow() {
        // l = 100 * 24 / 200 = 12 -> width 13.
        let d = decorations(&[0, 0, 100, 200]);
        assert_eq!(rect(&d[0]), (176, 14, 13, 16));
        let done = decorations(&[0, 0, 200, 200]);
        assert_eq!(rect(&done[0]), (176, 14, 25, 16));
    }

    #[test]
    fn short_property_lists_are_tolerated() {
        assert_eq!(decorations(&[]).len(), 1);
    }
}
