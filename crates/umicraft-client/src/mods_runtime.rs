// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of the built-in mods (ADR-0065): owns the [`ModRegistry`] and
//! feeds it once per rendered frame.
//!
//! The window hands over plain data ([`FrameInput`]); this module turns it into the
//! read-only [`ModContext`] the mods see, so no mod ever reaches into the client.
//! It counts the clicks between two frames because the mods have a single time
//! base, the frame (spec of the ADR: "per frame, not per tick").

use umicraft_builtin_mods::{mods, HudWidget, ModContext, ModInput, ModRegistry, ModsConfig};

use crate::input::InputState;
use crate::options::ModsOptions;

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
