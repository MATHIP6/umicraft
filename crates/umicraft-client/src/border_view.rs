// SPDX-License-Identifier: MIT OR Apache-2.0

//! Turns the server's world border into the plain data the renderer draws
//! (P7g, MCP `RenderGlobal.renderWorldBorder`).

use umicraft_game::server::world_border::Border;
use umicraft_render::{BorderStatus, BorderView};

/// The renderer's view of `border`. The status compares the target with the
/// current diameter, which has the sign of `endDiameter - startDiameter`
/// while a resize runs (`WorldBorder.getStatus`).
#[must_use]
pub fn border_view(border: &Border) -> BorderView {
    let diameter = border.diameter();
    BorderView {
        center: border.center(),
        diameter,
        portal_boundary: border.portal_boundary(),
        status: BorderStatus::of_resize(diameter, border.target_diameter()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::world_border::WorldBorder;

    #[test]
    fn a_default_border_is_stationary_and_huge() {
        let view = border_view(&Border::default());
        assert_eq!(view.status, BorderStatus::Stationary);
        assert_eq!(view.diameter, 6.0e7);
    }

    #[test]
    fn a_running_resize_picks_its_direction() {
        let mut border = Border::default();
        border.apply(&WorldBorder::LerpSize {
            old_diameter: 100.0,
            new_diameter: 50.0,
            speed: 10_000,
        });
        border.tick();
        let view = border_view(&border);
        assert_eq!(view.status, BorderStatus::Shrinking);
        assert!(view.diameter < 100.0 && view.diameter > 50.0);
    }
}
