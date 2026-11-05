// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title screen's splash text (MCP `GuiMainMenu`): a line of the user's
//! `texts/splashes.txt`, or one of three fixed greetings on their dates.
//!
//! The file is a Mojang asset read at runtime from the user's own jar and
//! never stored (spec §3). Without it the title screen keeps its original
//! list. The date is UTC; vanilla uses the local calendar.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::achievements_view::java_string_hash;

/// The jar entry of the splash list.
const SPLASHES_PATH: &str = "assets/minecraft/texts/splashes.txt";
/// `"".hashCode()`-style marker vanilla never shows (`125780783`).
const SKIPPED_HASH: i32 = 125_780_783;

/// The lines of `splashes.txt`: one splash per line, trimmed, blanks dropped
/// (`GuiMainMenu` trims and skips empty lines).
#[must_use]
pub fn parse(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// The date specials of `GuiMainMenu`, by `(month, day)`.
fn special(month: u32, day: u32) -> Option<&'static str> {
    match (month, day) {
        (12, 24) => Some("Merry X-mas!"),
        (1, 1) => Some("Happy new year!"),
        (10, 31) => Some("OOoooOOOoooo! Spooky!"),
        _ => None,
    }
}

/// The splash of a start: the date's greeting, else line `seed` of `lines`
/// (stepping over the one vanilla skips), else `None` for an empty list.
#[must_use]
pub fn choose(lines: &[String], (month, day): (u32, u32), seed: u64) -> Option<String> {
    if let Some(greeting) = special(month, day) {
        return Some(greeting.to_string());
    }
    let count = lines.len();
    (0..count)
        .map(|step| &lines[(seed as usize % count.max(1) + step) % count])
        .find(|line| java_string_hash(line) != SKIPPED_HASH)
        .cloned()
}

/// `(month, day)` of today in UTC (civil-from-days of the Unix day number).
#[must_use]
pub fn today() -> (u32, u32) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    month_day((seconds / 86_400) as i64)
}

/// `(month, day)` of the Unix day number `days`.
fn month_day(days: i64) -> (u32, u32) {
    let shifted = days + 719_468;
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (month as u32, day as u32)
}

/// The splash lines of the user's jar; empty without a jar or the file.
#[must_use]
pub fn load_lines() -> Vec<String> {
    let Ok(jar) = crate::official::find_verified_jar() else {
        return Vec::new();
    };
    jar.read_entry(SPLASHES_PATH)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_trimmed_and_blanks_dropped() {
        assert_eq!(parse("  Hello! \r\n\n World\n"), ["Hello!", "World"]);
    }

    #[test]
    fn the_three_dates_have_their_greeting() {
        let lines = vec!["x".to_string()];
        assert_eq!(choose(&lines, (12, 24), 0).as_deref(), Some("Merry X-mas!"));
        assert_eq!(
            choose(&lines, (1, 1), 0).as_deref(),
            Some("Happy new year!")
        );
        assert_eq!(
            choose(&lines, (10, 31), 0).as_deref(),
            Some("OOoooOOOoooo! Spooky!")
        );
        assert_eq!(choose(&lines, (6, 1), 0).as_deref(), Some("x"));
    }

    #[test]
    fn the_seed_picks_a_line_and_an_empty_list_none() {
        let lines: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choose(&lines, (6, 1), 4).as_deref(), Some("b"));
        assert_eq!(choose(&[], (6, 1), 4), None);
    }

    #[test]
    fn unix_days_become_dates() {
        assert_eq!(month_day(0), (1, 1));
        assert_eq!(month_day(19_723), (1, 1)); // 2024-01-01
        assert_eq!(month_day(19_723 + 365 - 1), (12, 30)); // leap year
        assert_eq!(month_day(20_027), (10, 31)); // 2024-10-31
    }
}
