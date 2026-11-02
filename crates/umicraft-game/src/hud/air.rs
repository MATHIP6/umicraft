// SPDX-License-Identifier: MIT OR Apache-2.0

//! The air bubbles (MCP `GuiIngame.renderPlayerStats`, section "air").

/// The air bubbles of a player whose head is in water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirView {
    /// Whole bubbles, drawn from the right edge inwards.
    pub full: u32,
    /// Bubbles that are popping: the one the next two ticks of air remove.
    pub popping: u32,
}

/// The bubbles for `air` ticks of breath (`Entity.getAir`, 300 when full).
///
/// `full = ceil((air - 2) * 10 / 300)` and the total is
/// `ceil(air * 10 / 300)`; the difference draws the popped bubble, so a
/// bubble shows its bursting sprite for the two ticks before it disappears. The caller shows the row only while the eyes are in water
/// (`isInsideOfMaterial(water)`).
///
/// ```
/// use umicraft_game::hud::air_view;
/// assert_eq!(air_view(300).full, 10);
/// ```
#[must_use]
pub fn air_view(air: i32) -> AirView {
    let full = bubbles(f64::from(air - 2));
    let total = bubbles(f64::from(air));
    AirView {
        full,
        popping: total.saturating_sub(full),
    }
}

/// `MathHelper.ceiling_double_int(value * 10 / 300)`, never negative.
fn bubbles(air: f64) -> u32 {
    (air * 10.0 / 300.0).ceil().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_air_shows_ten_whole_bubbles() {
        assert_eq!(
            air_view(300),
            AirView {
                full: 10,
                popping: 0
            }
        );
    }

    #[test]
    fn a_bubble_pops_for_two_ticks_before_it_goes() {
        let shown = |air| (air_view(air).full, air_view(air).popping);
        assert_eq!(shown(273), (10, 0));
        assert_eq!(shown(272), (9, 1));
        assert_eq!(shown(271), (9, 1));
        assert_eq!(shown(270), (9, 0));
        assert_eq!(shown(1), (0, 1));
    }

    #[test]
    fn drowning_air_shows_nothing() {
        assert_eq!(
            air_view(-20),
            AirView {
                full: 0,
                popping: 0
            }
        );
        assert_eq!(
            air_view(0),
            AirView {
                full: 0,
                popping: 0
            }
        );
    }
}
