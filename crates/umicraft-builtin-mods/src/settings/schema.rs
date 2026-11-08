// SPDX-License-Identifier: MIT OR Apache-2.0
//! The description of a setting: what the settings page shows and what is valid.

use super::SettingValue;

/// The shape of a setting and its default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingKind {
    /// On or off.
    Toggle {
        /// Initial state.
        default: bool,
    },
    /// A number in a range, moved in steps.
    Slider {
        /// Smallest value.
        min: f32,
        /// Largest value.
        max: f32,
        /// Distance between two values; must be positive.
        step: f32,
        /// Initial value.
        default: f32,
    },
    /// One of a fixed list of keys.
    Choice {
        /// The keys, in display order. Each is also a language key suffix.
        options: &'static [&'static str],
        /// Index in `options` of the initial choice.
        default: usize,
    },
    /// A color written `#rrggbb`.
    Color {
        /// Initial color.
        default: &'static str,
    },
}

/// One option of a mod.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingDef {
    /// Key in the config file, unique within the mod. Never change it once released.
    pub key: &'static str,
    /// Language key of the label.
    pub label_key: &'static str,
    /// Shape and default.
    pub kind: SettingKind,
}

impl SettingDef {
    /// The value a player who never touched the setting has.
    #[must_use]
    pub fn default_value(&self) -> SettingValue {
        match self.kind {
            SettingKind::Toggle { default } => SettingValue::Bool(default),
            SettingKind::Slider { default, .. } => SettingValue::Number(default),
            SettingKind::Choice { options, default } => {
                SettingValue::Text(options.get(default).copied().unwrap_or_default().to_owned())
            }
            SettingKind::Color { default } => SettingValue::Text(default.to_owned()),
        }
    }

    /// Makes `value` valid for this setting: a slider is clamped and snapped to its
    /// step, and a value of the wrong type or outside the list falls back to the
    /// default. The config file is user-editable, so nothing in it is trusted.
    #[must_use]
    pub fn sanitize(&self, value: &SettingValue) -> SettingValue {
        match (self.kind, value) {
            (SettingKind::Toggle { .. }, SettingValue::Bool(_)) => value.clone(),
            (SettingKind::Slider { min, max, step, .. }, SettingValue::Number(n))
                if n.is_finite() && step > 0.0 =>
            {
                let snapped = min + ((n - min) / step).round() * step;
                SettingValue::Number(snapped.clamp(min, max))
            }
            (SettingKind::Choice { options, .. }, SettingValue::Text(t))
                if options.contains(&t.as_str()) =>
            {
                value.clone()
            }
            (SettingKind::Color { .. }, SettingValue::Text(t)) if is_hex_color(t) => value.clone(),
            _ => self.default_value(),
        }
    }
}

/// Whether `text` is `#rrggbb`.
fn is_hex_color(text: &str) -> bool {
    text.strip_prefix('#')
        .is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLIDER: SettingDef = SettingDef {
        key: "size",
        label_key: "k",
        kind: SettingKind::Slider {
            min: 0.0,
            max: 10.0,
            step: 0.5,
            default: 5.0,
        },
    };
    const CHOICE: SettingDef = SettingDef {
        key: "style",
        label_key: "k",
        kind: SettingKind::Choice {
            options: &["a", "b"],
            default: 1,
        },
    };
    const COLOR: SettingDef = SettingDef {
        key: "color",
        label_key: "k",
        kind: SettingKind::Color { default: "#ffffff" },
    };
    const TOGGLE: SettingDef = SettingDef {
        key: "on",
        label_key: "k",
        kind: SettingKind::Toggle { default: true },
    };

    #[test]
    fn defaults_follow_the_kind() {
        assert_eq!(TOGGLE.default_value(), SettingValue::Bool(true));
        assert_eq!(SLIDER.default_value(), SettingValue::Number(5.0));
        assert_eq!(CHOICE.default_value(), SettingValue::Text("b".into()));
        assert_eq!(COLOR.default_value(), SettingValue::Text("#ffffff".into()));
    }

    #[test]
    fn a_slider_is_clamped_and_snapped() {
        assert_eq!(
            SLIDER.sanitize(&SettingValue::Number(99.0)),
            SettingValue::Number(10.0)
        );
        assert_eq!(
            SLIDER.sanitize(&SettingValue::Number(-3.0)),
            SettingValue::Number(0.0)
        );
        assert_eq!(
            SLIDER.sanitize(&SettingValue::Number(2.3)),
            SettingValue::Number(2.5)
        );
    }

    #[test]
    fn a_non_finite_slider_value_gives_the_default() {
        assert_eq!(
            SLIDER.sanitize(&SettingValue::Number(f32::NAN)),
            SettingValue::Number(5.0)
        );
    }

    #[test]
    fn a_wrong_type_gives_the_default() {
        assert_eq!(
            TOGGLE.sanitize(&SettingValue::Number(1.0)),
            SettingValue::Bool(true)
        );
        assert_eq!(
            SLIDER.sanitize(&SettingValue::Bool(false)),
            SettingValue::Number(5.0)
        );
    }

    #[test]
    fn an_unknown_choice_gives_the_default() {
        assert_eq!(
            CHOICE.sanitize(&SettingValue::Text("z".into())),
            SettingValue::Text("b".into())
        );
        assert_eq!(
            CHOICE.sanitize(&SettingValue::Text("a".into())),
            SettingValue::Text("a".into())
        );
    }

    #[test]
    fn only_a_six_digit_hex_color_is_accepted() {
        let ok = SettingValue::Text("#1a2B3c".into());
        assert_eq!(COLOR.sanitize(&ok), ok);
        for bad in ["red", "#fff", "#gggggg", "1a2b3c", "#1a2b3c0"] {
            assert_eq!(
                COLOR.sanitize(&SettingValue::Text(bad.into())),
                COLOR.default_value(),
                "{bad}"
            );
        }
    }
}
