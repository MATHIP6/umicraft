// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enchanting table, after `ContainerEnchantment` / `GuiEnchantment`.
//!
//! The three option rows are drawn with their level cost; the text in the
//! standard galactic alphabet and the animated book are not (see
//! `docs/parity.md`).

use super::decoration::Decoration;
use super::interaction::{Dynamic, Hotspot, HotspotSpec, Label, ScreenState};
use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Left edge, width and height of an option row, and the distance between
/// two rows.
const ROW: (u32, u32, u32) = (60, 108, 19);
const ROW_TOP: u32 = 14;
const ROW_STEP: u32 = 19;
/// Right edge of the cost label (`j1 + 86`, with `j1 = 60 + 20`).
const COST_RIGHT: u32 = 166;
/// Colours of the cost: affordable, not affordable (`ColorFor`).
const AFFORDABLE: u32 = 0x80_FF_20;
const UNAFFORDABLE: u32 = 0x40_7F_10;

/// Builds the enchanting table screen: item, lapis, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![
        SlotSpec {
            index: 0,
            x: 15,
            y: 47,
        },
        SlotSpec {
            index: 1,
            x: 35,
            y: 47,
        },
    ];
    slots.extend(player_inventory_slots(2, 84));
    ScreenSpec::single("container/enchanting_table", (176, 166), slots)
}

/// The three option rows.
pub(super) fn hotspots() -> Vec<HotspotSpec> {
    (0..3)
        .map(|row| HotspotSpec {
            hotspot: Hotspot::Enchant(row),
            x: ROW.0,
            y: ROW_TOP + u32::from(row) * ROW_STEP,
            width: ROW.1,
            height: ROW.2,
        })
        .collect()
}

/// Whether row `row` (0 to 2) can be taken: it has a cost, the player has the
/// level for it and `row + 1` lapis (`GuiEnchantment.drawGuiContainerBackgroundLayer`).
/// Creative players pay nothing.
#[must_use]
pub fn row_available(state: &ScreenState<'_>, row: u8) -> bool {
    let cost = row_cost(state, row);
    cost > 0 && (state.creative || (state.lapis > row && state.xp_level >= i32::from(cost)))
}

/// The level cost the server announced for `row` (window property `row`).
fn row_cost(state: &ScreenState<'_>, row: u8) -> i16 {
    state
        .properties
        .get(usize::from(row))
        .copied()
        .unwrap_or(0)
        .max(0)
}

/// The option rows: an empty row stays dark; an affordable one lights up
/// under the pointer.
pub(super) fn dynamic(state: &ScreenState<'_>) -> Dynamic {
    let mut dynamic = Dynamic::default();
    for row in 0..3_u8 {
        let y = ROW_TOP + u32::from(row) * ROW_STEP;
        let cost = row_cost(state, row);
        let icon = u32::from(row) * 16;
        let available = row_available(state, row);
        let (background_v, icon_v) = if cost == 0 {
            (185, None)
        } else if !available {
            (185, Some(239))
        } else if state.hover == Some(Hotspot::Enchant(row)) {
            (204, Some(223))
        } else {
            (166, Some(223))
        };
        dynamic.decorations.push(Decoration::full(
            (0, background_v),
            (ROW.1, ROW.2),
            (ROW.0, y),
        ));
        if let Some(v) = icon_v {
            dynamic
                .decorations
                .push(Decoration::full((icon, v), (16, 16), (ROW.0 + 1, y + 1)));
            dynamic.labels.push(Label {
                text: cost.to_string(),
                right: COST_RIGHT,
                y: y + 2 + 7,
                color: if available { AFFORDABLE } else { UNAFFORDABLE },
            });
        }
    }
    dynamic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_enchantment_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 38);
        assert_eq!((slots[0].x, slots[0].y), (15, 47));
        assert_eq!((slots[1].x, slots[1].y), (35, 47));
        assert_eq!((slots[2].x, slots[2].y), (8, 84));
        assert_eq!((slots[37].x, slots[37].y), (152, 142));
    }

    fn state<'a>(properties: &'a [i16]) -> ScreenState<'a> {
        ScreenState {
            properties,
            lapis: 3,
            xp_level: 20,
            ..ScreenState::default()
        }
    }

    #[test]
    fn rows_sit_where_gui_enchantment_clicks() {
        let rows = hotspots();
        assert_eq!(
            (rows[0].x, rows[0].y, rows[0].width, rows[0].height),
            (60, 14, 108, 19)
        );
        assert_eq!(rows[2].y, 14 + 38);
    }

    #[test]
    fn a_row_needs_cost_level_and_lapis() {
        let props = [10, 30, 0];
        assert!(row_available(&state(&props), 0));
        assert!(!row_available(&state(&props), 1), "level 30 needed");
        assert!(!row_available(&state(&props), 2), "no offer");
        let mut one_lapis = state(&props);
        one_lapis.lapis = 1;
        one_lapis.xp_level = 40;
        assert!(row_available(&one_lapis, 0));
        assert!(!row_available(&one_lapis, 1), "row 2 costs 2 lapis");
    }

    #[test]
    fn creative_players_pay_nothing_but_still_need_an_offer() {
        let mut creative = state(&[50, 0, 0]);
        creative.lapis = 0;
        creative.xp_level = 0;
        creative.creative = true;
        assert!(row_available(&creative, 0));
        assert!(!row_available(&creative, 1));
    }

    #[test]
    fn rows_draw_the_vanilla_sprites() {
        let props = [10, 30, 0];
        let mut hovered = state(&props);
        hovered.hover = Some(Hotspot::Enchant(0));
        let d = dynamic(&hovered);
        // Row 0: lit by the pointer, with its coloured icon.
        assert_eq!(d.decorations[0].src, (0, 204));
        assert_eq!(d.decorations[1].src, (0, 223));
        // Row 1: dark with the greyed icon (16 * row, 239).
        assert_eq!(d.decorations[2].src, (0, 185));
        assert_eq!(d.decorations[3].src, (16, 239));
        // Row 2: nothing to show.
        assert_eq!(d.decorations[4].src, (0, 185));
        assert_eq!(d.decorations.len(), 5);
    }

    #[test]
    fn costs_are_labelled_green_or_dark_green() {
        let props = [10, 30, 0];
        let labels = dynamic(&state(&props)).labels;
        assert_eq!(labels.len(), 2);
        assert_eq!(
            (labels[0].text.as_str(), labels[0].color),
            ("10", AFFORDABLE)
        );
        assert_eq!(
            (labels[1].text.as_str(), labels[1].color),
            ("30", UNAFFORDABLE)
        );
    }
}
