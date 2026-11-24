// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the API 1.2 menu registrations: categories, modules, settings
//! and buttons on a module, attachments and `module_enabled` (ADR-0095). Decisions
//! are made in [`crate::ui::menu`].
#![allow(unsafe_code)]

use std::mem::size_of;

use umicraft_modapi_sys::{
    UmiModHandle, UmiModuleDef, UmiSettingDef, UmiStatus, UmiStr, UMI_MODULE_DEFAULT_ON,
};

use super::ui::spec_of;
use super::{guarded, text_of};
use crate::error::DeclareError;
use crate::ui::menu::{Attach, ModuleDef};
use crate::ui::{
    MAX_BUTTON_TEXT_BYTES, MAX_CATEGORY_BYTES, MAX_DESCRIPTION_BYTES, MAX_LABEL_BYTES,
};

/// Longest key, in bytes, read from a mod (the grammar's own limit).
const MAX_KEY_BYTES: usize = 64;

fn invalid(what: &'static str, why: &'static str) -> DeclareError {
    DeclareError::Invalid { what, why }
}

/// A module key that may be empty (the mod's own card).
fn module_of<'a>(module: UmiStr) -> Result<&'a str, DeclareError> {
    text_of(module, "module key", MAX_KEY_BYTES)
}

fn module_def_of(def: &UmiModuleDef) -> Result<ModuleDef<'_>, DeclareError> {
    if def.size < size_of::<UmiModuleDef>() as u32 {
        return Err(invalid("module", "`size` is smaller than the struct"));
    }
    Ok(ModuleDef {
        key: text_of(def.key, "module key", MAX_KEY_BYTES)?,
        label: text_of(def.label, "module label", MAX_LABEL_BYTES)?,
        description: text_of(def.description, "module description", MAX_DESCRIPTION_BYTES)?,
        category: text_of(def.category, "category key", MAX_KEY_BYTES)?,
        default_on: def.flags & UMI_MODULE_DEFAULT_ON != 0,
    })
}

pub(super) extern "C" fn host_register_category(
    me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let key = text_of(key, "category key", MAX_KEY_BYTES)?;
            let label = text_of(label, "category label", MAX_CATEGORY_BYTES)?;
            cx.register_category(key, label)
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_register_module(
    me: UmiModHandle,
    def: *const UmiModuleDef,
) -> UmiStatus {
    guarded(me, |cx| {
        // SAFETY: a non-null `def` points to a `UmiModuleDef` the mod keeps readable
        // for the duration of the call (ABI contract, ADR-0087).
        let result = unsafe { def.as_ref() }
            .ok_or(invalid("module", "the definition must not be null"))
            .and_then(module_def_of)
            .and_then(|def| cx.register_module(&def));
        cx.report(result)
    })
}

pub(super) extern "C" fn host_register_setting_in(
    me: UmiModHandle,
    module: UmiStr,
    def: *const UmiSettingDef,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = module_of(module).and_then(|module| {
            // SAFETY: as `host_register_setting`: a non-null `def` is readable for
            // the duration of the call.
            let spec = unsafe { def.as_ref() }
                .ok_or(invalid("setting", "the definition must not be null"))
                .and_then(spec_of)?;
            cx.register_setting_in(module, spec)
        });
        cx.report(result)
    })
}

pub(super) extern "C" fn host_register_button(
    me: UmiModHandle,
    module: UmiStr,
    key: UmiStr,
    label: UmiStr,
    text: UmiStr,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let module = module_of(module)?;
            let key = text_of(key, "button key", MAX_KEY_BYTES)?;
            let label = text_of(label, "button label", MAX_LABEL_BYTES)?;
            let text = text_of(text, "button text", MAX_BUTTON_TEXT_BYTES)?;
            cx.register_button(module, key, label, text)
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_module_attach(
    me: UmiModHandle,
    module: UmiStr,
    kind: u32,
    key: UmiStr,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let module = module_of(module)?;
            let kind = Attach::from_raw(kind)?;
            let key = text_of(key, "attached key", MAX_KEY_BYTES)?;
            cx.module_attach(module, kind, key)
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_module_enabled(
    me: UmiModHandle,
    module: UmiStr,
    out: *mut u8,
) -> UmiStatus {
    guarded(me, |cx| {
        let on = module_of(module)
            .ok()
            .and_then(|module| cx.module_enabled(module));
        match on {
            Some(on) if !out.is_null() => {
                // SAFETY: `out` is non-null and the ABI says it is valid for a write
                // of one byte.
                unsafe { out.write(u8::from(on)) };
                UmiStatus::OK
            }
            _ => UmiStatus::INVALID_ARGUMENT,
        }
    })
}
