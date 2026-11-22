// SPDX-License-Identifier: MIT OR Apache-2.0

//! The villager trading screen, after `ContainerMerchant` / `GuiMerchant`.
//!
//! The offer on display (items and arrows) is drawn by the client from the
//! trade list; this file holds the slots and the two arrow buttons.

use super::decoration::Decoration;
use super::interaction::{Dynamic, Hotspot, HotspotSpec, ScreenState};
use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Width and height of an arrow button.
const ARROW: (u32, u32) = (12, 19);
/// Where the offer preview puts its items: the first and second wanted item
/// and the result (`GuiMerchant.drawScreen`).
pub const OFFER_ITEMS: [(u32, u32); 3] = [(36, 24), (62, 24), (120, 24)];
/// Size of the cross over a used-up offer, from (212, 0) of the texture.
const CROSS: (u32, u32) = (28, 21);
/// Where `GuiMerchant` draws the two crosses of a used-up offer: over the
/// arrow of the preview and over the arrow of the trade slots. Hovering them
/// shows `merchant.deprecated`.
pub const DISABLED_CROSSES: [(u32, u32, u32, u32); 2] =
    [(83, 21, CROSS.0, CROSS.1), (83, 51, CROSS.0, CROSS.1)];

/// Builds the villager screen: two buy slots, the result, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots: Vec<SlotSpec> = [(36, 53), (62, 53), (120, 53)]
        .iter()
        .enumerate()
        .map(|(index, &(x, y))| SlotSpec { index, x, y })
        .collect();
    slots.extend(player_inventory_slots(3, 84));
    ScreenSpec::single("container/villager", (176, 166), slots)
}

/// The two arrows: next at (147, 23), previous at (17, 23).
pub(super) fn hotspots() -> Vec<HotspotSpec> {
    [(Hotspot::TradeNext, 147), (Hotspot::TradePrevious, 17)]
        .into_iter()
        .map(|(hotspot, x)| HotspotSpec {
            hotspot,
            x,
            y: 23,
            width: ARROW.0,
            height: ARROW.1,
        })
        .collect()
}

/// The arrows, greyed out when there is no offer in that direction, and the
/// crosses of a used-up offer (`drawGuiContainerBackgroundLayer`).
pub(super) fn dynamic(state: &ScreenState<'_>) -> Dynamic {
    let mut decorations: Vec<Decoration> = hotspots()
        .into_iter()
        .map(|spec| {
            let enabled = match spec.hotspot {
                Hotspot::TradeNext => state.trade_next,
                _ => state.trade_previous,
            };
            let column = if !enabled {
                2
            } else if state.hover == Some(spec.hotspot) {
                1
            } else {
                0
            };
            let row = if spec.hotspot == Hotspot::TradeNext {
                0
            } else {
                ARROW.1
            };
            Decoration::full((176 + column * ARROW.0, row), ARROW, (spec.x, spec.y))
        })
        .collect();
    if state.trade_disabled {
        decorations.extend(
            DISABLED_CROSSES
                .iter()
                .map(|&(x, y, w, h)| Decoration::full((212, 0), (w, h), (x, y))),
        );
    }
    Dynamic::sprites(decorations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_merchant_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 39);
        assert_eq!((slots[0].x, slots[0].y), (36, 53));
        assert_eq!((slots[1].x, slots[1].y), (62, 53));
        assert_eq!((slots[2].x, slots[2].y), (120, 53));
        assert_eq!((slots[3].x, slots[3].y), (8, 84));
    }

    #[test]
    fn arrows_use_the_merchant_button_sprites() {
        let both = ScreenState {
            trade_previous: true,
            trade_next: true,
            ..ScreenState::default()
        };
        let sprites = dynamic(&both).decorations;
        // Next is the top row, previous the bottom row of the sheet.
        assert_eq!((sprites[0].src, sprites[0].dst), ((176, 0), (147, 23)));
        assert_eq!((sprites[1].src, sprites[1].dst), ((176, 19), (17, 23)));
    }

    #[test]
    fn a_missing_offer_greys_the_arrow_and_hover_lights_it() {
        let state = ScreenState {
            trade_next: true,
            hover: Some(Hotspot::TradeNext),
            ..ScreenState::default()
        };
        let sprites = dynamic(&state).decorations;
        assert_eq!(sprites[0].src.0, 176 + 12, "hovered");
        assert_eq!(sprites[1].src.0, 176 + 24, "no previous offer");
    }

    #[test]
    fn a_used_up_offer_is_crossed_out_twice() {
        let state = ScreenState {
            trade_disabled: true,
            ..ScreenState::default()
        };
        let crosses: Vec<_> = dynamic(&state)
            .decorations
            .into_iter()
            .filter(|d| d.src == (212, 0))
            .map(|d| d.dst)
            .collect();
        assert_eq!(crosses, [(83, 21), (83, 51)]);
        assert_eq!(dynamic(&ScreenState::default()).decorations.len(), 2);
    }
}
