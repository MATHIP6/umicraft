// SPDX-License-Identifier: MIT OR Apache-2.0

//! Command-line arguments for `umicraft-client`.
//!
//! Five modes are available: the windowed client (the default), the headless
//! screenshot path (`--screenshot PATH`), the headless scripted session
//! (`--headless`) that spec §11 wants for the integration suite, the
//! server-less UI acceptance run (`--ui-demo`), and the server-less M5/M6 block
//! acceptance run (`--block-demo`).

use std::path::PathBuf;
use std::time::Duration;

/// The acceptance camera views for `--block-demo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoView {
    /// The existing four-row camera plus the new M6 wing.
    Overview,
    /// Ground/AO/water at noon.
    Day,
    /// Same camera at midnight.
    Night,
    /// Inside the cave looking at the glowstone falloff.
    Cave,
    /// Close-up of water/glass/leaves against stone.
    Transparency,
    /// View of entities from behind.
    Back,
    /// Close-up of player dual-layer clothing (Steve & Alex).
    Closeup,
    /// Walk cycle animation decomposition showing leg/arm swinging phases.
    WalkCycle,
    /// Exhibition lineup of all Minecraft 1.8 mobs.
    Mobs,
}

impl DemoView {
    /// Parses a view name, returning an error for unknown values.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "overview" => Ok(Self::Overview),
            "day" => Ok(Self::Day),
            "night" => Ok(Self::Night),
            "cave" => Ok(Self::Cave),
            "transparency" => Ok(Self::Transparency),
            "back" => Ok(Self::Back),
            "closeup" => Ok(Self::Closeup),
            "walk-cycle" | "walk" => Ok(Self::WalkCycle),
            "mobs" | "all" | "all-mobs" => Ok(Self::Mobs),
            other => Err(format!(
                "unknown demo view {other:?}; must be one of overview, day, night, cave, transparency, back, closeup, walk-cycle, mobs"
            )),
        }
    }

    /// Returns the default world time in ticks for this view.
    pub fn default_time(self) -> i64 {
        match self {
            Self::Overview | Self::Back | Self::Closeup | Self::WalkCycle | Self::Mobs => 6000,
            Self::Day => 6000,
            Self::Night => 18000,
            Self::Cave => 6000,
            Self::Transparency => 6000,
        }
    }

    /// Returns the view name as a static string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Day => "day",
            Self::Night => "night",
            Self::Cave => "cave",
            Self::Transparency => "transparency",
            Self::Back => "back",
            Self::Closeup => "closeup",
            Self::WalkCycle => "walk-cycle",
            Self::Mobs => "mobs",
        }
    }
}

/// Usage text shown by `--help` and on argument errors.
pub const USAGE: &str = "\
usage: umicraft-client <host> <port> [options]
       umicraft-client --ui-demo [--screenshot PATH] [options]
       umicraft-client --block-demo [--screenshot PATH] [options]
       umicraft-client --entity-demo [--screenshot PATH] [options]
  <host> <port>       Minecraft 1.8 server (protocol 47); not needed with
                      --ui-demo, --block-demo or --entity-demo

options:
  --name NAME         offline username (default: umicraft)
  --view-distance N   requested chunk view distance, 2-16 (default: 8)
  --screenshot PATH   render one offscreen PNG to PATH instead of opening a window
  --collect-secs N    world collection time for screenshots (default: 8)
  --headless          run a scripted session without a window and print a report
  --play-secs N       duration of the headless session (default: 60)
  --script NAME       headless script: default (walk, break, place) or idle
  --chat MESSAGE      chat line the headless session sends when it is placed
  --ui-demo           drive the scripted M4 debug screen without a server or GPU;
                      combine with --screenshot to render the same screen offscreen
  --block-demo        mesh and report the M5/M6 acceptance blocks from the official
                      1.8.8 jar; no server or GPU unless --screenshot is given
  --entity-demo       mesh and report the M8 acceptance entities (Steve, Alex,
                      Zombie, Cow, Item); no server or GPU unless --screenshot is given
  --resource-pack PATH path to a resource pack directory or .zip (repeatable)
  --setup-assets      download and verify official 1.8.8 assets then exit
  --time N            world time in ticks (signed; default: 6000 noon)
  --demo-view NAME    acceptance camera: overview, day, night, cave, transparency,
                      back, closeup, walk-cycle, mobs (default: overview)
  --width N           framebuffer width (default: 1280)
  --height N          framebuffer height (default: 720)
  --help, -h          print this help";

/// The scripted sessions the headless mode knows.
pub const SCRIPTS: [&str; 2] = ["default", "idle"];

/// The top-level mode selected by the parsed arguments.
///
/// [`Args::mode`] fixes the dispatch precedence: a UI demo wins over the block
/// demo, which wins over the entity demo, which wins over the headless session,
/// which wins over the terrain screenshot, which wins over the windowed client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The interactive windowed client.
    Windowed,
    /// The offscreen terrain screenshot.
    Screenshot,
    /// The scripted headless session.
    Headless,
    /// The server-less scripted UI acceptance run.
    UiDemo,
    /// The server-less M5 block-model acceptance run.
    BlockDemo,
    /// The server-less M8 entity acceptance run.
    EntityDemo,
    /// First-run download and verification of official assets.
    SetupAssets,
}

/// Parsed client options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// Server host name or IP; `None` in the server-less UI demo mode.
    pub host: Option<String>,
    /// Server port; `None` in the server-less UI demo mode.
    pub port: Option<u16>,
    /// Offline-mode username.
    pub name: String,
    /// Requested chunk view distance.
    pub view_distance: u8,
    /// Offscreen output path; `None` opens a window.
    pub screenshot: Option<PathBuf>,
    /// How long to collect chunks before a screenshot.
    pub collect: Duration,
    /// Whether the scripted headless session is requested.
    pub headless: bool,
    /// How long the headless session plays.
    pub play: Duration,
    /// The scripted session of the headless mode.
    pub script: String,
    /// A chat line the headless session sends once the player is placed.
    pub chat: Option<String>,
    /// Whether the server-less scripted UI acceptance run is requested.
    pub ui_demo: bool,
    /// Whether the server-less M5/M6 block acceptance run is requested.
    pub block_demo: bool,
    /// Whether the server-less M8 entity acceptance run is requested.
    pub entity_demo: bool,
    /// Whether asset setup/download was explicitly requested.
    pub setup_assets: bool,
    /// Active resource pack paths, layered in argument order.
    pub resource_packs: Vec<PathBuf>,
    /// World time in ticks (signed; default: 6000 noon). Only used by `--block-demo`.
    pub time: i64,
    /// Acceptance camera view for `--block-demo`.
    pub demo_view: DemoView,
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
}

impl Args {
    /// The server endpoint, when this mode talks to a server.
    ///
    /// A server-less demo without positional arguments has no endpoint; every
    /// other mode is rejected by [`Self::parse`] before it reaches this method.
    #[must_use]
    pub fn server(&self) -> Option<(&str, u16)> {
        match (self.host.as_deref(), self.port) {
            (Some(host), Some(port)) => Some((host, port)),
            _ => None,
        }
    }

    /// The dispatch mode of these arguments.
    ///
    /// `--ui-demo` wins over `--block-demo`; the block demo wins over
    /// `--entity-demo`; the entity demo wins over `--headless` and `--screenshot`;
    /// `--headless` wins over `--screenshot`.
    #[must_use]
    pub fn mode(&self) -> Mode {
        if self.setup_assets {
            Mode::SetupAssets
        } else if self.ui_demo {
            Mode::UiDemo
        } else if self.block_demo {
            Mode::BlockDemo
        } else if self.entity_demo {
            Mode::EntityDemo
        } else if self.headless {
            Mode::Headless
        } else if self.screenshot.is_some() {
            Mode::Screenshot
        } else {
            Mode::Windowed
        }
    }

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
        let mut headless = false;
        let mut play_secs = 60.0_f64;
        let mut script = "default".to_string();
        let mut chat = None;
        let mut ui_demo = false;
        let mut block_demo = false;
        let mut entity_demo = false;
        let mut setup_assets = false;
        let mut resource_packs = Vec::new();
        let mut time = 6000_i64;
        let mut demo_view = DemoView::Overview;
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
                "--headless" => {
                    headless = true;
                    index += 1;
                }
                "--play-secs" => {
                    play_secs = parse_number(words.get(index + 1), "--play-secs")?;
                    index += 2;
                }
                "--script" => {
                    let value = words.get(index + 1).ok_or_else(help)?;
                    if !SCRIPTS.contains(&value.as_str()) {
                        return Err(format!(
                            "--script must be one of {SCRIPTS:?}, got {value:?}\n{USAGE}"
                        ));
                    }
                    script = value.clone();
                    index += 2;
                }
                "--chat" => {
                    let value = words.get(index + 1).ok_or_else(help)?;
                    chat = Some(value.clone());
                    index += 2;
                }
                "--ui-demo" => {
                    ui_demo = true;
                    index += 1;
                }
                "--block-demo" => {
                    block_demo = true;
                    index += 1;
                }
                "--entity-demo" => {
                    entity_demo = true;
                    index += 1;
                }
                "--setup-assets" => {
                    setup_assets = true;
                    index += 1;
                }
                "--resource-pack" => {
                    let value = words.get(index + 1).ok_or_else(help)?;
                    if value.is_empty() {
                        return Err(format!("--resource-pack needs a path\n{USAGE}"));
                    }
                    resource_packs.push(PathBuf::from(value));
                    index += 2;
                }
                "--time" => {
                    time = parse_number(words.get(index + 1), "--time")?;
                    index += 2;
                }
                "--demo-view" => {
                    let value = words.get(index + 1).ok_or_else(help)?;
                    demo_view = DemoView::parse(value)?;
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
        if !(1.0..=3600.0).contains(&play_secs) {
            return Err(format!(
                "--play-secs must be 1-3600, got {play_secs}\n{USAGE}"
            ));
        }
        if !(64..=7680).contains(&width) || !(64..=4320).contains(&height) {
            return Err(format!(
                "framebuffer must be 64-7680 × 64-4320, got {width}×{height}\n{USAGE}"
            ));
        }

        // The UI, block demos, entity demo, and setup-assets mode can run without a
        // server. Every other mode keeps the M1-M3 contract of an explicit
        // host and port.
        let (host, port) = if ui_demo || block_demo || entity_demo || setup_assets {
            (host, port)
        } else {
            (Some(host.ok_or_else(help)?), Some(port.ok_or_else(help)?))
        };

        Ok(Self {
            host,
            port,
            name,
            view_distance,
            screenshot,
            collect: Duration::from_secs_f64(collect_secs),
            headless,
            play: Duration::from_secs_f64(play_secs),
            script,
            chat,
            ui_demo,
            block_demo,
            entity_demo,
            setup_assets,
            resource_packs,
            time,
            demo_view,
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
        assert_eq!(args.host.as_deref(), Some("localhost"));
        assert_eq!(args.port, Some(25565));
        assert_eq!(args.name, "umicraft");
        assert_eq!(args.view_distance, 8);
        assert!(args.screenshot.is_none());
        assert!(!args.headless);
        assert!(!args.ui_demo);
        assert!(!args.block_demo);
        assert_eq!(args.play, Duration::from_secs(60));
        assert_eq!(args.script, "default");
        assert_eq!(args.width, 1280);
        assert_eq!(args.height, 720);
        assert_eq!(args.mode(), Mode::Windowed);
        assert_eq!(args.server(), Some(("localhost", 25565)));
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
    fn parses_the_headless_options() {
        let args = Args::parse(&words(&[
            "127.0.0.1",
            "25599",
            "--headless",
            "--play-secs",
            "90",
            "--script",
            "idle",
        ]))
        .expect("valid args");
        assert!(args.headless);
        assert_eq!(args.play, Duration::from_secs(90));
        assert_eq!(args.script, "idle");
        assert_eq!(args.mode(), Mode::Headless);
    }

    #[test]
    fn parses_ui_demo_without_a_server() {
        let args = Args::parse(&words(&["--ui-demo"])).expect("the ui demo needs no server");
        assert!(args.ui_demo);
        assert_eq!(args.host, None);
        assert_eq!(args.port, None);
        assert_eq!(args.server(), None);
        assert_eq!(args.mode(), Mode::UiDemo);
        assert_eq!((args.width, args.height), (1280, 720));
    }

    #[test]
    fn parses_ui_demo_with_a_screenshot() {
        let args = Args::parse(&words(&[
            "--ui-demo",
            "--screenshot",
            "/tmp/m4-ui.png",
            "--width",
            "800",
            "--height",
            "600",
        ]))
        .expect("the ui demo screenshot needs no server");
        assert_eq!(args.mode(), Mode::UiDemo);
        assert_eq!(
            args.screenshot.as_deref(),
            Some(std::path::Path::new("/tmp/m4-ui.png"))
        );
        assert_eq!((args.width, args.height), (800, 600));
        assert_eq!(args.server(), None);
    }

    #[test]
    fn parses_the_serverless_block_demo() {
        let args = Args::parse(&words(&[
            "--block-demo",
            "--screenshot",
            "/tmp/m5-blocks.png",
        ]))
        .expect("the block demo needs no server");
        assert!(args.block_demo);
        assert!(!args.ui_demo);
        assert_eq!(args.host, None);
        assert_eq!(args.port, None);
        assert_eq!(args.server(), None);
        assert_eq!(args.mode(), Mode::BlockDemo);
        assert_eq!(
            args.screenshot.as_deref(),
            Some(std::path::Path::new("/tmp/m5-blocks.png"))
        );
    }

    #[test]
    fn the_demo_precedence_is_ui_then_blocks_then_headless() {
        let ui = Args::parse(&words(&["--ui-demo", "--block-demo", "--headless"]))
            .expect("both demos parse together");
        assert_eq!(ui.mode(), Mode::UiDemo);

        let blocks = Args::parse(&words(&["--block-demo", "--headless", "--screenshot", "x"]))
            .expect("the block demo accepts the other mode flags");
        assert_eq!(blocks.mode(), Mode::BlockDemo);
    }

    #[test]
    fn ui_demo_wins_over_the_other_serverless_modes() {
        let args = Args::parse(&words(&[
            "--ui-demo",
            "--headless",
            "--screenshot",
            "/tmp/out.png",
        ]))
        .expect("ui demo accepts the other mode flags");
        assert_eq!(args.mode(), Mode::UiDemo);

        let headless = Args::parse(&words(&[
            "host",
            "25565",
            "--headless",
            "--screenshot",
            "x",
        ]))
        .expect("headless still parses");
        assert_eq!(headless.mode(), Mode::Headless);
    }

    #[test]
    fn the_serverless_modes_still_require_a_server() {
        for case in [
            vec!["--headless"],
            vec!["--screenshot", "/tmp/out.png"],
            vec!["host"],
            vec![],
        ] {
            assert!(
                Args::parse(&words(&case)).is_err(),
                "the normal modes must reject {case:?}"
            );
        }
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
        assert!(Args::parse(&words(&["host", "25565", "--play-secs", "0"])).is_err());
        assert!(Args::parse(&words(&["host", "25565", "--script", "dance"])).is_err());
        assert!(Args::parse(&words(&["--ui-demo", "--width", "0"])).is_err());
        assert!(Args::parse(&words(&["--block-demo", "--width", "0"])).is_err());
        // New M6 arguments
        assert!(Args::parse(&words(&["--block-demo", "--time", "not-a-number"])).is_err());
        assert!(Args::parse(&words(&["--block-demo", "--demo-view", "unknown"])).is_err());
    }

    #[test]
    fn parses_block_demo_with_time_and_view() {
        let args = Args::parse(&words(&[
            "--block-demo",
            "--time",
            "-6000",
            "--demo-view",
            "cave",
            "--screenshot",
            "/tmp/m6-cave.png",
        ]))
        .expect("block demo with time and view");
        assert!(args.block_demo);
        assert_eq!(args.time, -6000);
        assert_eq!(args.demo_view, DemoView::Cave);
        assert_eq!(args.mode(), Mode::BlockDemo);
    }

    #[test]
    fn demo_view_defaults_to_overview() {
        let args = Args::parse(&words(&["--block-demo"])).expect("block demo");
        assert_eq!(args.demo_view, DemoView::Overview);
        assert_eq!(args.time, 6000);
    }

    #[test]
    fn setup_assets_sets_expected_mode() {
        let args = Args::parse(&words(&["--setup-assets"])).expect("setup assets parses");
        assert!(args.setup_assets);
        assert_eq!(args.mode(), Mode::SetupAssets);
    }

    #[test]
    fn resource_pack_accumulates_in_order() {
        let args = Args::parse(&words(&[
            "--block-demo",
            "--resource-pack",
            "pack_a.zip",
            "--resource-pack",
            "pack_b",
        ]))
        .expect("block demo with resource packs parses");
        assert_eq!(args.resource_packs.len(), 2);
        assert_eq!(
            args.resource_packs[0],
            std::path::PathBuf::from("pack_a.zip")
        );
        assert_eq!(args.resource_packs[1], std::path::PathBuf::from("pack_b"));
    }

    #[test]
    fn entity_demo_parses_without_server() {
        let args = Args::parse(&words(&["--entity-demo"])).expect("entity demo parses");
        assert!(args.entity_demo);
        assert_eq!(args.mode(), Mode::EntityDemo);
    }

    #[test]
    fn entity_demo_with_screenshot_sets_expected_mode() {
        let args = Args::parse(&words(&["--entity-demo", "--screenshot", "/tmp/m8.png"]))
            .expect("entity demo with screenshot");
        assert!(args.entity_demo);
        assert_eq!(
            args.screenshot,
            Some(std::path::PathBuf::from("/tmp/m8.png"))
        );
        assert_eq!(args.mode(), Mode::EntityDemo);
    }
}
