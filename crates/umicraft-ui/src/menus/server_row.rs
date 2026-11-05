// SPDX-License-Identifier: MIT OR Apache-2.0

//! One row of the server list (MCP `ServerListEntryNormal`): the view model
//! the client fills from its ping results. How a row is drawn is in
//! [`super::row_draw`].
//!
//! A row is 32 GUI units high inside a 36-unit slot and 305 wide. Relative to
//! the row: the 32x32 icon at `(0, 0)`, the name at `(35, 1)`, up to two
//! description lines at `(35, 12)` and `(35, 21)` in `0x808080`, the
//! player count right-aligned ending 17 units from the right edge, and the
//! 10x8 latency icon at `(width - 15, 0)`.

use crate::draw::SpriteSheet;

/// Height of a row slot in GUI units (`ServerSelectionList` slot height).
pub(super) const SLOT_HEIGHT: f32 = 36.0;
/// Height of the drawn row, the slot minus its 4-unit gap.
pub(super) const ENTRY_HEIGHT: f32 = 32.0;
/// Width of a row (`ServerSelectionList.getListWidth`: 220 + 85).
pub(super) const ENTRY_WIDTH: f32 = 305.0;
/// The row's left edge relative to the screen centre (`-152 + 2`).
pub(super) const ENTRY_LEFT: f32 = -150.0;

/// The state of one server's ping, as the client maps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowStatus {
    /// The ping is under way.
    Pinging,
    /// The server answered and speaks our protocol.
    Online {
        /// The server description with its legacy `§` codes.
        motd: String,
        /// Players online.
        players: u32,
        /// Player slots.
        max: u32,
        /// The server's version name.
        version: String,
        /// The round trip in milliseconds.
        latency_ms: u32,
        /// A sample of player names for the tooltip (may carry `§` codes).
        sample: Vec<String>,
    },
    /// The ping failed (`Can't connect to server.`).
    Unreachable,
    /// The server answered with another protocol version: the row keeps the
    /// description, shows the server's version in dark red instead of the
    /// player count and a red X (`ServerListEntryNormal`, `flag2`).
    VersionMismatch {
        /// The server description with its legacy `§` codes.
        motd: String,
        /// The server's version name, drawn in dark red.
        version: String,
        /// True when the server is newer, so the client is out of date.
        client_outdated: bool,
        /// Players online, for the "and N more" tooltip line.
        players: u32,
        /// A sample of player names for the tooltip.
        sample: Vec<String>,
    },
}

/// A row of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerRow {
    /// The name the player gave the entry.
    pub name: String,
    /// The address (`host[:port]`).
    pub address: String,
    /// The ping state.
    pub status: RowStatus,
    /// The server's own 64x64 icon in the GUI atlas, when it sent one; the
    /// default `unknown_server.png` is drawn otherwise.
    pub favicon: Option<SpriteSheet>,
}

/// The latency bars level for `ms` (`ServerListEntryNormal.drawEntry`):
/// 0 is the best (under 150 ms) and 4 the worst (1000 ms and more).
#[must_use]
pub fn latency_level(ms: u32) -> u8 {
    match ms {
        0..=149 => 0,
        150..=299 => 1,
        300..=599 => 2,
        600..=999 => 3,
        _ => 4,
    }
}

/// The part of a row a pointer is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowZone {
    /// The play arrow of the icon: joins.
    Play,
    /// The up arrow: moves the entry up.
    Up,
    /// The down arrow: moves the entry down.
    Down,
    /// Anything else.
    Body,
}

/// The zone at `rel`, a GUI position relative to the row's corner
/// (`ServerListEntryNormal.mousePressed`).
pub(super) fn zone_at(rel: [f32; 2], can_up: bool, can_down: bool) -> RowZone {
    if rel[1] > ENTRY_HEIGHT || rel[0] >= ENTRY_HEIGHT {
        RowZone::Body
    } else if rel[0] > 16.0 {
        RowZone::Play
    } else if rel[1] < 16.0 && can_up {
        RowZone::Up
    } else if rel[1] > 16.0 && can_down {
        RowZone::Down
    } else {
        RowZone::Body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_thresholds_are_the_vanilla_ones() {
        let levels: Vec<u8> = [0, 149, 150, 299, 300, 599, 600, 999, 1000]
            .map(latency_level)
            .to_vec();
        assert_eq!(levels, [0, 0, 1, 1, 2, 2, 3, 3, 4]);
    }

    #[test]
    fn zones_follow_the_icon_halves() {
        assert_eq!(zone_at([20.0, 10.0], true, true), RowZone::Play);
        assert_eq!(zone_at([5.0, 5.0], true, true), RowZone::Up);
        assert_eq!(zone_at([5.0, 25.0], true, true), RowZone::Down);
        assert_eq!(zone_at([5.0, 5.0], false, true), RowZone::Body);
        assert_eq!(zone_at([5.0, 25.0], true, false), RowZone::Body);
        assert_eq!(zone_at([100.0, 10.0], true, true), RowZone::Body);
    }
}
