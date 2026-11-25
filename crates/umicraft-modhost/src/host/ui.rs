// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player-facing side of [`ModHost`]: what the client's menus, HUD, controls
//! page and chat need from the loaded mods (F3.4, ADR-0086).

use crossbeam_channel::Receiver;
use umicraft_builtin_mods::{HudPlacement, SettingValue};

use super::ModHost;
use crate::error::ConfigureError;
use crate::events::{ButtonPressed, ModuleToggled};
use crate::manifest::ModId;
use crate::packets::PacketOutbox;
use crate::ui::commands::CommandTable;
use crate::ui::config::ExternalModConfig;
use crate::ui::hud::DrawnWidget;
use crate::ui::view::{KeyBinding, ModView};

impl ModHost {
    /// Every loaded mod as the mods menu shows it, in load order. Includes mods the
    /// player switched off and mods that failed.
    pub fn mods(&self) -> impl Iterator<Item = ModView<'_>> {
        self.mods
            .iter()
            .map(|m| ModView::new(&m.manifest, &m.sha256, &m.context))
    }

    /// The key bindings of every mod that has not failed, for the controls page.
    /// A switched-off mod is listed too so the player can rebind it beforehand;
    /// `KeyPressed` only reaches an enabled one.
    pub fn key_bindings(&self) -> impl Iterator<Item = KeyBinding<'_>> {
        self.mods
            .iter()
            .filter(|m| !m.context.is_failed())
            .flat_map(|m| ModView::new(&m.manifest, &m.sha256, &m.context).keys())
    }

    /// Calls the draw function of every widget of every enabled mod.
    ///
    /// Render thread, once per frame. Each call is budgeted ([`HUD_BUDGET`]); a mod
    /// whose draw function fails or overruns for too long is failed and skipped from
    /// then on. Read the result with [`visit_widgets`](Self::visit_widgets).
    ///
    /// [`HUD_BUDGET`]: crate::HUD_BUDGET
    pub fn draw_widgets(&self) {
        for loaded in &self.mods {
            loaded.context.draw_widgets();
        }
    }

    /// Runs the world layers of every enabled mod, in load order, into `out`,
    /// which is cleared first (API 1.4).
    ///
    /// Render thread, once per frame in a world, after the camera layers. Budgeted
    /// like the widgets; the caps of each mod are in [`WorldDraw`]'s module.
    ///
    /// [`WorldDraw`]: crate::WorldDraw
    pub fn draw_world_layers(&self, out: &mut crate::WorldDraw) {
        out.clear();
        for loaded in &self.mods {
            loaded.context.draw_world_layers(out);
        }
    }

    /// Lends each widget of each enabled mod, as drawn by the last
    /// [`draw_widgets`](Self::draw_widgets), to `visit`.
    ///
    /// A visitor instead of a returned `Vec` because the content buffers are reused
    /// from frame to frame: borrowing them keeps a frame with foreign widgets free of
    /// allocations once the buffers have grown. `visit` must not call back into the
    /// host.
    pub fn visit_widgets(&self, mut visit: impl FnMut(DrawnWidget<'_>)) {
        for loaded in &self.mods {
            loaded.context.visit_widgets(&mut visit);
        }
    }

    /// The command table for the game thread: `Send + Clone`.
    pub fn commands(&self) -> CommandTable {
        self.shared.commands()
    }

    /// The receiving end of the local chat lines mods print. The client drains it on
    /// the render thread and shows each line as local chat.
    pub fn chat_outbox(&self) -> Receiver<String> {
        self.shared.chat_outbox()
    }

    /// The packets mods injected with `packet_send`, for the network thread to drain.
    pub fn packet_outbox(&self) -> PacketOutbox {
        PacketOutbox::new(self.shared.packet_inbox())
    }

    /// Turns server-compat mode on or off: while on, a mod that declared
    /// `network_packets` or `movement` is not enabled. Takes effect at once on every
    /// thread, because handlers consult the state per call.
    pub fn set_server_compat(&self, on: bool) {
        self.shared.set_server_compat(on);
    }

    /// Whether server-compat mode is on.
    pub fn server_compat(&self) -> bool {
        self.shared.server_compat()
    }

    /// The current choices of mod `id`, for saving; `None` for an unknown mod.
    pub fn config(&self, id: &ModId) -> Option<ExternalModConfig> {
        self.context(id).map(|context| context.config())
    }

    /// Applies saved choices to mod `id` (sanitised; unknown keys are kept for the
    /// next [`config`](Self::config)).
    ///
    /// # Errors
    ///
    /// [`ConfigureError::UnknownMod`] for an id that is not loaded.
    pub fn apply_config(
        &self,
        id: &ModId,
        config: &ExternalModConfig,
    ) -> Result<(), ConfigureError> {
        self.configure(id, |context| {
            context.apply_config(config);
            Ok(())
        })
    }

    /// Changes a setting of mod `id`; the value is sanitised.
    ///
    /// # Errors
    ///
    /// [`ConfigureError`] for an unknown mod or setting key.
    pub fn set_setting(
        &self,
        id: &ModId,
        key: &str,
        value: &SettingValue,
    ) -> Result<(), ConfigureError> {
        self.configure(id, |context| context.set_setting(key, value))
    }

    /// Moves a widget of mod `id`; the scale is clamped.
    ///
    /// # Errors
    ///
    /// [`ConfigureError`] for an unknown mod or widget key.
    pub fn set_placement(
        &self,
        id: &ModId,
        widget: &str,
        placement: HudPlacement,
    ) -> Result<(), ConfigureError> {
        self.configure(id, |context| context.set_placement(widget, placement))
    }

    /// Binds a key binding of mod `id` to the key named `binding` (empty: unbound).
    ///
    /// # Errors
    ///
    /// [`ConfigureError`] for an unknown mod or key-binding id.
    pub fn set_key(&self, id: &ModId, key: &str, binding: &str) -> Result<(), ConfigureError> {
        self.configure(id, |context| context.set_key(key, binding))
    }

    /// Switches module `module` of mod `id` on or off (API 1.2). Returns the
    /// [`ModuleToggled`] event to dispatch on the render bus when the switch changed,
    /// `None` when it already was in that state.
    ///
    /// # Errors
    ///
    /// [`ConfigureError`] for an unknown mod or module.
    pub fn set_module(
        &self,
        id: &ModId,
        module: &str,
        on: bool,
    ) -> Result<Option<ModuleToggled>, ConfigureError> {
        let context = self.known(id)?;
        let changed = context.set_module(module, on)?;
        Ok(changed.then(|| ModuleToggled::new(id.clone(), module, on)))
    }

    /// The [`ButtonPressed`] event for a click on button `key` of mod `id` (API 1.2),
    /// to dispatch on the render bus.
    ///
    /// # Errors
    ///
    /// [`ConfigureError`] for an unknown mod, or a key that is not a button.
    pub fn press_button(&self, id: &ModId, key: &str) -> Result<ButtonPressed, ConfigureError> {
        let module = self.known(id)?.button_module(key)?;
        Ok(ButtonPressed::new(id.clone(), &module, key))
    }

    fn known(&self, id: &ModId) -> Result<&crate::ModContext, ConfigureError> {
        self.context(id)
            .ok_or_else(|| ConfigureError::UnknownMod(id.clone()))
    }

    fn configure(
        &self,
        id: &ModId,
        change: impl FnOnce(&crate::ModContext) -> Result<(), ConfigureError>,
    ) -> Result<(), ConfigureError> {
        change(self.known(id)?)
    }
}
