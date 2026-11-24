// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings a mod declares: their spec and their lock-free current value.
//!
//! The value of a setting lives in one [`AtomicU32`] holding a bit pattern (toggle
//! 0/1, the slider's `f32` bits, the choice index, `0xRRGGBB`), so `setting_get` is
//! callable from any thread without a lock (ADR-0086, F3.4). Every write goes through
//! the spec's sanitising rules, shared with the built-in settings.

use std::sync::atomic::{AtomicU32, Ordering};

use umicraft_builtin_mods::{format_hex_color, parse_hex_color, snap_slider, SettingValue};
use umicraft_modapi_sys::{UmiSettingKind, UmiSettingValue};

use super::validate::{check_key, check_text};
use super::{MAX_BUTTON_TEXT_BYTES, MAX_CHOICES, MAX_LABEL_BYTES, MAX_OPTION_BYTES};
use crate::error::DeclareError;

/// The shape of a declared setting and its default.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingSpecKind {
    /// On or off.
    Toggle {
        /// Initial state.
        default: bool,
    },
    /// A number in `[min, max]` moved by `step`.
    Slider {
        /// Lowest value.
        min: f32,
        /// Highest value.
        max: f32,
        /// Increment, greater than 0.
        step: f32,
        /// Initial value, within `[min, max]`.
        default: f32,
    },
    /// One of a list of options, identified by their label.
    Choice {
        /// The option labels, 1 to 32 of them.
        options: Vec<String>,
        /// Index of the initial option.
        default: usize,
    },
    /// An `0xRRGGBB` colour.
    Color {
        /// Initial colour.
        default: u32,
    },
    /// A button that tells the mod it was clicked (API 1.2). It has no value: it is
    /// never saved and `setting_get` does not know it.
    Button {
        /// The text on the button.
        text: String,
    },
}

/// A setting as a mod declared it, with owned texts (the host copies them).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingSpec {
    /// Key, unique within the mod; the player's value is saved under it.
    pub key: String,
    /// Label shown in the mods menu (literal text, not a language key).
    pub label: String,
    /// Shape and default.
    pub kind: SettingSpecKind,
}

impl SettingSpec {
    /// Checks the whole definition: key grammar, label length, and per kind a finite
    /// slider range with `min < max`, `step > 0` and the default in range, 1 to 32
    /// choices with the default among them, or a 24-bit colour.
    ///
    /// # Errors
    ///
    /// The first [`DeclareError`] found.
    pub fn validate(&self) -> Result<(), DeclareError> {
        check_key("setting key", &self.key)?;
        check_text("setting label", &self.label, MAX_LABEL_BYTES)?;
        match &self.kind {
            SettingSpecKind::Toggle { .. } => Ok(()),
            SettingSpecKind::Slider {
                min,
                max,
                step,
                default,
            } => validate_slider(*min, *max, *step, *default),
            SettingSpecKind::Choice { options, default } => validate_choice(options, *default),
            SettingSpecKind::Color { default } => {
                invalid(*default > 0x00ff_ffff, "colour default must be 0xRRGGBB")
            }
            SettingSpecKind::Button { text } => {
                check_text("button text", text, MAX_BUTTON_TEXT_BYTES)
            }
        }
    }

    /// Whether this is a button, which holds no value.
    pub fn is_button(&self) -> bool {
        matches!(self.kind, SettingSpecKind::Button { .. })
    }

    /// The value of a player who never touched the setting.
    pub fn default_value(&self) -> SettingValue {
        self.decode(self.default_bits())
    }

    /// Makes `value` valid for this setting, like the built-in settings do: a slider
    /// is clamped and snapped to its step; a value of the wrong type, an unknown
    /// choice or a malformed colour becomes the default.
    pub fn sanitize(&self, value: &SettingValue) -> SettingValue {
        self.decode(self.sanitize_bits(value))
    }

    fn default_bits(&self) -> u32 {
        match &self.kind {
            SettingSpecKind::Toggle { default } => u32::from(*default),
            SettingSpecKind::Slider { default, .. } => default.to_bits(),
            SettingSpecKind::Choice { default, .. } => *default as u32,
            SettingSpecKind::Color { default } => *default,
            SettingSpecKind::Button { .. } => 0,
        }
    }

    /// The bit pattern of `value`, or `None` when it is not valid for this spec.
    fn encode(&self, value: &SettingValue) -> Option<u32> {
        match (&self.kind, value) {
            (SettingSpecKind::Toggle { .. }, SettingValue::Bool(on)) => Some(u32::from(*on)),
            (SettingSpecKind::Slider { min, max, step, .. }, SettingValue::Number(n)) => {
                snap_slider(*n, *min, *max, *step).map(f32::to_bits)
            }
            (SettingSpecKind::Choice { options, .. }, SettingValue::Text(label)) => options
                .iter()
                .position(|option| option == label)
                .map(|index| index as u32),
            (SettingSpecKind::Color { .. }, SettingValue::Text(text)) => parse_hex_color(text),
            _ => None,
        }
    }

    fn sanitize_bits(&self, value: &SettingValue) -> u32 {
        self.encode(value).unwrap_or_else(|| self.default_bits())
    }

    fn decode(&self, bits: u32) -> SettingValue {
        match &self.kind {
            SettingSpecKind::Toggle { .. } => SettingValue::Bool(bits != 0),
            SettingSpecKind::Slider { .. } => SettingValue::Number(f32::from_bits(bits)),
            SettingSpecKind::Choice { options, .. } => {
                SettingValue::Text(options.get(bits as usize).cloned().unwrap_or_default())
            }
            SettingSpecKind::Color { .. } => SettingValue::Text(format_hex_color(bits)),
            SettingSpecKind::Button { .. } => SettingValue::Bool(false),
        }
    }

    /// The ABI value for `bits`: only the field matching the kind is set. `None` for
    /// a button, which has no value.
    fn abi_value(&self, bits: u32) -> Option<UmiSettingValue> {
        let mut out = UmiSettingValue {
            kind: UmiSettingKind::TOGGLE,
            toggle: 0,
            slider: 0.0,
            choice: 0,
            color: 0,
        };
        match &self.kind {
            SettingSpecKind::Toggle { .. } => out.toggle = u8::from(bits != 0),
            SettingSpecKind::Slider { .. } => {
                out.kind = UmiSettingKind::SLIDER;
                out.slider = f32::from_bits(bits);
            }
            SettingSpecKind::Choice { .. } => {
                out.kind = UmiSettingKind::CHOICE;
                out.choice = bits;
            }
            SettingSpecKind::Color { .. } => {
                out.kind = UmiSettingKind::COLOR;
                out.color = bits;
            }
            SettingSpecKind::Button { .. } => return None,
        }
        Some(out)
    }
}

fn invalid(failed: bool, why: &'static str) -> Result<(), DeclareError> {
    if failed {
        Err(DeclareError::Invalid {
            what: "setting",
            why,
        })
    } else {
        Ok(())
    }
}

fn validate_slider(min: f32, max: f32, step: f32, default: f32) -> Result<(), DeclareError> {
    let finite = [min, max, step, default].iter().all(|v| v.is_finite());
    invalid(!finite, "slider numbers must be finite")?;
    invalid(min >= max, "slider `min` must be below `max`")?;
    invalid(step <= 0.0, "slider `step` must be positive")?;
    invalid(
        !(min..=max).contains(&default),
        "slider default must be within `[min, max]`",
    )
}

fn validate_choice(options: &[String], default: usize) -> Result<(), DeclareError> {
    invalid(
        options.is_empty() || options.len() > MAX_CHOICES,
        "a choice needs 1 to 32 options",
    )?;
    for option in options {
        check_text("choice option", option, MAX_OPTION_BYTES)?;
    }
    invalid(default >= options.len(), "choice default is not an option")
}

/// A declared setting and its current value.
#[derive(Debug)]
pub(crate) struct SettingSlot {
    spec: SettingSpec,
    bits: AtomicU32,
    /// Index of the module whose card shows it (API 1.2); `None` for the mod's card.
    module: Option<usize>,
}

impl SettingSlot {
    /// A slot holding the spec's default, on the card of `module` (`None`: the
    /// mod's own card).
    pub(crate) fn new(spec: SettingSpec, module: Option<usize>) -> Self {
        let bits = AtomicU32::new(spec.default_bits());
        Self { spec, bits, module }
    }

    pub(crate) fn spec(&self) -> &SettingSpec {
        &self.spec
    }

    /// The module whose card shows this setting, if any.
    pub(crate) fn module(&self) -> Option<usize> {
        self.module
    }

    /// The current value, for the menu and the config file.
    pub(crate) fn value(&self) -> SettingValue {
        self.spec.decode(self.bits.load(Ordering::Relaxed))
    }

    /// The current value as the ABI struct, for `setting_get`. Lock-free. `None` for
    /// a button.
    pub(crate) fn abi_value(&self) -> Option<UmiSettingValue> {
        self.spec.abi_value(self.bits.load(Ordering::Relaxed))
    }

    /// Stores `value` after sanitising it.
    pub(crate) fn set(&self, value: &SettingValue) {
        self.bits
            .store(self.spec.sanitize_bits(value), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests;
