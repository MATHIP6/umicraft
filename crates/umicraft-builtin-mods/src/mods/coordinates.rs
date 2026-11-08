// SPDX-License-Identifier: MIT OR Apache-2.0
//! The position: X, Y and Z of the player and the direction they face.

use std::fmt::Write;

use super::WHITE;
use crate::context::ModContext;
use crate::hud::{Anchor, HudContent, HudPlacement};
use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingKind};
use crate::traits::{BuiltinMod, HudSpec};

static META: ModMeta = ModMeta {
    id: ModId::new("coordinates"),
    name_key: "umicraft.mods.coordinates.name",
    description_key: "umicraft.mods.coordinates.desc",
    category: ModCategory::Hud,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "decimals",
        label_key: "umicraft.mods.coordinates.decimals",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "direction",
        label_key: "umicraft.mods.coordinates.direction",
        kind: SettingKind::Toggle { default: true },
    },
];

/// The coordinates mod.
pub(super) struct Coordinates;

/// The cardinal direction a yaw faces. Yaw 0 looks south (+Z) and grows clockwise
/// seen from above (90 is west), as in the game's own F3 screen.
fn cardinal(yaw: f32) -> &'static str {
    // Sectors of 90 degrees centered on the four directions.
    let sector = ((yaw + 45.0).rem_euclid(360.0) / 90.0) as usize;
    match sector {
        0 => "South",
        1 => "West",
        2 => "North",
        _ => "East",
    }
}

impl BuiltinMod for Coordinates {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }

    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopLeft, 4.0, 52.0),
        })
    }

    fn draw(&self, ctx: &ModContext, settings: &ModSettings, out: &mut HudContent) {
        let decimals = settings.bool("decimals");
        for (axis, value) in ["X", "Y", "Z"].into_iter().zip(ctx.position) {
            // Writing into a `String` cannot fail.
            let _ = if decimals {
                write!(out.line(WHITE), "{axis}: {value:.1}")
            } else {
                // The block the player stands in, as the F3 screen shows it: the
                // floor, so -7.5 is block -8 and not -7.
                write!(out.line(WHITE), "{axis}: {}", value.floor())
            };
        }
        if settings.bool("direction") {
            let _ = write!(out.line(WHITE), "{}", cardinal(ctx.yaw));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{assert_reads_only_schema_keys, drawn_texts};
    use crate::settings::SettingValue;

    fn ctx(yaw: f32) -> ModContext {
        ModContext {
            position: [12.345, 64.0, -7.5],
            yaw,
            ..ModContext::default()
        }
    }

    #[test]
    fn shows_the_position_and_direction() {
        let settings = ModSettings::defaults(SETTINGS);
        assert_eq!(
            drawn_texts(&Coordinates, &ctx(0.0), &settings),
            ["X: 12.3", "Y: 64.0", "Z: -7.5", "South"]
        );
    }

    #[test]
    fn without_decimals_the_block_coordinates_are_shown() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set("decimals", &SettingValue::Bool(false))
            .expect("known key");
        assert_eq!(
            drawn_texts(&Coordinates, &ctx(0.0), &settings)[..3],
            ["X: 12", "Y: 64", "Z: -8"],
            "negative values go down to the block they are in"
        );
    }

    #[test]
    fn the_direction_can_be_hidden() {
        let mut settings = ModSettings::defaults(SETTINGS);
        settings
            .set("direction", &SettingValue::Bool(false))
            .expect("known key");
        assert_eq!(drawn_texts(&Coordinates, &ctx(0.0), &settings).len(), 3);
    }

    #[test]
    fn yaw_follows_the_minecraft_convention() {
        let cases = [
            (0.0, "South"),
            (90.0, "West"),
            (180.0, "North"),
            (-90.0, "East"),
            (270.0, "East"),
            (-180.0, "North"),
            (44.0, "South"),
            (46.0, "West"),
            (-46.0, "East"),
            (720.0, "South"),
            (-720.5, "South"),
        ];
        for (yaw, expected) in cases {
            assert_eq!(cardinal(yaw), expected, "yaw {yaw}");
        }
    }

    #[test]
    fn reads_only_schema_keys() {
        assert_reads_only_schema_keys(&Coordinates, &["decimals", "direction"]);
    }
}
