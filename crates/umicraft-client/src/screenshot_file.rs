// SPDX-License-Identifier: MIT OR Apache-2.0

//! The files of the in-game screenshot key (`ScreenShotHelper`): a PNG in the
//! `screenshots` folder of the data directory, named after the time like
//! `2026-11-04_18.30.05.png`, with `_1`, `_2`... appended when the name is
//! taken.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ClientError;
use crate::screenshot::write_png;

/// The folder of the screenshots inside the data directory.
pub(crate) const FOLDER: &str = "screenshots";

/// Writes `rgba` as a new PNG in `folder` and returns its path.
///
/// # Errors
/// [`ClientError`] when the folder or the file cannot be written.
pub(crate) fn save(
    folder: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<PathBuf, ClientError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let path = free_path(folder, &file_stem(seconds));
    write_png(&path, rgba, width, height)?;
    Ok(path)
}

/// `<stem>.png`, or `<stem>_N.png` with the first `N` that is free
/// (`ScreenShotHelper.getTimestampedPNGFileForDirectory`).
fn free_path(folder: &Path, stem: &str) -> PathBuf {
    let first = folder.join(format!("{stem}.png"));
    if !first.exists() {
        return first;
    }
    (1..)
        .map(|n| folder.join(format!("{stem}_{n}.png")))
        .find(|path| !path.exists())
        .unwrap_or(first)
}

/// `yyyy-MM-dd_HH.mm.ss` of a Unix time. The reference uses the local zone;
/// std has no zone database, so the name is in UTC.
fn file_stem(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let rest = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}.{:02}.{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_the_vanilla_timestamp() {
        assert_eq!(file_stem(0), "1970-01-01_00.00.00");
        // 2026-11-04 18:30:05 UTC.
        assert_eq!(file_stem(1_793_817_005), "2026-11-04_18.30.05");
        // A leap day.
        assert_eq!(file_stem(951_782_400), "2000-02-29_00.00.00");
    }

    #[test]
    fn a_taken_name_gets_a_counter() {
        let folder = std::env::temp_dir().join(format!("umicraft-shots-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let first = free_path(&folder, "a");
        assert_eq!(first, folder.join("a.png"));
        std::fs::write(&first, b"x").unwrap();
        let second = free_path(&folder, "a");
        assert_eq!(second, folder.join("a_1.png"));
        std::fs::write(&second, b"x").unwrap();
        assert_eq!(free_path(&folder, "a"), folder.join("a_2.png"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn save_writes_a_png() {
        let folder =
            std::env::temp_dir().join(format!("umicraft-shot-save-{}", std::process::id()));
        let path = save(&folder, &[255; 16], 2, 2).unwrap();
        assert!(path.starts_with(&folder));
        assert!(std::fs::metadata(&path).unwrap().len() > 8);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
