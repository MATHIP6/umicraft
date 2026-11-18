// SPDX-License-Identifier: MIT OR Apache-2.0
//! The direction HUD: a ruler along the top of the screen that scrolls as the
//! player turns. Degrees every 15, a letter for each of the eight directions,
//! the current heading in the middle, and everything fading towards the edges.

use std::fmt::Write;

use super::hud_style::hud_settings_bare;
use super::parse_color;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("direction_hud"),
    name_key: "umicraft.mods.direction_hud.name",
    description_key: "umicraft.mods.direction_hud.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = hud_settings_bare![
    SettingDef {
        key: "width",
        label_key: "umicraft.mods.direction_hud.width",
        kind: SettingKind::Slider {
            min: 120.0,
            max: 400.0,
            step: 10.0,
            default: 240.0,
        },
    },
    SettingDef {
        key: "degrees",
        label_key: "umicraft.mods.direction_hud.degrees",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "heading",
        label_key: "umicraft.mods.direction_hud.heading",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "color",
        label_key: "umicraft.mods.direction_hud.color",
        kind: SettingKind::Color { default: "#ffffff" },
    },
];

/// The degrees the ruler shows at once, whatever its width.
const VISIBLE_DEGREES: f32 = 150.0;
/// The gap between two marks, in degrees.
const STEP: i32 = 15;
/// Letters stand for every multiple of this many degrees.
const LETTER_STEP: i32 = 45;
/// The names of the eight directions from yaw 0, which looks south and grows
/// clockwise seen from above (90 is west), as in the F3 screen.
const LETTERS: [&str; 8] = ["S", "SW", "W", "NW", "N", "NE", "E", "SE"];
/// The size of a letter relative to a number.
const LETTER_SCALE: f32 = 1.5;
/// Marks closer than this many degrees to the edge are too faint to draw.
const MIN_ALPHA: f32 = 0.05;

/// Rows of the widget, in GUI units from its top.
const ARROW_TOP: f32 = 9.0;
const TICK_TOP: f32 = 13.0;
const TICK_HEIGHT: f32 = 3.0;
const NUMBER_TOP: f32 = 17.0;
const LETTER_TOP: f32 = 15.0;
/// The height of the whole widget: the letters hang lowest.
const HEIGHT: f32 = LETTER_TOP + 8.0 * LETTER_SCALE;

/// The direction HUD mod.
pub(super) struct DirectionHud;

/// The opacity of a mark `offset` degrees from the middle: full there, none at
/// the edge, falling off with the square so the middle stays readable.
fn fade(offset: f32) -> f32 {
    let t = (offset.abs() / (VISIBLE_DEGREES / 2.0)).min(1.0);
    1.0 - t * t
}

/// The letter for a direction that is a multiple of 45 degrees.
fn letter(degrees: i32) -> &'static str {
    let index = usize::try_from(degrees.rem_euclid(360) / LETTER_STEP).unwrap_or(0);
    LETTERS[index % LETTERS.len()]
}

impl BuiltinMod for DirectionHud {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopCenter, 0.0, 4.0),
        })
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        let width = settings.number("width");
        let color = parse_color(settings.text("color"));
        let per_degree = width / VISIBLE_DEGREES;
        let middle = width / 2.0;
        out.reserve = [width, HEIGHT];
        if settings.bool("heading") {
            // The heading in the middle with an arrow under it, drawn as three
            // rows of one unit.
            let heading = ctx.yaw.rem_euclid(360.0).floor() as u32;
            // Writing into a `String` cannot fail.
            let _ = write!(out.label([middle, 0.0], color, 1.0, 1.0), "{heading}");
            for (row, half) in [2.0, 1.0, 0.0].into_iter().enumerate() {
                out.tick(
                    [middle - half, ARROW_TOP + row as f32, 2.0 * half + 1.0, 1.0],
                    color,
                    1.0,
                );
            }
        }
        let half_range = VISIBLE_DEGREES / 2.0;
        let first = ((ctx.yaw - half_range) / STEP as f32).ceil() as i32;
        let last = ((ctx.yaw + half_range) / STEP as f32).floor() as i32;
        for mark in first..=last {
            let degrees = mark * STEP;
            let offset = degrees as f32 - ctx.yaw;
            let alpha = fade(offset);
            if alpha < MIN_ALPHA {
                continue;
            }
            let x = middle + offset * per_degree;
            if degrees % LETTER_STEP == 0 {
                out.label([x, LETTER_TOP], color, alpha, LETTER_SCALE)
                    .push_str(letter(degrees));
            } else if settings.bool("degrees") {
                out.tick([x - 0.5, TICK_TOP, 1.0, TICK_HEIGHT], color, alpha);
                let _ = write!(
                    out.label([x, NUMBER_TOP], color, alpha, 1.0),
                    "{}",
                    degrees.rem_euclid(360)
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::assert_reads_only_schema_keys;
    use crate::settings::SettingValue;

    fn drawn(yaw: f32, settings: &ModSettings) -> HudContent {
        let mut out = HudContent::default();
        DirectionHud.draw(
            &ModContext {
                yaw,
                ..ModContext::default()
            },
            settings,
            &mut out,
        );
        out
    }

    fn texts(out: &HudContent) -> Vec<&str> {
        out.labels().iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn the_heading_is_shown_in_the_middle() {
        let out = drawn(11.4, &ModSettings::defaults(SETTINGS));
        let heading = &out.labels()[0];
        assert_eq!(heading.text, "11");
        assert_eq!(heading.at[0], 120.0, "the middle of the default width");
    }

    #[test]
    fn marks_are_numbers_every_15_and_letters_every_45() {
        let out = drawn(11.0, &ModSettings::defaults(SETTINGS));
        let shown = texts(&out);
        for expected in ["345", "S", "15", "30", "SW", "60", "75", "330", "SE"] {
            assert!(shown.contains(&expected), "{expected} in {shown:?}");
        }
        assert!(!shown.contains(&"315"), "a letter takes its place");
        assert!(!shown.contains(&"0"), "so does S");
    }

    #[test]
    fn the_letters_follow_the_game_directions() {
        assert_eq!(
            (0..8).map(|i| letter(i * 45)).collect::<Vec<_>>(),
            ["S", "SW", "W", "NW", "N", "NE", "E", "SE"]
        );
        assert_eq!(letter(-45), "SE");
        assert_eq!(letter(720 + 90), "W");
    }

    #[test]
    fn turning_scrolls_the_ruler_towards_the_left() {
        let settings = ModSettings::defaults(SETTINGS);
        let x_of = |yaw: f32| {
            drawn(yaw, &settings)
                .labels()
                .iter()
                .find(|l| l.text == "30")
                .map(|l| l.at[0])
        };
        let (before, after) = (x_of(0.0).expect("shown"), x_of(10.0).expect("shown"));
        assert!(after < before, "{after} < {before}");
        assert!((before - after - 10.0 * 240.0 / VISIBLE_DEGREES).abs() < 1e-3);
    }

    #[test]
    fn marks_fade_towards_the_edges_and_the_middle_is_opaque() {
        assert_eq!(fade(0.0), 1.0);
        assert_eq!(fade(VISIBLE_DEGREES / 2.0), 0.0);
        assert!(fade(30.0) > fade(60.0));
        let out = drawn(0.0, &ModSettings::defaults(SETTINGS));
        let s = out.labels().iter().find(|l| l.text == "S").expect("S");
        assert_eq!((s.alpha, s.scale), (1.0, LETTER_SCALE));
    }

    #[test]
    fn the_widget_keeps_its_size_whatever_is_drawn() {
        let settings = ModSettings::defaults(SETTINGS);
        assert_eq!(
            drawn(0.0, &settings).reserve,
            drawn(37.0, &settings).reserve
        );
        assert_eq!(drawn(0.0, &settings).reserve[0], 240.0);
    }

    #[test]
    fn the_options_hide_the_numbers_and_the_heading() {
        let mut settings = ModSettings::defaults(SETTINGS);
        for key in ["degrees", "heading"] {
            settings
                .set(key, &SettingValue::Bool(false))
                .expect("known key");
        }
        let out = drawn(11.0, &settings);
        assert!(texts(&out)
            .iter()
            .all(|t| t.chars().all(char::is_alphabetic)));
        assert!(out.ticks().is_empty(), "no arrow, no marks");
    }

    #[test]
    fn a_wider_ruler_spreads_the_marks() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set("width", &SettingValue::Number(400.0))
            .expect("known key");
        let out = drawn(0.0, &settings);
        assert_eq!(out.reserve[0], 400.0);
        let at = |text: &str| {
            out.labels()
                .iter()
                .find(|l| l.text == text)
                .map(|l| l.at[0])
        };
        assert!((at("15").expect("shown") - 200.0 - 15.0 * 400.0 / VISIBLE_DEGREES).abs() < 1e-3);
    }

    #[test]
    fn reads_only_its_schema() {
        assert_reads_only_schema_keys(&DirectionHud, &["width", "degrees", "heading", "color"]);
    }
}
