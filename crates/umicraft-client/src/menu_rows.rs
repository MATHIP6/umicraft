// SPDX-License-Identifier: MIT OR Apache-2.0

//! The view models of the menu screens: the server list rows built from the
//! book and the ping results, and the statistics rows with their vanilla
//! number formats (ADR-0044).
//!
//! The screens in `umicraft_ui::menus` only draw rows; what a row says is
//! decided here, without a window, a socket or a language file, so it is
//! unit-tested.

use umicraft_ui::menus::{RowStatus, ServerRow, StatRow};

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

/// The rows of the general statistics, sorted by name.
///
/// Only names of the form `stat.<id>` are general statistics; the per-block,
/// per-item and per-mob counters (`stat.mineBlock.1`, ...) belong to tabs this
/// screen does not have, and `achievement.*` entries to the achievements
/// screen. The row key is the statistic name itself, which is also its
/// language key (`stat.jump`); an unknown key shows as itself.
#[must_use]
pub fn stat_rows(stats: &[(String, i32)]) -> Vec<StatRow> {
    let mut rows: Vec<StatRow> = stats
        .iter()
        .filter(|(name, _)| is_general(name))
        .map(|(name, value)| StatRow::new(name.as_str(), format_stat(name, *value)))
        .collect();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

fn is_general(name: &str) -> bool {
    name.strip_prefix("stat.")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains('.'))
}

/// A statistic value as vanilla formats it (`StatBase`): distances in
/// cm, m or km, times from ticks, damage in tenths, other counters as integers
/// with thousands separators.
#[must_use]
pub fn format_stat(name: &str, value: i32) -> String {
    match name {
        "stat.playOneMinute" | "stat.timeSinceDeath" => format_time(value),
        "stat.damageDealt" | "stat.damageTaken" => format!("{:.2}", f64::from(value) * 0.1),
        _ if name.ends_with("OneCm") => format_distance(value),
        _ => group_thousands(value),
    }
}

/// `StatBase.timeStatType`: 20 ticks per second, the largest unit above 0.5.
fn format_time(ticks: i32) -> String {
    let seconds = f64::from(ticks) / 20.0;
    let minutes = seconds / 60.0;
    let hours = minutes / 60.0;
    let days = hours / 24.0;
    let years = days / 365.0;
    for (amount, unit) in [(years, "y"), (days, "d"), (hours, "h"), (minutes, "m")] {
        if amount > 0.5 {
            return format!("{amount:.2} {unit}");
        }
    }
    format!("{seconds:?} s")
}

/// `StatBase.distanceStatType`: the value is in centimetres.
fn format_distance(centimetres: i32) -> String {
    let metres = f64::from(centimetres) / 100.0;
    let kilometres = metres / 1000.0;
    if kilometres > 0.5 {
        format!("{kilometres:.2} km")
    } else if metres > 0.5 {
        format!("{metres:.2} m")
    } else {
        format!("{centimetres} cm")
    }
}

/// `NumberFormat.getIntegerInstance(Locale.US)`: `1,234,567`.
fn group_thousands(value: i32) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if value < 0 {
        grouped.insert(0, '-');
    }
    grouped
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

    #[test]
    fn counters_are_grouped_like_the_reference() {
        assert_eq!(format_stat("stat.jump", 0), "0");
        assert_eq!(format_stat("stat.jump", 999), "999");
        assert_eq!(format_stat("stat.jump", 1_234_567), "1,234,567");
        assert_eq!(format_stat("stat.deaths", -1_000), "-1,000");
    }

    #[test]
    fn distances_pick_cm_m_or_km() {
        assert_eq!(format_stat("stat.walkOneCm", 40), "40 cm");
        assert_eq!(format_stat("stat.walkOneCm", 1_234), "12.34 m");
        assert_eq!(format_stat("stat.flyOneCm", 250_000), "2.50 km");
    }

    #[test]
    fn times_pick_the_largest_unit_above_half() {
        assert_eq!(format_stat("stat.playOneMinute", 100), "5.0 s");
        assert_eq!(format_stat("stat.playOneMinute", 20 * 90), "1.50 m");
        assert_eq!(format_stat("stat.timeSinceDeath", 20 * 3600 * 3), "3.00 h");
        assert_eq!(format_stat("stat.playOneMinute", 20 * 86_400 * 2), "2.00 d");
    }

    #[test]
    fn damage_is_counted_in_tenths_of_a_heart() {
        assert_eq!(format_stat("stat.damageDealt", 25), "2.50");
    }

    #[test]
    fn only_general_statistics_become_rows_in_name_order() {
        let stats = vec![
            ("stat.jump".to_string(), 7),
            ("stat.mineBlock.1".to_string(), 3),
            ("achievement.openInventory".to_string(), 1),
            ("stat.deaths".to_string(), 1),
        ];
        let rows = stat_rows(&stats);
        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, ["stat.deaths", "stat.jump"]);
        assert_eq!(rows[1].value, "7");
    }
}
