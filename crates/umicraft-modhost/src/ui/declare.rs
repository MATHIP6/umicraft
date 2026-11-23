// SPDX-License-Identifier: MIT OR Apache-2.0

//! The registration side of [`ModContext`]: what the `register_*`, `setting_get` and
//! `chat_print` host functions do once the `ffi` module has decoded their arguments.

use umicraft_builtin_mods::HudPlacement;
use umicraft_modapi_sys::{UmiCommandFn, UmiHudDrawFn, UmiSettingValue, UmiStatus};

use super::commands::CommandEntry;
use super::hud::WidgetSlot;
use super::keys::KeySlot;
use super::settings::SettingSpec;
use super::validate::{check_command_name, check_len};
use super::{MAX_CHAT_BYTES, MAX_USAGE_BYTES};
use crate::context::ModContext;
use crate::error::DeclareError;
use crate::ffi::UserPtr;
use crate::hook::warn_sparse;

impl ModContext {
    /// Declares a setting. Registrations are accepted during `umi_mod_init` only.
    ///
    /// # Errors
    ///
    /// [`DeclareError::Late`] after init, [`DeclareError::Duplicate`] for a key
    /// already declared, and the spec's own validation errors.
    pub fn register_setting(&self, spec: SettingSpec) -> Result<(), DeclareError> {
        self.ui.register(|declared| declared.add_setting(spec))
    }

    /// Declares a HUD widget drawn by `draw` with `user`.
    ///
    /// # Errors
    ///
    /// As [`register_setting`](Self::register_setting), plus an invalid key or label
    /// and a null `draw`.
    pub fn register_widget(
        &self,
        key: &str,
        label: &str,
        placement: HudPlacement,
        draw: Option<UmiHudDrawFn>,
        user: UserPtr,
    ) -> Result<(), DeclareError> {
        self.ui.register(|declared| {
            declared.add_widget(WidgetSlot::new(key, label, placement, draw, user)?)
        })
    }

    /// Declares a HUD widget anchored to the screen itself (API 1.1): its content
    /// starts at `at`, in GUI pixels from the top-left corner, at scale 1, and the
    /// player cannot move it. For annotations over the world.
    ///
    /// # Errors
    ///
    /// As [`register_widget`](Self::register_widget).
    pub fn register_screen_widget(
        &self,
        key: &str,
        label: &str,
        at: [f32; 2],
        draw: Option<UmiHudDrawFn>,
        user: UserPtr,
    ) -> Result<(), DeclareError> {
        self.ui.register(|declared| {
            declared.add_widget(WidgetSlot::on_screen(key, label, at, draw, user)?)
        })
    }

    /// Declares a key binding; `default_binding` may be empty (unbound).
    ///
    /// # Errors
    ///
    /// As [`register_setting`](Self::register_setting), plus an invalid key, label or
    /// default key name.
    pub fn register_key(
        &self,
        key: &str,
        label: &str,
        default_binding: &str,
    ) -> Result<(), DeclareError> {
        self.ui
            .register(|declared| declared.add_key(KeySlot::new(key, label, default_binding)?))
    }

    /// Declares the command `/<name>`.
    ///
    /// # Errors
    ///
    /// As [`register_setting`](Self::register_setting), plus an invalid name, an
    /// over-long usage text, a null `run`, and [`DeclareError::Duplicate`] when any
    /// mod already took the name.
    pub fn register_command(
        &self,
        name: &str,
        usage: &str,
        run: Option<UmiCommandFn>,
        user: UserPtr,
    ) -> Result<(), DeclareError> {
        self.ui.register(|declared| {
            check_command_name(name)?;
            check_len("command usage", usage, MAX_USAGE_BYTES)?;
            let run = run.ok_or(DeclareError::Invalid {
                what: "command",
                why: "the run function must not be null",
            })?;
            declared.check_command_room()?;
            let entry = CommandEntry {
                owner: self.me.clone(),
                usage: usage.into(),
                run,
                user,
            };
            self.shared.add_command(name, entry)?;
            declared.record_command(name);
            Ok(())
        })
    }

    /// The current value of setting `key`, from any thread, without a lock once the
    /// mod is initialised. `None` for an undeclared key.
    pub fn setting_get(&self, key: &str) -> Option<UmiSettingValue> {
        self.ui.setting_value(key)
    }

    /// Queues a local chat line for the client; `DENIED` from a mod the player
    /// switched off (its worker may still be running), `INVALID_ARGUMENT` for a line
    /// over [`MAX_CHAT_BYTES`]. A full outbox drops the line, counts it and still
    /// reports `OK`: it is flood protection, not an error of the mod.
    pub fn chat_print(&self, text: &str) -> UmiStatus {
        if self.ui.is_sealed() && !self.is_enabled() {
            return UmiStatus::DENIED;
        }
        if check_len("chat line", text, MAX_CHAT_BYTES).is_err() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        if let Some(dropped) = self.shared.print(text.to_owned()) {
            warn_sparse(self, dropped, "chat outbox full, line dropped");
        }
        UmiStatus::OK
    }

    /// The status for a mod and a log line for its author when a registration failed.
    pub fn report(&self, result: Result<(), DeclareError>) -> UmiStatus {
        match result {
            Ok(()) => UmiStatus::OK,
            Err(error) => {
                let mod_id = self.id();
                tracing::warn!(%mod_id, %error, "registration refused");
                error.status()
            }
        }
    }
}

#[cfg(test)]
mod tests;
