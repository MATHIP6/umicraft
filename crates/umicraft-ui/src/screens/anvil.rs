// SPDX-License-Identifier: MIT OR Apache-2.0

//! The anvil, after `ContainerRepair` / `GuiRepair`.
//!
//! The static layout, the name field background and what the cost line says
//! ([`cost_line`]) are here; the text field and `MC|ItemName` live with the
//! anvil input handling of the client, which also translates the cost line.

use super::decoration::Decoration;
use super::interaction::{Dynamic, ScreenState};
use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Window property of the repair cost (`ContainerRepair.maximumCost`).
const COST_PROPERTY: usize = 0;
/// From this cost on a survival player sees "Too Expensive!".
const TOO_EXPENSIVE: i32 = 40;
/// Colour of a cost the player can pay (`8453920`).
const COST_GREEN: u32 = 0x80_FF_20;
/// Colour of a cost the player cannot pay, or of "Too Expensive!" (`16736352`).
const COST_RED: u32 = 0xFF_60_60;
/// Right edge of the cost line (`xSize - 8`).
pub const COST_RIGHT: u32 = 168;
/// Top of the cost line.
pub const COST_Y: u32 = 67;

/// What the cost line of the anvil says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostText {
    /// `container.repair.cost` with the number of levels.
    Cost(i32),
    /// `container.repair.expensive`.
    TooExpensive,
}

/// The cost line of the anvil and its colour (`0xRRGGBB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostLine {
    /// The text to translate.
    pub text: CostText,
    /// The colour of the text.
    pub color: u32,
}

/// The cost line of `GuiRepair.drawGuiContainerForegroundLayer`, if any.
///
/// Nothing shows without a cost, nor without a result unless the cost is too
/// high. A survival player sees "Too Expensive!" from 40 levels on, and a red
/// cost when the result cannot be taken (`canTakeStack`: creative, or enough
/// levels).
#[must_use]
pub fn cost_line(state: &ScreenState<'_>) -> Option<CostLine> {
    let cost = i32::from(*state.properties.get(COST_PROPERTY)?);
    if cost <= 0 {
        return None;
    }
    if cost >= TOO_EXPENSIVE && !state.creative {
        return Some(CostLine {
            text: CostText::TooExpensive,
            color: COST_RED,
        });
    }
    let has_result = state.filled[2];
    if !has_result {
        return None;
    }
    let can_take = state.creative || state.xp_level >= cost;
    Some(CostLine {
        text: CostText::Cost(cost),
        color: if can_take { COST_GREEN } else { COST_RED },
    })
}

/// Builds the anvil screen: two inputs, the result, backpack, hotbar.
///
/// The name field is drawn at (59, 20) from `v = 166` (enabled) or `v = 182`
/// (no item in the first slot, so the field is disabled).
pub(super) fn spec() -> ScreenSpec {
    let mut slots: Vec<SlotSpec> = [(27, 47), (76, 47), (134, 47)]
        .iter()
        .enumerate()
        .map(|(index, &(x, y))| SlotSpec { index, x, y })
        .collect();
    slots.extend(player_inventory_slots(3, 84));
    ScreenSpec::single("container/anvil", (176, 166), slots)
}

/// The name field and the "no result" cross for the current slot contents.
pub(super) fn dynamic(state: &ScreenState<'_>) -> Dynamic {
    let [has_input, has_second, has_result] = state.filled;
    Dynamic::sprites(decorations(has_input, has_second, has_result))
}

fn decorations(has_input: bool, has_second: bool, has_result: bool) -> Vec<Decoration> {
    let field_v = if has_input { 166 } else { 182 };
    let mut decorations = vec![Decoration::full((0, field_v), (110, 16), (59, 20))];
    if (has_input || has_second) && !has_result {
        decorations.push(Decoration::full((176, 0), (28, 21), (99, 45)));
    }
    decorations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_repair_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 39);
        assert_eq!((slots[0].x, slots[0].y), (27, 47));
        assert_eq!((slots[1].x, slots[1].y), (76, 47));
        assert_eq!((slots[2].x, slots[2].y), (134, 47));
        assert_eq!((slots[3].x, slots[3].y), (8, 84));
    }

    fn state(
        properties: &[i16],
        has_result: bool,
        xp_level: i32,
        creative: bool,
    ) -> ScreenState<'_> {
        ScreenState {
            properties,
            filled: [true, true, has_result],
            xp_level,
            creative,
            ..ScreenState::default()
        }
    }

    #[test]
    fn an_affordable_cost_is_green() {
        assert_eq!(
            cost_line(&state(&[5], true, 5, false)),
            Some(CostLine {
                text: CostText::Cost(5),
                color: COST_GREEN,
            })
        );
    }

    #[test]
    fn a_cost_above_the_level_is_red_but_free_in_creative() {
        assert_eq!(
            cost_line(&state(&[5], true, 4, false)).map(|l| l.color),
            Some(COST_RED)
        );
        assert_eq!(
            cost_line(&state(&[5], true, 0, true)).map(|l| l.color),
            Some(COST_GREEN)
        );
    }

    #[test]
    fn forty_levels_are_too_expensive_in_survival_only() {
        assert_eq!(
            cost_line(&state(&[40], false, 99, false)).map(|l| l.text),
            Some(CostText::TooExpensive)
        );
        assert_eq!(
            cost_line(&state(&[40], true, 0, true)).map(|l| l.text),
            Some(CostText::Cost(40))
        );
    }

    #[test]
    fn no_cost_or_no_result_shows_nothing() {
        assert_eq!(cost_line(&state(&[0], true, 9, false)), None);
        assert_eq!(cost_line(&state(&[], true, 9, false)), None);
        assert_eq!(cost_line(&state(&[3], false, 9, false)), None);
    }

    #[test]
    fn the_name_field_dims_without_an_input_item() {
        assert_eq!(decorations(true, false, true)[0].src, (0, 166));
        assert_eq!(decorations(false, false, false)[0].src, (0, 182));
    }

    #[test]
    fn the_cross_shows_for_an_input_without_a_result() {
        assert_eq!(decorations(true, false, false).len(), 2);
        assert_eq!(decorations(false, true, false).len(), 2);
        assert_eq!(decorations(true, true, true).len(), 1);
        assert_eq!(decorations(false, false, false).len(), 1);
    }
}
