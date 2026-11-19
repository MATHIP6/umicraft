// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed handles to the settings a mod declares.

use umicraft_modapi_sys::{UmiSettingDef, UmiSettingKind, UmiSettingValue, UmiStr};

use crate::host;

/// A setting's current value, read from the host on demand.
///
/// Obtained from [`InitCx::toggle`](crate::InitCx::toggle), `slider`, `choice`
/// or `color`. `Copy`, `Send` and `Sync`: capture it in any handler. The host
/// stores the value in atomics, so [`get`](Self::get) works on any thread and
/// always returns what the player last chose.
///
/// ```ignore
/// let show = cx.toggle("show", "Show it", true)?;
/// cx.observe::<PreTick>(0, move |_| if show.get() { /* ... */ })?;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Setting<T> {
    key: &'static str,
    kind: UmiSettingKind,
    read: fn(&UmiSettingValue) -> T,
    default: T,
}

impl<T: Copy> Setting<T> {
    pub(crate) fn new(
        key: &'static str,
        kind: UmiSettingKind,
        read: fn(&UmiSettingValue) -> T,
        default: T,
    ) -> Self {
        Self {
            key,
            kind,
            read,
            default,
        }
    }

    /// The current value.
    ///
    /// Never fails: if the host cannot answer (before `init`, or a host that
    /// does not know the key), the default given at registration is returned,
    /// because a mod should keep working with its defaults rather than crash.
    pub fn get(&self) -> T {
        let mut out = UmiSettingValue {
            kind: self.kind,
            toggle: 0,
            slider: 0.0,
            choice: 0,
            color: 0,
        };
        let asked = host::call(|host, me| {
            (host.setting_get)(me, host::utf8(self.key), &raw mut out).is_ok()
        });
        if asked == Ok(true) && out.kind == self.kind {
            (self.read)(&out)
        } else {
            self.default
        }
    }
}

/// A definition of `kind` with every kind-specific field zeroed; callers set
/// the ones that apply.
pub(crate) fn definition(kind: UmiSettingKind, key: &str, label: &str) -> UmiSettingDef {
    UmiSettingDef {
        size: u32::try_from(size_of::<UmiSettingDef>()).unwrap_or(u32::MAX),
        kind,
        key: host::utf8(key),
        label: host::utf8(label),
        toggle_default: 0,
        slider_min: 0.0,
        slider_max: 0.0,
        slider_step: 0.0,
        slider_default: 0.0,
        choices: std::ptr::null(),
        choices_len: 0,
        choice_default: 0,
        color_default: 0,
    }
}

/// The option labels as ABI strings, borrowed from `labels`.
pub(crate) fn utf8_all(labels: &[&str]) -> Vec<UmiStr> {
    labels.iter().map(|label| host::utf8(label)).collect()
}
