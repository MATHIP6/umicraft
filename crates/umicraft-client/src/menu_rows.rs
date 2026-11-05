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
        favicon: None,
    }
}

/// The protocol version of 1.8: a newer server makes the client out of date,
/// an older one (or none) the server (`ServerListEntryNormal`: `version > 47`).
const PROTOCOL: i32 = 47;

/// The row status a ping outcome shows (`ServerListEntryNormal`): a version
/// mismatch keeps the description and shows the server's version, an
/// unreachable server shows the failure text.
#[must_use]
pub fn row_status(outcome: &PingOutcome) -> RowStatus {
    match outcome {
        PingOutcome::Online(info) => online(info),
        PingOutcome::VersionMismatch(info) => mismatch(info),
        PingOutcome::Unreachable(_) => RowStatus::Unreachable,
    }
}

fn mismatch(info: &ServerInfo) -> RowStatus {
    RowStatus::VersionMismatch {
        motd: info.motd.clone(),
        version: info.version_name.clone(),
        client_outdated: info.protocol.unwrap_or(0) > PROTOCOL,
        players: info.players_online,
        sample: info.player_sample.clone(),
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
    fn a_mismatch_keeps_the_description_and_says_which_side_is_old() {
        let mismatch = |protocol| row_status(&PingOutcome::VersionMismatch(info(protocol)));
        let RowStatus::VersionMismatch {
            motd,
            version,
            client_outdated,
            ..
        } = mismatch(Some(107))
        else {
            panic!("a mismatch row");
        };
        assert_eq!((motd.as_str(), version.as_str()), ("Hello", "1.8.8"));
        assert!(client_outdated, "a newer server outdates the client");
        for older in [Some(5), None] {
            assert!(matches!(
                mismatch(older),
                RowStatus::VersionMismatch {
                    client_outdated: false,
                    ..
                }
            ));
        }
    }

    #[test]
    fn a_failure_maps_to_its_own_row() {
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
