// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of the built-in mods (ADR-0065): owns the [`ModRegistry`] and
//! feeds it once per rendered frame.
//!
//! The window hands over plain data ([`FrameInput`]); this module turns it into the
//! read-only [`ModContext`] the mods see, so no mod ever reaches into the client.
//! It counts the clicks between two frames because the mods have a single time
//! base, the frame (spec of the ADR: "per frame, not per tick").

use umicraft_builtin_mods::{
    mods, History, HudWidget, ModContext, ModInput, ModRegistry, ModsConfig, RegistryError,
};
use umicraft_ui::menus::{HudEditorOutcome, ModsOutcome};
use umicraft_render::{MotionBlurSettings, MotionBlurVersion};

use crate::input::InputState;
use crate::options::ModsOptions;
use crate::zoom::ZoomSettings;

/// The id of the Motion Blur mod, whose settings the renderer is given.
const MOTION_BLUR_ID: &str = "motion_blur";

/// The id of the Fullbright mod, whose switch the renderer is given.
const FULLBRIGHT_ID: &str = "fullbright";

/// The id of the Zoom mod, whose settings the window is given.
const ZOOM_ID: &str = "zoom";

/// The id of the No Hurt Cam mod.
const NO_HURT_CAM_ID: &str = "no_hurt_cam";

/// Height of the eye above the feet of a standing player, in blocks (MCP
/// `EntityPlayer.getEyeHeight`), to turn the camera into the feet position.
const EYE_HEIGHT: f64 = 1.62;

/// What the window knows at the start of a frame; everything a mod may see.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameInput {
    /// Monotonic time in milliseconds.
    pub now_ms: u64,
    /// Frames drawn during the last second.
    pub fps: u32,
    /// Smoothed duration of a frame, in milliseconds.
    pub frame_ms: f32,
    /// The camera (eye) position, in blocks.
    pub eye: [f64; 3],
    /// The camera yaw, in radians.
    pub yaw_radians: f32,
    /// The camera pitch, in radians.
    pub pitch_radians: f32,
    /// Whether the sprint key is held.
    pub sprinting: bool,
    /// The round trip to the server, when measured.
    pub ping_ms: Option<u32>,
}

/// The registry of the built-in mods and the per-frame data it needs.
pub struct ModsRuntime {
    registry: ModRegistry,
    left_clicks: u32,
    right_clicks: u32,
    /// The changes made since the mods menu opened, to undo and redo.
    history: History,
    /// The state when the mods menu opened, which "cancel" goes back to.
    origin: Option<ModsConfig>,
}

impl ModsRuntime {
    /// Builds the registry from [`mods::all`] and applies the saved `config`.
    ///
    /// A registry that cannot be built (two mods sharing an id, a bug of the
    /// build) is logged and replaced by an empty one: the game must start anyway.
    #[must_use]
    pub fn new(config: &ModsOptions) -> Self {
        let registry = ModRegistry::new(mods::all()).unwrap_or_else(|error| {
            tracing::error!(%error, "built-in mods disabled");
            ModRegistry::new(Vec::new()).expect("an empty registry has no duplicate id")
        });
        let mut runtime = Self {
            registry,
            left_clicks: 0,
            right_clicks: 0,
            history: History::default(),
            origin: None,
        };
        runtime.registry.apply_config(config);
        runtime
    }

    /// Counts a click for the next frame: the left button when `left`, else the
    /// right one.
    pub fn note_click(&mut self, left: bool) {
        let counter = if left {
            &mut self.left_clicks
        } else {
            &mut self.right_clicks
        };
        *counter = counter.saturating_add(1);
    }

    /// Updates every enabled mod for this frame and resets the click counters.
    pub fn update(&mut self, frame: &FrameInput) {
        let ctx = self.context(frame);
        self.left_clicks = 0;
        self.right_clicks = 0;
        self.registry.update(&ctx);
    }

    /// The widgets to draw on the HUD, in drawing order.
    pub fn hud_widgets(&self) -> impl Iterator<Item = HudWidget<'_>> {
        self.registry.hud_widgets()
    }

    /// Lets the behaviour mods rewrite the movement keys.
    pub fn apply_input(&mut self, input: &mut ModInput) {
        self.registry.modify_input(input);
    }

    /// [`Self::apply_input`] on the input state the window sends to the game:
    /// only the forward and sprint keys are visible to the mods.
    pub fn apply_to_state(&mut self, state: &mut InputState) {
        let mut input = ModInput {
            forward: state.forward > 0.0,
            sprint: state.sprint,
        };
        let before = input;
        self.apply_input(&mut input);
        if input.forward && !before.forward {
            state.forward = 1.0;
        }
        state.sprint = input.sprint;
    }

    /// Whether a mod changed what it keeps between runs (toggle sprint went on or
    /// off) since the last call: the options are worth saving now.
    pub fn take_state_changed(&mut self) -> bool {
        self.registry.take_state_changed()
    }

    /// The motion blur the renderer is to draw: `None` while the Motion Blur mod
    /// is off, else its version and strength. Read every frame, so a change in
    /// the menu applies at once.
    #[must_use]
    pub fn motion_blur(&self) -> Option<MotionBlurSettings> {
        let blur = self
            .registry
            .mods()
            .find(|view| view.meta.id.as_str() == MOTION_BLUR_ID)?;
        if !blur.enabled {
            return None;
        }
        let version = match blur.settings.text("version") {
            "v1" => MotionBlurVersion::V1,
            "v2" => MotionBlurVersion::V2,
            _ => MotionBlurVersion::V3,
        };
        Some(MotionBlurSettings {
            version,
            strength: blur.settings.number("strength"),
        })
    }

    /// The Zoom mod settings while it is on, `None` while it is off.
    #[must_use]
    pub fn zoom(&self) -> Option<ZoomSettings> {
        let zoom = self
            .registry
            .mods()
            .find(|view| view.meta.id.as_str() == ZOOM_ID)?;
        zoom.enabled.then(|| ZoomSettings {
            factor: zoom.settings.number("factor"),
            smooth: zoom.settings.bool("smooth"),
            scroll: zoom.settings.bool("scroll"),
            slow_mouse: zoom.settings.bool("slow_mouse"),
        })
    }

    /// Whether the No Hurt Cam mod is on: a hit does not tilt the view.
    #[must_use]
    pub fn no_hurt_cam(&self) -> bool {
        self.registry
            .mods()
            .any(|view| view.meta.id.as_str() == NO_HURT_CAM_ID && view.enabled)
    }

    /// Whether the Fullbright mod is on: the world is lit at full strength.
    #[must_use]
    pub fn fullbright(&self) -> bool {
        self.registry
            .mods()
            .any(|view| view.meta.id.as_str() == FULLBRIGHT_ID && view.enabled)
    }

    /// The registry, for the mods menu to list the mods.
    #[must_use]
    pub fn registry(&self) -> &ModRegistry {
        &self.registry
    }

    /// The registry, for the mods menu to change a setting.
    pub fn registry_mut(&mut self) -> &mut ModRegistry {
        &mut self.registry
    }

    /// Starts a session of the mods menu: the history is empty and "cancel"
    /// goes back to the state of now.
    pub fn begin_edit(&mut self) {
        self.history = History::default();
        self.origin = Some(self.registry.config());
    }

    /// Whether there is a change to undo, and one to redo.
    #[must_use]
    pub fn history_state(&self) -> (bool, bool) {
        (self.history.can_undo(), self.history.can_redo())
    }

    /// Applies what the player did on the mods menu, HUD editor included, and
    /// remembers the state before it so it can be undone. Returns whether the
    /// saved state may have changed, so the caller writes it to disk.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the change names a mod or a setting that does not
    /// exist.
    pub fn apply_change(&mut self, change: &ModsOutcome) -> Result<bool, RegistryError> {
        let restored = match change {
            ModsOutcome::Hud(HudEditorOutcome::Undo) => self.history.undo(self.registry.config()),
            ModsOutcome::Hud(HudEditorOutcome::Redo) => self.history.redo(self.registry.config()),
            ModsOutcome::Hud(HudEditorOutcome::Cancel) => {
                self.history = History::default();
                self.origin.clone()
            }
            _ => {
                let before = self.registry.config();
                let changed = crate::mods_view::apply(&mut self.registry, change)?;
                if changed {
                    self.history.record(before, &history_tag(change));
                }
                return Ok(changed);
            }
        };
        let Some(config) = restored else {
            return Ok(false);
        };
        self.registry.apply_config(&config);
        Ok(true)
    }

    /// The state to save in `options.toml`.
    #[must_use]
    pub fn config(&self) -> ModsConfig {
        self.registry.config()
    }

    fn context(&self, frame: &FrameInput) -> ModContext {
        let [x, y, z] = frame.eye;
        ModContext {
            now_ms: frame.now_ms,
            fps: frame.fps,
            frame_ms: frame.frame_ms,
            position: [x, y - EYE_HEIGHT, z],
            yaw: frame.yaw_radians.to_degrees(),
            pitch: frame.pitch_radians.to_degrees(),
            sprinting: frame.sprinting,
            left_clicks: self.left_clicks,
            right_clicks: self.right_clicks,
            ping_ms: frame.ping_ms,
        }
    }
}

/// What a change is about, for the history to merge the frames of one slider
/// drag into one step; empty for a change that is a step of its own.
fn history_tag(change: &ModsOutcome) -> String {
    match change {
        ModsOutcome::Set { id, key, .. } => format!("{id}/{key}"),
        ModsOutcome::Hud(HudEditorOutcome::Scaled { id, .. }) => format!("{id}/scale"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> FrameInput {
        FrameInput {
            eye: [10.0, 65.62, -3.0],
            yaw_radians: std::f32::consts::PI,
            ..FrameInput::default()
        }
    }

    #[test]
    fn the_context_is_in_feet_and_degrees() {
        let runtime = ModsRuntime::new(&ModsOptions::default());
        let ctx = runtime.context(&frame());
        assert!((ctx.position[1] - 64.0).abs() < 1e-9);
        assert!((ctx.yaw - 180.0).abs() < 1e-3);
    }

    #[test]
    fn clicks_are_counted_for_one_frame_only() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        runtime.note_click(true);
        runtime.note_click(true);
        runtime.note_click(false);
        let ctx = runtime.context(&frame());
        assert_eq!((ctx.left_clicks, ctx.right_clicks), (2, 1));
        runtime.update(&frame());
        let ctx = runtime.context(&frame());
        assert_eq!((ctx.left_clicks, ctx.right_clicks), (0, 0));
    }

    #[test]
    fn the_saved_state_is_kept_and_given_back() {
        let mut config = ModsOptions::default();
        config.0.insert("future".into(), Default::default());
        let runtime = ModsRuntime::new(&config);
        assert!(runtime.config().0.contains_key("future"));
    }

    #[test]
    fn an_idle_mod_set_leaves_the_input_alone() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        let mut state = InputState {
            forward: -1.0,
            sprint: true,
            ..InputState::default()
        };
        let before = state;
        runtime.apply_to_state(&mut state);
        assert_eq!(state, before);
    }

    #[test]
    fn the_motion_blur_is_off_until_the_mod_is_switched_on() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        assert_eq!(runtime.motion_blur(), None);
        runtime
            .registry_mut()
            .set_enabled("motion_blur", true)
            .expect("the mod exists");
        assert_eq!(
            runtime.motion_blur(),
            Some(MotionBlurSettings {
                version: MotionBlurVersion::V3,
                strength: 0.5,
            }),
            "the best version at half strength"
        );
        runtime
            .registry_mut()
            .set_enabled("motion_blur", false)
            .expect("the mod exists");
        assert_eq!(runtime.motion_blur(), None);
    }

    #[test]
    fn no_hurt_cam_is_off_until_the_mod_is_switched_on() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        assert!(!runtime.no_hurt_cam());
        runtime
            .registry_mut()
            .set_enabled("no_hurt_cam", true)
            .expect("the mod exists");
        assert!(runtime.no_hurt_cam());
    }

    #[test]
    fn fullbright_is_off_until_the_mod_is_switched_on() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        assert!(!runtime.fullbright());
        runtime
            .registry_mut()
            .set_enabled("fullbright", true)
            .expect("the mod exists");
        assert!(runtime.fullbright());
        runtime
            .registry_mut()
            .set_enabled("fullbright", false)
            .expect("the mod exists");
        assert!(!runtime.fullbright());
    }

    #[test]
    fn the_motion_blur_follows_its_settings() {
        use umicraft_builtin_mods::SettingValue;
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        let registry = runtime.registry_mut();
        registry
            .set_enabled("motion_blur", true)
            .expect("the mod exists");
        for (key, expected) in [
            ("v1", MotionBlurVersion::V1),
            ("v2", MotionBlurVersion::V2),
            ("v3", MotionBlurVersion::V3),
        ] {
            runtime
                .registry_mut()
                .set_setting("motion_blur", "version", &SettingValue::Text(key.into()))
                .expect("the setting exists");
            assert_eq!(runtime.motion_blur().map(|s| s.version), Some(expected));
        }
        runtime
            .registry_mut()
            .set_setting("motion_blur", "strength", &SettingValue::Number(0.8))
            .expect("the setting exists");
        assert_eq!(runtime.motion_blur().map(|s| s.strength), Some(0.8));
    }

    #[test]
    fn a_toggled_sprint_survives_a_restart() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        runtime
            .registry_mut()
            .set_enabled("toggle_sprint", true)
            .expect("the mod exists");
        assert!(!runtime.take_state_changed(), "nothing pressed yet");
        let walk = |runtime: &mut ModsRuntime, sprint: bool| {
            let mut state = InputState {
                forward: 1.0,
                sprint,
                ..InputState::default()
            };
            runtime.apply_to_state(&mut state);
            state.sprint
        };
        assert!(walk(&mut runtime, true), "one press switches sprint on");
        assert!(runtime.take_state_changed(), "and it is worth saving");
        assert!(!runtime.take_state_changed(), "once");

        // The options go through a file and a new run reads them back.
        let text = toml::to_string(&runtime.config()).expect("serializes");
        let saved: ModsOptions = toml::from_str(&text).expect("parses");
        let mut restarted = ModsRuntime::new(&saved);
        assert!(
            walk(&mut restarted, false),
            "sprint is still on, with no press"
        );
        assert!(!restarted.take_state_changed(), "loading is not a change");
    }

    /// Regression: the first toggle sprint only worked when the key was pressed
    /// while already walking forward, because standing still reset its state.
    #[test]
    fn toggle_sprint_works_when_the_key_is_pressed_before_walking() {
        let mut runtime = ModsRuntime::new(&ModsOptions::default());
        runtime
            .registry_mut()
            .set_enabled("toggle_sprint", true)
            .expect("the mod exists");
        let mut frame = |forward: f32, sprint: bool| {
            let mut state = InputState {
                forward,
                sprint,
                ..InputState::default()
            };
            runtime.apply_to_state(&mut state);
            state.sprint
        };
        assert!(!frame(0.0, true), "the key is pressed standing still");
        assert!(!frame(0.0, false), "and released");
        assert!(frame(1.0, false), "walking forward now sprints");
        assert!(frame(1.0, false), "and keeps sprinting");
    }

    use umicraft_builtin_mods::{Anchor, HudPlacement};
    use umicraft_ui::menus::{HudPresetKind, ModValue};
    use umicraft_ui::Rect;

    fn runtime() -> ModsRuntime {
        let mut mods = ModsRuntime::new(&crate::options::ModsOptions::default());
        mods.begin_edit();
        mods
    }

    fn place(id: &str, x: f32) -> ModsOutcome {
        ModsOutcome::Hud(HudEditorOutcome::Placed {
            id: id.into(),
            rect: Rect::new(x, 50.0, 40.0, 10.0),
            scale: 1.0,
            screen: [400.0, 300.0],
        })
    }

    fn placement(mods: &ModsRuntime, id: &str) -> Option<HudPlacement> {
        mods.registry()
            .mods()
            .find(|view| view.meta.id.as_str() == id)?
            .placement
    }

    fn hud(outcome: HudEditorOutcome) -> ModsOutcome {
        ModsOutcome::Hud(outcome)
    }

    #[test]
    fn undo_and_redo_walk_the_changes_of_the_menu() {
        let mut mods = runtime();
        let start = placement(&mods, "fps");
        assert_eq!(mods.history_state(), (false, false));
        assert_eq!(mods.apply_change(&place("fps", 100.0)), Ok(true));
        let first = placement(&mods, "fps");
        assert_eq!(mods.apply_change(&place("fps", 200.0)), Ok(true));
        let second = placement(&mods, "fps");
        assert_ne!(first, second);
        assert_eq!(mods.history_state(), (true, false));

        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Undo)), Ok(true));
        assert_eq!(placement(&mods, "fps"), first);
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Undo)), Ok(true));
        assert_eq!(placement(&mods, "fps"), start);
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Undo)), Ok(false));
        assert_eq!(mods.history_state(), (false, true));

        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Redo)), Ok(true));
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Redo)), Ok(true));
        assert_eq!(placement(&mods, "fps"), second);
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Redo)), Ok(false));
    }

    #[test]
    fn undo_covers_the_switches_the_settings_the_lock_and_the_presets() {
        let mut mods = runtime();
        let before = mods.config();
        let changes = [
            ModsOutcome::Toggle {
                id: "cps".into(),
                enabled: true,
            },
            ModsOutcome::Set {
                id: "fps".into(),
                key: "background".into(),
                value: ModValue::Bool(false),
            },
            hud(HudEditorOutcome::Lock {
                id: "fps".into(),
                locked: true,
            }),
            hud(HudEditorOutcome::Preset(HudPresetKind::Minimal)),
        ];
        for change in &changes {
            assert_eq!(mods.apply_change(change), Ok(true), "{change:?}");
        }
        assert_ne!(mods.config(), before);
        for _ in &changes {
            assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Undo)), Ok(true));
        }
        assert_eq!(mods.config(), before);
    }

    #[test]
    fn the_frames_of_one_slider_drag_are_one_step() {
        let mut mods = runtime();
        let before = mods.config();
        for opacity in [0.4, 0.5, 0.6, 0.7] {
            let change = ModsOutcome::Set {
                id: "fps".into(),
                key: "background_opacity".into(),
                value: ModValue::Number(opacity),
            };
            assert_eq!(mods.apply_change(&change), Ok(true));
        }
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Undo)), Ok(true));
        assert_eq!(mods.config(), before, "one undo takes the whole drag back");
    }

    #[test]
    fn cancel_goes_back_to_the_state_the_menu_opened_on() {
        let mut mods = runtime();
        let opened = mods.config();
        for change in [
            place("fps", 120.0),
            hud(HudEditorOutcome::Preset(HudPresetKind::Pvp)),
            hud(HudEditorOutcome::Save),
        ] {
            assert_eq!(mods.apply_change(&change), Ok(true));
        }
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Cancel)), Ok(true));
        assert_eq!(mods.config(), opened);
        assert_eq!(mods.history_state(), (false, false));
    }

    #[test]
    fn a_saved_layout_comes_back_after_a_restart_as_the_custom_preset() {
        let mut mods = runtime();
        assert_eq!(mods.apply_change(&place("fps", 300.0)), Ok(true));
        let saved = placement(&mods, "fps");
        assert_eq!(mods.apply_change(&hud(HudEditorOutcome::Save)), Ok(true));

        // Through `options.toml`, as the window saves it after every change.
        let text = toml::to_string(&mods.config()).expect("serializes");
        let mut restarted = ModsRuntime::new(&toml::from_str(&text).expect("parses"));
        assert_eq!(placement(&restarted, "fps"), saved, "the layout is kept");
        restarted.begin_edit();
        let minimal = hud(HudEditorOutcome::Preset(HudPresetKind::Minimal));
        assert_eq!(restarted.apply_change(&minimal), Ok(true));
        assert_ne!(placement(&restarted, "fps"), saved);
        let custom = hud(HudEditorOutcome::Preset(HudPresetKind::Custom));
        assert_eq!(restarted.apply_change(&custom), Ok(true));
        assert_eq!(placement(&restarted, "fps"), saved);
    }

    #[test]
    fn reset_all_and_the_presets_leave_a_locked_widget_where_it_is() {
        let mut mods = runtime();
        assert_eq!(mods.apply_change(&place("fps", 200.0)), Ok(true));
        let here = placement(&mods, "fps");
        assert_eq!(here.map(|p| p.anchor), Some(Anchor::TopCenter));
        let lock = hud(HudEditorOutcome::Lock {
            id: "fps".into(),
            locked: true,
        });
        assert_eq!(mods.apply_change(&lock), Ok(true));
        for change in [
            hud(HudEditorOutcome::ResetAll),
            hud(HudEditorOutcome::Preset(HudPresetKind::Pvp)),
            hud(HudEditorOutcome::Preset(HudPresetKind::Default)),
        ] {
            assert_eq!(mods.apply_change(&change), Ok(true));
            assert_eq!(placement(&mods, "fps"), here, "{change:?}");
        }
    }

    #[test]
    fn a_change_the_registry_refuses_leaves_no_step_to_undo() {
        let mut mods = runtime();
        assert!(mods.apply_change(&place("ghost", 10.0)).is_err());
        assert_eq!(mods.history_state(), (false, false));
    }
}
