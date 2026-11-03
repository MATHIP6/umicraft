// SPDX-License-Identifier: MIT OR Apache-2.0

//! The brewing stand, after `ContainerBrewingStand` / `GuiBrewingStand`.

use super::decoration::Decoration;
use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Height of the bubble column sprite.
const BUBBLES_HEIGHT: u32 = 29;
/// Brewing time of a full cycle in ticks (`TileEntityBrewingStand`).
const BREW_TIME: f32 = 400.0;
/// Height of the progress arrow when drawn in full.
const ARROW_HEIGHT: f32 = 28.0;

/// Builds the brewing stand screen: three bottles, the ingredient, backpack,
/// hotbar.
pub(super) fn spec() -> ScreenSpec {
    let own = [(56, 46), (79, 53), (102, 46), (79, 17)];
    let mut slots: Vec<SlotSpec> = own
        .iter()
        .enumerate()
        .map(|(index, &(x, y))| SlotSpec { index, x, y })
        .collect();
    slots.extend(player_inventory_slots(4, 84));
    ScreenSpec::single("container/brewing_stand", (176, 166), slots)
}

/// The arrow and the bubbles for the brewing time left (window property 0).
///
/// Vanilla draws nothing while no brew runs. The arrow grows downward as the
/// time runs out; the bubbles cycle through seven heights, `time / 2 % 7`.
pub(super) fn decorations(properties: &[i16]) -> Vec<Decoration> {
    let time = i32::from(properties.first().copied().unwrap_or(0));
    if time <= 0 {
        return Vec::new();
    }
    let mut decorations = Vec::with_capacity(2);
    let arrow = (ARROW_HEIGHT * (1.0 - time as f32 / BREW_TIME)) as u32;
    if arrow > 0 {
        decorations.push(Decoration::full((176, 0), (9, arrow), (97, 16)));
    }
    let bubbles = match time / 2 % 7 {
        0 => 29,
        1 => 24,
        2 => 20,
        3 => 16,
        4 => 11,
        5 => 6,
        _ => 0,
    };
    if bubbles > 0 {
        decorations.push(Decoration::full(
            (185, BUBBLES_HEIGHT - bubbles),
            (12, bubbles),
            (65, 14 + BUBBLES_HEIGHT - bubbles),
        ));
    }
    decorations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_brewing_stand_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 40);
        let at = |i: usize| (slots[i].x, slots[i].y);
        assert_eq!(at(0), (56, 46));
        assert_eq!(at(1), (79, 53));
        assert_eq!(at(2), (102, 46));
        assert_eq!(at(3), (79, 17));
        assert_eq!(at(4), (8, 84));
        assert_eq!(at(39), (152, 142));
    }

    #[test]
    fn an_idle_stand_draws_nothing() {
        assert!(decorations(&[]).is_empty());
        assert!(decorations(&[0]).is_empty());
    }

    #[test]
    fn arrow_grows_as_the_time_runs_out() {
        // 28 * (1 - 200 / 400) = 14 rows.
        let d = decorations(&[200]);
        assert_eq!(
            (d[0].src, d[0].size, d[0].dst),
            ((176, 0), (9, 14), (97, 16))
        );
    }

    #[test]
    fn bubbles_follow_the_seven_step_cycle() {
        // 200 / 2 % 7 = 2 -> 20 rows, drawn from the bottom of the sprite.
        let d = decorations(&[200]);
        assert_eq!(
            (d[1].src, d[1].size, d[1].dst),
            ((185, 9), (12, 20), (65, 23))
        );
        // 12 / 2 % 7 = 6 -> no bubbles; the arrow is at 28 * (1 - 0.03) = 27.
        assert_eq!(decorations(&[12]).len(), 1);
    }
}
