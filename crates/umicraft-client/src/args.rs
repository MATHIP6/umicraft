// SPDX-License-Identifier: MIT OR Apache-2.0

//! Command-line arguments for `umicraft-client`.
//!
//! Windowed mode is the default; passing `--screenshot PATH` switches to the
//! headless offscreen path that collects world data for `--collect-secs` and
//! writes one PNG.

use std::path::PathBuf;
use std::time::Duration;

/// Usage text shown by `--help` and on argument errors.
pub const USAGE: &str = "\
usage: umicraft-client <host> <port> [options]
  <host> <port>       Minecraft 1.8 server (protocol 47)

options:
  --name NAME         offline username (default: umicraft)
  --view-distance N   requested chunk view distance, 2-16 (default: 8)
  --screenshot PATH   render one offscreen PNG to PATH instead of opening a window
  --collect-secs N    world collection time for screenshots (default: 8)
  --width N           framebuffer width (default: 1280)
  --height N          framebuffer height (default: 720)
  --help, -h          print this help";

/// Parsed client options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// Server host name or IP.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Offline-mode username.
    pub name: String,
    /// Requested chunk view distance.
    pub view_distance: u8,
    /// Offscreen output path; `None` opens a window.
    pub screenshot: Option<PathBuf>,
    /// How long to collect chunks before a screenshot.
    pub collect: Duration,
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
}

impl Args {
    /// Parses argv-style words (without the program name).
    ///
    /// # Errors
    /// A human-readable message including [`USAGE`] for missing values,
    /// unknown flags, or out-of-range numbers.
    pub fn parse(words: &[String]) -> Result<Self, String> {
        let help = || format!("missing/invalid arguments\n{USAGE}");
        let mut host = None;
        let mut port = None;
        let mut name = "umicraft".to_string();
        let mut view_distance = 8_u8;
        let mut screenshot = None;
        let mut collect_secs = 8.0_f64;
        let mut width = 1280_u32;
        let mut height = 720_u32;

        let mut index = 0;
        while index < words.len() {
            match words[index].as_str() {
                "--help" | "-h" => return Err(USAGE.to_string()),
                "--name" => {
                    name = words.get(index + 1).ok_or_else(help)?.clone();
                    index += 2;
                }
                "--view-distance" => {
                    view_distance = parse_number(words.get(index + 1), "--view-distance")?;
                    index += 2;
                }
                "--screenshot" => {
                    let value = words.get(index + 1).ok_or_else(help)?;
                    if value.is_empty() {
                        return Err(format!("--screenshot needs a path\n{USAGE}"));
                    }
                    screenshot = Some(PathBuf::from(value));
                    index += 2;
                }
                "--collect-secs" => {
                    collect_secs = parse_number(words.get(index + 1), "--collect-secs")?;
                    index += 2;
                }
                "--width" => {
                    width = parse_number(words.get(index + 1), "--width")?;
                    index += 2;
                }
                "--height" => {
                    height = parse_number(words.get(index + 1), "--height")?;
                    index += 2;
                }
                other if host.is_none() => {
                    host = Some(other.to_string());
                    index += 1;
                }
                other if port.is_none() => {
                    port =
                        Some(other.parse().map_err(|_| {
                            format!("port must be 1-65535, got {other:?}\n{USAGE}")
                        })?);
                    index += 1;
                }
                other => return Err(format!("unexpected argument {other:?}\n{USAGE}")),
            }
        }

        if port == Some(0) {
            return Err(format!("port must be 1-65535\n{USAGE}"));
        }
        if !(2..=16).contains(&view_distance) {
            return Err(format!(
                "--view-distance must be 2-16, got {view_distance}\n{USAGE}"
            ));
        }
        if !(0.2..=300.0).contains(&collect_secs) {
            return Err(format!(
                "--collect-secs must be 0.2-300, got {collect_secs}\n{USAGE}"
            ));
        }
        if !(64..=7680).contains(&width) || !(64..=4320).contains(&height) {
            return Err(format!(
                "framebuffer must be 64-7680 × 64-4320, got {width}×{height}\n{USAGE}"
            ));
        }

        Ok(Self {
            host: host.ok_or_else(help)?,
            port: port.ok_or_else(help)?,
            name,
            view_distance,
            screenshot,
            collect: Duration::from_secs_f64(collect_secs),
            width,
            height,
        })
    }
}

fn parse_number<T>(value: Option<&String>, flag: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    value
        .ok_or_else(|| format!("{flag} expects a value\n{USAGE}"))?
        .parse()
        .map_err(|_| format!("{flag} expects a number\n{USAGE}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_windowed_defaults() {
        let args = Args::parse(&words(&["localhost", "25565"])).expect("valid args");
        assert_eq!(args.host, "localhost");
        assert_eq!(args.port, 25565);
        assert_eq!(args.name, "umicraft");
        assert_eq!(args.view_distance, 8);
        assert!(args.screenshot.is_none());
        assert_eq!(args.width, 1280);
        assert_eq!(args.height, 720);
    }

    #[test]
    fn parses_screenshot_options() {
        let args = Args::parse(&words(&[
            "127.0.0.1",
            "25599",
            "--name",
            "Pixel",
            "--view-distance",
            "12",
            "--screenshot",
            "/tmp/umicraft.png",
            "--collect-secs",
            "3.5",
            "--width",
            "1920",
            "--height",
            "1080",
        ]))
        .expect("valid args");
        assert_eq!(args.name, "Pixel");
        assert_eq!(args.view_distance, 12);
        assert_eq!(
            args.screenshot.as_deref(),
            Some(std::path::Path::new("/tmp/umicraft.png"))
        );
        assert_eq!(args.collect, Duration::from_millis(3500));
        assert_eq!((args.width, args.height), (1920, 1080));
    }

    #[test]
    fn rejects_bad_values() {
        assert!(Args::parse(&words(&[])).is_err());
        assert!(Args::parse(&words(&["host", "0"])).is_err());
        assert!(Args::parse(&words(&["host", "70000"])).is_err());
        assert!(Args::parse(&words(&["host", "25565", "--view-distance", "1"])).is_err());
        assert!(Args::parse(&words(&["host", "25565", "--width", "0"])).is_err());
        assert!(Args::parse(&words(&["host", "25565", "--unknown"])).is_err());
        assert!(Args::parse(&words(&["host", "25565", "--name"])).is_err());
    }
}
