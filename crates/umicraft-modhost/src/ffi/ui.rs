// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the F3.4 registrations: settings, widgets, key bindings,
//! commands, `setting_get` and `chat_print`. Raw pointers become owned, validated
//! Rust values here; every decision is made in [`crate::ui`] (ADR-0087).
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::mem::size_of;

use umicraft_modapi_sys::{
    UmiCommandFn, UmiHudDrawFn, UmiHudPlacement, UmiModHandle, UmiSettingDef, UmiSettingKind,
    UmiSettingValue, UmiStatus, UmiStr,
};

use super::{guarded, text_of, UserPtr};
use crate::error::DeclareError;
use crate::ui::hud::{placement_of, DeclaredPlacement};
use crate::ui::settings::{SettingSpec, SettingSpecKind};
use crate::ui::{MAX_CHAT_BYTES, MAX_CHOICES, MAX_LABEL_BYTES, MAX_OPTION_BYTES, MAX_USAGE_BYTES};

/// Longest key, in bytes, read from a mod (the grammar's own limit).
const MAX_KEY_BYTES: usize = 64;
/// Longest command name, in bytes.
const MAX_COMMAND_BYTES: usize = 32;

fn invalid(what: &'static str, why: &'static str) -> DeclareError {
    DeclareError::Invalid { what, why }
}

/// Reads the option labels of a choice. At most one more than the limit is read, so
/// a huge `choices_len` costs nothing and still fails validation.
fn choices_of(def: &UmiSettingDef) -> Result<Vec<String>, DeclareError> {
    let count = def.choices_len.min(MAX_CHOICES + 1);
    if count == 0 {
        return Ok(Vec::new());
    }
    if def.choices.is_null() {
        return Err(invalid("setting", "`choices` must not be null"));
    }
    // SAFETY: the ABI promises `choices` points to `choices_len` readable `UmiStr`
    // for the duration of the call; `count <= choices_len` and null was excluded.
    let raw = unsafe { core::slice::from_raw_parts(def.choices, count) };
    raw.iter()
        .map(|option| text_of(*option, "choice option", MAX_OPTION_BYTES).map(str::to_owned))
        .collect()
}

fn kind_of(def: &UmiSettingDef) -> Result<SettingSpecKind, DeclareError> {
    match def.kind {
        UmiSettingKind::TOGGLE if def.toggle_default <= 1 => Ok(SettingSpecKind::Toggle {
            default: def.toggle_default == 1,
        }),
        UmiSettingKind::TOGGLE => Err(invalid("setting", "toggle default must be 0 or 1")),
        UmiSettingKind::SLIDER => Ok(SettingSpecKind::Slider {
            min: def.slider_min,
            max: def.slider_max,
            step: def.slider_step,
            default: def.slider_default,
        }),
        UmiSettingKind::CHOICE => Ok(SettingSpecKind::Choice {
            options: choices_of(def)?,
            default: def.choice_default as usize,
        }),
        UmiSettingKind::COLOR => Ok(SettingSpecKind::Color {
            default: def.color_default,
        }),
        _ => Err(invalid("setting", "unknown setting kind")),
    }
}

fn spec_of(def: &UmiSettingDef) -> Result<SettingSpec, DeclareError> {
    if def.size < size_of::<UmiSettingDef>() as u32 {
        return Err(invalid("setting", "`size` is smaller than the struct"));
    }
    Ok(SettingSpec {
        key: text_of(def.key, "setting key", MAX_KEY_BYTES)?.to_owned(),
        label: text_of(def.label, "setting label", MAX_LABEL_BYTES)?.to_owned(),
        kind: kind_of(def)?,
    })
}

pub(super) extern "C" fn host_register_setting(
    me: UmiModHandle,
    def: *const UmiSettingDef,
) -> UmiStatus {
    guarded(me, |cx| {
        // SAFETY: a non-null `def` points to a `UmiSettingDef` the mod keeps readable
        // for the duration of the call (ABI contract, ADR-0087).
        let result = unsafe { def.as_ref() }
            .ok_or(invalid("setting", "the definition must not be null"))
            .and_then(spec_of)
            .and_then(|spec| cx.register_setting(spec));
        cx.report(result)
    })
}

pub(super) extern "C" fn host_setting_get(
    me: UmiModHandle,
    key: UmiStr,
    out: *mut UmiSettingValue,
) -> UmiStatus {
    guarded(me, |cx| {
        let value = text_of(key, "setting key", MAX_KEY_BYTES)
            .ok()
            .and_then(|key| cx.setting_get(key));
        match value {
            Some(value) if !out.is_null() => {
                // SAFETY: `out` is non-null and the ABI says it is valid for a write
                // of one `UmiSettingValue`; `write` does not read the old contents,
                // which may be uninitialised.
                unsafe { out.write(value) };
                UmiStatus::OK
            }
            _ => UmiStatus::INVALID_ARGUMENT,
        }
    })
}

pub(super) extern "C" fn host_register_hud_widget(
    me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
    placement: UmiHudPlacement,
    draw: Option<UmiHudDrawFn>,
    user: *mut c_void,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let key = text_of(key, "widget key", MAX_KEY_BYTES)?;
            let label = text_of(label, "widget label", MAX_LABEL_BYTES)?;
            let user = UserPtr::new(user);
            match placement_of(placement)? {
                DeclaredPlacement::Anchored(placement) => {
                    cx.register_widget(key, label, placement, draw, user)
                }
                DeclaredPlacement::Screen(at) => {
                    cx.register_screen_widget(key, label, at, draw, user)
                }
            }
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_register_key(
    me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
    default_binding: UmiStr,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let key = text_of(key, "key binding id", MAX_KEY_BYTES)?;
            let label = text_of(label, "key binding label", MAX_LABEL_BYTES)?;
            let default = text_of(
                default_binding,
                "default key name",
                crate::ui::MAX_BINDING_BYTES,
            )?;
            cx.register_key(key, label, default)
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_register_command(
    me: UmiModHandle,
    name: UmiStr,
    usage: UmiStr,
    run: Option<UmiCommandFn>,
    user: *mut c_void,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let name = text_of(name, "command name", MAX_COMMAND_BYTES)?;
            let usage = text_of(usage, "command usage", MAX_USAGE_BYTES)?;
            cx.register_command(name, usage, run, UserPtr::new(user))
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_chat_print(me: UmiModHandle, text: UmiStr) -> UmiStatus {
    guarded(me, |cx| match text_of(text, "chat line", MAX_CHAT_BYTES) {
        Ok(text) => cx.chat_print(text),
        Err(_) => UmiStatus::INVALID_ARGUMENT,
    })
}

#[cfg(test)]
mod tests;
