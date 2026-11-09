// SPDX-License-Identifier: MIT OR Apache-2.0
//! How a HUD widget looks around its text: the background, the corners, the
//! margin and the shadow of the text. Every HUD mod shares these options
//! (`mods::hud_style`), so they look and behave the same in all of them.

use crate::settings::ModSettings;

/// The shadow of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shadow {
    /// A quarter of the brightness of the text, as `drawStringWithShadow` does.
    #[default]
    Vanilla,
    /// A fixed color, `0xRRGGBB`.
    Color(u32),
    /// No shadow.
    Off,
}

/// The look of one widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudStyle {
    /// The background as `(0xRRGGBB, opacity)`, or `None` for no background.
    pub background: Option<(u32, f32)>,
    /// The corner radius of the background, in GUI units.
    pub radius: f32,
    /// The margin between the background and the text, in GUI units.
    pub padding: f32,
    /// The shadow of the text.
    pub shadow: Shadow,
}

impl Default for HudStyle {
    /// The look of the widgets before they had options: a black panel at 35 %.
    fn default() -> Self {
        Self {
            background: Some((0x00_0000, 0.35)),
            radius: 2.0,
            padding: 2.0,
            shadow: Shadow::Vanilla,
        }
    }
}

impl HudStyle {
    /// Reads the shared options of `settings`.
    #[must_use]
    pub fn from_settings(settings: &ModSettings) -> Self {
        let background = settings.bool("background").then(|| {
            (
                crate::mods::parse_color(settings.text("background_color")),
                settings.number("background_opacity"),
            )
        });
        let shadow = match settings.text("shadow") {
            "off" => Shadow::Off,
            "custom" => Shadow::Color(crate::mods::parse_color(settings.text("shadow_color"))),
            _ => Shadow::Vanilla,
        };
        Self {
            background,
            radius: settings.number("corner_radius"),
            padding: settings.number("padding"),
            shadow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::hud_style::hud_settings;
    use crate::settings::{SettingDef, SettingValue};

    const SHARED: &[SettingDef] = hud_settings![];

    fn settings() -> ModSettings {
        ModSettings::defaults(SHARED)
    }

    #[test]
    fn the_defaults_are_the_look_the_widgets_had_before_the_options() {
        assert_eq!(HudStyle::from_settings(&settings()), HudStyle::default());
    }

    #[test]
    fn every_option_reaches_the_style() {
        let mut settings = settings();
        for (key, value) in [
            ("background_color", SettingValue::Text("#102030".into())),
            ("background_opacity", SettingValue::Number(0.8)),
            ("corner_radius", SettingValue::Number(4.0)),
            ("padding", SettingValue::Number(5.0)),
            ("shadow", SettingValue::Text("custom".into())),
            ("shadow_color", SettingValue::Text("#ff0000".into())),
        ] {
            settings.set(key, &value).expect("known key");
        }
        assert_eq!(
            HudStyle::from_settings(&settings),
            HudStyle {
                background: Some((0x10_2030, 0.8)),
                radius: 4.0,
                padding: 5.0,
                shadow: Shadow::Color(0xff_0000),
            }
        );
    }

    #[test]
    fn the_background_and_the_shadow_can_be_turned_off() {
        let mut settings = settings();
        settings
            .set("background", &SettingValue::Bool(false))
            .expect("known key");
        settings
            .set("shadow", &SettingValue::Text("off".into()))
            .expect("known key");
        let style = HudStyle::from_settings(&settings);
        assert_eq!(style.background, None);
        assert_eq!(style.shadow, Shadow::Off);
    }
}
