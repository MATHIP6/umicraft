// SPDX-License-Identifier: MIT OR Apache-2.0
//! Toggle sprint: press the sprint key once to sprint whenever moving forward,
//! press it again to stop.
//!
//! The state belongs to the key, not to the movement: it survives stopping and
//! starting again, and the key can be pressed before moving. (A first version
//! reset it whenever the player stood still, so pressing sprint before walking
//! did nothing.)
//!
//! The "[Sprinting (toggled)]" HUD indicator is not offered: `BuiltinMod::hud` is
//! static, so a widget that exists only while some setting is on cannot be
//! expressed. Adding it needs a trait change (e.g. a hud() taking the settings).

use std::collections::BTreeMap;

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingValue};
use crate::traits::{BuiltinMod, ModInput};

static META: ModMeta = ModMeta {
    id: ModId::new("toggle_sprint"),
    name_key: "umicraft.mods.toggle_sprint.name",
    description_key: "umicraft.mods.toggle_sprint.desc",
    category: ModCategory::Gameplay,
    default_enabled: false,
};

/// The key of the switch in the saved state (append-only, like the settings).
const STATE_KEY: &str = "toggled";

/// The toggle sprint mod.
#[derive(Default)]
pub(super) struct ToggleSprint {
    /// Whether sprint was held at the previous call, to find the rising edge.
    was_pressed: bool,
    /// Whether sprinting is switched on.
    toggled: bool,
    /// Whether `toggled` changed since the client last asked.
    changed: bool,
}

impl BuiltinMod for ToggleSprint {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn modify_input(&mut self, input: &mut ModInput, _settings: &ModSettings) {
        let pressed_now = input.sprint && !self.was_pressed;
        self.was_pressed = input.sprint;
        if pressed_now {
            self.toggled = !self.toggled;
            self.changed = true;
        }
        // The key only toggles: it no longer sprints while held, or the press that
        // switches the mod off would keep sprinting until released.
        input.sprint = self.toggled && input.forward;
    }

    /// The switch is kept between runs: sprint is still toggled after a restart.
    fn state(&self) -> BTreeMap<String, SettingValue> {
        BTreeMap::from([(STATE_KEY.to_owned(), SettingValue::Bool(self.toggled))])
    }

    fn load_state(&mut self, saved: &BTreeMap<String, SettingValue>) {
        self.toggled = matches!(saved.get(STATE_KEY), Some(SettingValue::Bool(true)));
    }

    fn take_state_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds one frame of input and returns the sprint the game would see.
    fn step(m: &mut ToggleSprint, forward: bool, sprint: bool) -> bool {
        let mut input = ModInput { forward, sprint };
        m.modify_input(&mut input, &ModSettings::defaults(&[]));
        input.sprint
    }

    #[test]
    fn sprint_stays_on_after_release() {
        let mut m = ToggleSprint::default();
        assert!(step(&mut m, true, true));
        assert!(step(&mut m, true, false));
        assert!(step(&mut m, true, false));
    }

    #[test]
    fn nothing_changes_without_a_press() {
        let mut m = ToggleSprint::default();
        assert!(!step(&mut m, true, false));
    }

    #[test]
    fn standing_still_does_not_switch_it_off() {
        let mut m = ToggleSprint::default();
        step(&mut m, true, true);
        step(&mut m, true, false);
        assert!(!step(&mut m, false, false), "no sprint without moving");
        assert!(step(&mut m, true, false), "it comes back when moving again");
    }

    #[test]
    fn the_key_can_be_pressed_before_moving() {
        let mut m = ToggleSprint::default();
        assert!(!step(&mut m, false, true), "nothing to sprint yet");
        step(&mut m, false, false);
        assert!(step(&mut m, true, false));
    }

    #[test]
    fn the_press_that_switches_it_off_stops_the_sprint_at_once() {
        let mut m = ToggleSprint::default();
        step(&mut m, true, true);
        step(&mut m, true, false);
        assert!(!step(&mut m, true, true), "still held, but toggled off");
    }

    #[test]
    fn a_second_press_stops_it() {
        let mut m = ToggleSprint::default();
        step(&mut m, true, true);
        step(&mut m, true, false);
        step(&mut m, true, true);
        assert!(!step(&mut m, true, false));
    }

    #[test]
    fn holding_sprint_is_one_press() {
        let mut m = ToggleSprint::default();
        step(&mut m, true, true);
        step(&mut m, true, true);
        assert!(step(&mut m, true, false));
    }

    #[test]
    fn the_switch_is_saved_and_a_new_run_starts_with_it() {
        let mut m = ToggleSprint::default();
        step(&mut m, true, true);
        let saved = m.state();
        assert_eq!(saved["toggled"], SettingValue::Bool(true));
        let mut restarted = ToggleSprint::default();
        restarted.load_state(&saved);
        assert!(
            step(&mut restarted, true, false),
            "sprinting without a press"
        );
    }

    #[test]
    fn a_missing_or_foreign_state_starts_switched_off() {
        let mut m = ToggleSprint::default();
        m.load_state(&BTreeMap::new());
        assert!(!step(&mut m, true, false));
        m.load_state(&BTreeMap::from([(
            "toggled".to_owned(),
            SettingValue::Number(1.0),
        )]));
        assert!(!step(&mut m, true, false));
    }

    #[test]
    fn a_press_reports_a_change_once() {
        let mut m = ToggleSprint::default();
        assert!(!m.take_state_changed());
        step(&mut m, true, true);
        assert!(m.take_state_changed());
        assert!(!m.take_state_changed(), "once per change");
        step(&mut m, true, true);
        assert!(!m.take_state_changed(), "holding is not a change");
    }

    #[test]
    fn has_no_hud_and_no_settings() {
        let m = ToggleSprint::default();
        assert!(m.hud().is_none());
        assert!(m.settings().is_empty());
        assert!(!META.default_enabled);
    }
}
