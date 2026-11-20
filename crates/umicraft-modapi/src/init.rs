// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `Mod` trait and the context handed to it during `init`.

use core::ffi::c_void;

use umicraft_modapi_sys::{
    UmiHost, UmiModHandle, UmiMode, UmiSettingDef, UmiSettingKind, UmiStatus,
};

use crate::event::{Control, Event};
use crate::ffi::{
    command_trampoline, draw_trampoline, trampoline, CommandHandler, DrawHandler, Handler,
    OwnedUser,
};
use crate::host::utf8;
use crate::setting::{definition, utf8_all};
use crate::{Chat, Hud, Logger, ModError, Packets, Placement, Setting};

/// A mod: built once by the host, told when it is shut down.
pub trait Mod: Send + Sized + 'static {
    /// Builds the mod and registers its handlers. An `Err` makes the host
    /// refuse to load the mod; handlers registered before it are dropped.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError>;

    /// Called once before the library is unloaded, while handlers are still
    /// alive. The default does nothing.
    fn shutdown(&mut self) {}
}

/// What a mod may do during [`Mod::init`]; it is only valid for that call.
///
/// Settings, widgets, key bindings and commands can only be declared here:
/// the host refuses them afterwards. Keys follow `^[a-z][a-z0-9_]{0,63}$`,
/// command names `^[a-z][a-z0-9_-]{0,31}$`.
#[derive(Debug)]
pub struct InitCx<'a> {
    host: &'a UmiHost,
    me: UmiModHandle,
    owned: Vec<OwnedUser>,
}

impl<'a> InitCx<'a> {
    pub(crate) fn new(host: &'a UmiHost, me: UmiModHandle) -> Self {
        Self {
            host,
            me,
            owned: Vec::new(),
        }
    }

    pub(crate) fn into_owned(self) -> Vec<OwnedUser> {
        self.owned
    }

    /// A logger that stays valid after `init` returns.
    pub fn logger(&self) -> Logger {
        Logger
    }

    /// A handle for printing local chat lines, valid after `init` returns.
    pub fn chat(&self) -> Chat {
        Chat
    }

    /// A handle for injecting packets, valid after `init` returns. Using it
    /// needs `network_packets = true` in `mod.toml`.
    pub fn packets(&self) -> Packets {
        Packets
    }

    /// Watches events of type `E`; lower `priority` runs first.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] when the host rejects the subscription.
    pub fn observe<E: Event>(
        &mut self,
        priority: i32,
        mut handler: impl FnMut(&E) + Send + 'static,
    ) -> Result<(), ModError> {
        self.subscribe::<E>(
            UmiMode::OBSERVE,
            priority,
            Box::new(move |event, _control| handler(event)),
        )
    }

    /// Takes part in events of type `E`: the handler may [`Control::cancel`]
    /// them when the event allows it. Lower `priority` runs first.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] when the host rejects the subscription.
    pub fn intercept<E: Event>(
        &mut self,
        priority: i32,
        handler: impl FnMut(&mut E, &mut Control) + Send + 'static,
    ) -> Result<(), ModError> {
        self.subscribe::<E>(UmiMode::INTERCEPT, priority, Box::new(handler))
    }

    /// Declares an on/off setting shown in the mods menu.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`.
    pub fn toggle(
        &mut self,
        key: &'static str,
        label: &str,
        default: bool,
    ) -> Result<Setting<bool>, ModError> {
        let def = UmiSettingDef {
            toggle_default: u8::from(default),
            ..definition(UmiSettingKind::TOGGLE, key, label)
        };
        self.register_setting(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.toggle != 0, default))
    }

    /// Declares a number in `[min, max]` moved by `step`.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`, a
    /// non-positive `step` or a `default` outside the range.
    pub fn slider(
        &mut self,
        key: &'static str,
        label: &str,
        min: f32,
        max: f32,
        step: f32,
        default: f32,
    ) -> Result<Setting<f32>, ModError> {
        let def = UmiSettingDef {
            slider_min: min,
            slider_max: max,
            slider_step: step,
            slider_default: default,
            ..definition(UmiSettingKind::SLIDER, key, label)
        };
        self.register_setting(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.slider, default))
    }

    /// Declares a choice among `options`; the value is the selected index.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`, no
    /// options or a `default_index` past the last one.
    pub fn choice(
        &mut self,
        key: &'static str,
        label: &str,
        options: &[&str],
        default_index: u32,
    ) -> Result<Setting<u32>, ModError> {
        let labels = utf8_all(options);
        let def = UmiSettingDef {
            choices: labels.as_ptr(),
            choices_len: labels.len(),
            choice_default: default_index,
            ..definition(UmiSettingKind::CHOICE, key, label)
        };
        self.register_setting(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.choice, default_index))
    }

    /// Declares a colour, `0xRRGGBB`.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`.
    pub fn color(
        &mut self,
        key: &'static str,
        label: &str,
        default: u32,
    ) -> Result<Setting<u32>, ModError> {
        let def = UmiSettingDef {
            color_default: default,
            ..definition(UmiSettingKind::COLOR, key, label)
        };
        self.register_setting(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.color, default))
    }

    /// Declares a HUD widget the player can move and scale; `draw` runs on
    /// the render thread every frame while the mod is enabled and must be
    /// quick (the host budgets it).
    ///
    /// ```ignore
    /// cx.hud_widget("clock", "Clock", Placement::new(Anchor::TopLeft), |hud| {
    ///     hud.line("hello", 0xFFFFFF);
    /// })?;
    /// ```
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`.
    pub fn hud_widget(
        &mut self,
        key: &str,
        label: &str,
        placement: Placement,
        draw: impl FnMut(&mut Hud<'_>) + Send + 'static,
    ) -> Result<(), ModError> {
        let draw: DrawHandler = Box::new(draw);
        let (host, me) = (self.host, self.me);
        self.keep(draw, |user| {
            (host.register_hud_widget)(
                me,
                utf8(key),
                utf8(label),
                placement.to_raw(),
                Some(draw_trampoline),
                user,
            )
        })
    }

    /// Declares a key binding listed on the controls page under the mod's
    /// name. `default_binding` is a key name such as `"KeyH"` or `"Mouse4"`,
    /// or empty for unbound. A press arrives as [`KeyPressed`](crate::KeyPressed)
    /// to this mod only.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`.
    pub fn key(&mut self, key: &str, label: &str, default_binding: &str) -> Result<(), ModError> {
        let status =
            (self.host.register_key)(self.me, utf8(key), utf8(label), utf8(default_binding));
        ModError::check(status)
    }

    /// Declares the client command `/<name>`; the line never reaches the
    /// server. `run` gets the text after the name and returns `Err` to make
    /// the host report the failure (the error text is not shown).
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid name or one another mod took.
    pub fn command(
        &mut self,
        name: &str,
        usage: &str,
        run: impl FnMut(&str) -> Result<(), ModError> + Send + 'static,
    ) -> Result<(), ModError> {
        let run: CommandHandler = Box::new(run);
        let (host, me) = (self.host, self.me);
        self.keep(run, |user| {
            (host.register_command)(me, utf8(name), utf8(usage), Some(command_trampoline), user)
        })
    }

    fn register_setting(&self, def: &UmiSettingDef) -> Result<(), ModError> {
        ModError::check((self.host.register_setting)(self.me, def))
    }

    fn subscribe<E: Event>(
        &mut self,
        mode: UmiMode,
        priority: i32,
        handler: Handler<E>,
    ) -> Result<(), ModError> {
        let (host, me) = (self.host, self.me);
        self.keep(handler, |user| {
            (host.subscribe)(me, E::ID, mode, priority, Some(trampoline::<E>), user)
        })
    }

    /// Hands `value` to the host as a `user` pointer through `register` and
    /// keeps it alive until shutdown, or frees it at once when the host
    /// refuses (it did not keep the pointer).
    fn keep<T: Send + 'static>(
        &mut self,
        value: T,
        register: impl FnOnce(*mut c_void) -> UmiStatus,
    ) -> Result<(), ModError> {
        let owned = OwnedUser::new(value);
        ModError::check(register(owned.user()))?;
        self.owned.push(owned);
        Ok(())
    }
}
