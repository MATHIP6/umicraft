// SPDX-License-Identifier: MIT OR Apache-2.0

//! The view models of the server list: the rows built from the book and the
//! ping results (ADR-0044). The statistics screen has its own in
//! [`crate::stats_view`].
//!
//! The screens in `umicraft_ui::menus` only draw rows; what a row says is
//! decided here, without a window, a socket or a language file, so it is
//! unit-tested.

use umicraft_ui::menus::{RowStatus, ServerRow};

use crate::servers::{PingOutcome, ServerEntry, ServerInfo};

/// The row of a server nobody has pinged yet.
#[must_use]
pub fn pinging_row(entry: &ServerEntry) -> ServerRow {
    ServerRow {
        name: entry.name().to_string(),
        address: entry.address().to_string(),
        status: RowStatus::Pinging,
    }
}

/// The row status a ping outcome shows (`ServerListEntryNormal`): a version
/// mismatch is greyed, an unreachable server shows the failure text.
#[must_use]
pub fn row_status(outcome: &PingOutcome) -> RowStatus {
    match outcome {
        PingOutcome::Online(info) => online(info),
        PingOutcome::VersionMismatch(_) => RowStatus::VersionMismatch,
        PingOutcome::Unreachable(_) => RowStatus::Unreachable,
    }
}

fn online(info: &ServerInfo) -> RowStatus {
    RowStatus::Online {
        motd: info.motd.clone(),
        players: info.players_online,
        max: info.players_max,
        version: info.version_name.clone(),
        latency_ms: info.latency_ms,
        sample: info.player_sample.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(protocol: Option<i32>) -> Box<ServerInfo> {
        Box::new(ServerInfo {
            motd: "Hello".to_string(),
            players_online: 3,
            players_max: 20,
            version_name: "1.8.8".to_string(),
            protocol,
            latency_ms: 42,
            favicon_png: None,
            player_sample: vec!["Alex".to_string()],
        })
    }

    #[test]
    fn an_online_server_shows_its_status() {
        let status = row_status(&PingOutcome::Online(info(Some(47))));
        assert_eq!(
            status,
            RowStatus::Online {
                motd: "Hello".to_string(),
                players: 3,
                max: 20,
                version: "1.8.8".to_string(),
                latency_ms: 42,
                sample: vec!["Alex".to_string()],
            }
        );
    }

    #[test]
    fn a_mismatch_and_a_failure_map_to_their_own_rows() {
        assert_eq!(
            row_status(&PingOutcome::VersionMismatch(info(Some(5)))),
            RowStatus::VersionMismatch
        );
        assert_eq!(
            row_status(&PingOutcome::Unreachable("timed out".to_string())),
            RowStatus::Unreachable
        );
    }

    #[test]
    fn a_new_row_is_pinging() {
        let entry = ServerEntry::new("", "play.example.net:25570").expect("valid");
        let row = pinging_row(&entry);
        assert_eq!(row.name, "Minecraft Server");
        assert_eq!(row.address, "play.example.net:25570");
        assert_eq!(row.status, RowStatus::Pinging);
    }
}
