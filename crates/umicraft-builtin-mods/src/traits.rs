// SPDX-License-Identifier: MIT OR Apache-2.0
//! The trait every built-in mod implements.

use std::collections::BTreeMap;

use crate::context::ModContext;
use crate::hud::{HudContent, HudPlacement};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingDef, SettingValue};

/// What a HUD mod needs to be placed the first time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudSpec {
    /// Where the widget sits until the player moves it.
    pub default_placement: HudPlacement,
}

/// The movement keys as held by the player, which a mod may rewrite before they
/// reach the game. Only what a built-in mod needs; it grows with the mods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModInput {
    /// Moving forward.
    pub forward: bool,
    /// Sprinting.
    pub sprint: bool,
}

/// A built-in mod.
///
/// Only [`meta`](Self::meta) is required. A display mod implements
/// [`hud`](Self::hud) and [`draw`](Self::draw); a behaviour mod implements
/// [`modify_input`](Self::modify_input); the rest default to doing nothing, so a
/// mod is a few dozen lines.
///
/// Every method but `meta` is only called while the mod is enabled. `update`,
/// `draw` and `modify_input` run on the render thread every frame: they must not
/// allocate (write into the given [`HudContent`], which reuses its buffers).
pub trait BuiltinMod: Send {
    /// The static description shown in the menu.
    fn meta(&self) -> &'static ModMeta;

    /// The options the player can change. The settings page is generated from this.
    fn settings(&self) -> &'static [SettingDef] {
        &[]
    }

    /// `Some` if the mod draws a widget on the HUD.
    fn hud(&self) -> Option<HudSpec> {
        None
    }

    /// Advances the mod's own state, once per frame.
    fn update(&mut self, _ctx: &ModContext, _settings: &ModSettings) {}

    /// Fills the widget text. Called after [`update`](Self::update).
    fn draw(&self, _ctx: &ModContext, _settings: &ModSettings, _out: &mut HudContent) {}

    /// Rewrites the movement input, e.g. to keep sprinting after one press.
    fn modify_input(&mut self, _input: &mut ModInput, _settings: &ModSettings) {}

    /// What the mod keeps between two runs of the client, apart from its
    /// settings, as the registry saves it. Nothing by default.
    fn state(&self) -> BTreeMap<String, SettingValue> {
        BTreeMap::new()
    }

    /// Takes back what [`state`](Self::state) saved. Keys it does not know are
    /// ignored and a missing one leaves the mod as it starts.
    fn load_state(&mut self, _saved: &BTreeMap<String, SettingValue>) {}

    /// Whether [`state`](Self::state) changed since the last call, so the client
    /// can save it at once instead of waiting for the menu: `true` once per
    /// change.
    fn take_state_changed(&mut self) -> bool {
        false
    }
}
