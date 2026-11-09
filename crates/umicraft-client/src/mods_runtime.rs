// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of the built-in mods (ADR-0065): owns the [`ModRegistry`] and
//! feeds it once per rendered frame.
//!
//! The window hands over plain data ([`FrameInput`]); this module turns it into the
//! read-only [`ModContext`] the mods see, so no mod ever reaches into the client.
//! It counts the clicks between two frames because the mods have a single time
//! base, the frame (spec of the ADR: "per frame, not per tick").

use umicraft_builtin_mods::{mods, HudWidget, ModContext, ModInput, ModRegistry, ModsConfig};
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
}
