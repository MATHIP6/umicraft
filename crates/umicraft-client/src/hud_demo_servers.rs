// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scripted server list of `--hud-demo --hud-view servers`: one row of
//! each look the list can take, so the screen can be compared with vanilla
//! (ADR-0047).
//!
//! The demo opens no socket. The rows get the results a ping would have
//! produced: an unreachable server, an online one with a plain description,
//! one still pinging, a full one with a coloured accented description and an
//! icon of its own, and a version mismatch.

use umicraft_assets::DynamicImage;
use umicraft_ui::menus::RowStatus;

use crate::menu_flow::MenuFlow;
use crate::servers::ICON_SIDE;

/// The entries of the demo book, as `(name, address)`.
pub(crate) const ENTRIES: [(&str, &str); 5] = [
    ("Minecraft Server", "down.example.net"),
    ("Minecraft Server", "play.example.net"),
    ("Minecraft Server", "slow.example.net"),
    ("Minecraft Server", "prisme.example.net"),
    ("Minecraft Server", "old.example.net"),
];

/// The row that has its own icon.
const ICON_ROW: usize = 3;

/// The description of the full server: magenta name with an accent, a grey
/// tag, then an orange second line (the case that showed `B?ta`).
const PRISME_MOTD: &str =
    "\u{a7}dPrisme B\u{ea}ta \u{a7}f- Serveur Practice \u{a7}8[1.8+]\n\u{a7}6Nouvelle caisse d'Halloween !";

/// Gives the rows of `flow` the scripted ping results.
pub(crate) fn script_pings(flow: &mut MenuFlow) {
    flow.set_row_status(0, RowStatus::Unreachable);
    flow.set_row_status(
        1,
        RowStatus::Online {
            motd: "A Minecraft Server".to_string(),
            players: 0,
            max: 20,
            version: "1.8.8".to_string(),
            latency_ms: 40,
            sample: Vec::new(),
        },
    );
    flow.set_row_status(2, RowStatus::Pinging);
    flow.set_row_status(
        ICON_ROW,
        RowStatus::Online {
            motd: PRISME_MOTD.to_string(),
            players: 3,
            max: 500,
            version: "1.8.8".to_string(),
            latency_ms: 35,
            sample: vec!["Steve".to_string(), "\u{a7}aAlex".to_string()],
        },
    );
    flow.set_row_status(
        4,
        RowStatus::VersionMismatch {
            motd: "\u{a7}cA newer server".to_string(),
            version: "1.12.2".to_string(),
            client_outdated: true,
            players: 7,
            sample: Vec::new(),
        },
    );
}

/// The generated 64x64 icon of the full server: a pink cube on a dark
/// background (no Mojang or server artwork).
#[must_use]
pub(crate) fn icon_images() -> Vec<DynamicImage> {
    let side = ICON_SIDE as usize;
    let mut pixels = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let inside = (14..50).contains(&x) && (14..50).contains(&y);
            let shade = (150 + (x + y) as u32 * 100 / (2 * side as u32)) as u8;
            pixels.extend_from_slice(&if inside {
                [shade, 40, shade, 255]
            } else {
                [30, 20, 40, 255]
            });
        }
    }
    vec![DynamicImage {
        name: format!("server{ICON_ROW}"),
        width: ICON_SIDE,
        height: ICON_SIDE,
        pixels,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_full_64_square_for_the_icon_row() {
        let images = icon_images();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].name, "server3");
        assert_eq!(images[0].pixels.len(), 64 * 64 * 4);
    }
}
