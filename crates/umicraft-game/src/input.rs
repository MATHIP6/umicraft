// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's input intent.
//!
//! The game logic never sees a key press: the client samples the window once
//! per tick and hands the game thread an [`InputIntent`]. The conversion from
//! the raw key and axis state applies the 1.8 rules that live in the input
//! layer, so that the physics only ever sees the values the vanilla game
//! would have used.

/// The factor the movement axes are scaled by while sneaking.
///
/// MCP `net/minecraft/util/MovementInputFromOptions.java:44-45` multiplies
/// `moveStrafe` and `moveForward` by `0.3` when the sneak key is held.
pub const SNEAK_INPUT_SCALE: f32 = 0.3;

/// The factor the movement axes are scaled by while an item is in use.
///
/// MCP `net/minecraft/client/entity/EntityPlayerSP.java:788-792` multiplies
/// `moveStrafe` and `moveForward` by `0.2` while `isUsingItem()` and the
/// player does not ride.
pub const USING_ITEM_INPUT_SCALE: f32 = 0.2;

/// The forward input from which the player may sprint.
///
/// MCP `net/minecraft/client/entity/EntityPlayerSP.java:784-785` compares the
/// forward input against `0.8`.
pub const SPRINT_FORWARD_THRESHOLD: f32 = 0.8;

/// One tick of player intent, sampled from the input system.
///
/// The axes are the analogue values of the movement keys: `-1.0` is fully
/// backwards (or to the right) and `1.0` fully forwards (or to the left),
/// matching the sign convention of MCP
/// `net/minecraft/util/MovementInput.java` ("positive numbers to the left").
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputIntent {
    /// Forward movement, `-1.0` (backwards) ..= `1.0` (forwards).
    pub forward: f32,
    /// Sideways movement, `-1.0` (right) ..= `1.0` (left).
    pub strafe: f32,
    /// Whether the jump key is held.
    pub jump: bool,
    /// Whether the sneak key is held.
    pub sneak: bool,
    /// Whether the sprint key is held.
    pub sprint: bool,
    /// The horizontal view angle in degrees (MCP `rotationYaw`, ADR-0076).
    pub yaw: f32,
    /// The vertical view angle in degrees (MCP `rotationPitch`).
    pub pitch: f32,
}

impl InputIntent {
    /// An intent without any movement, looking along `yaw`.
    #[must_use]
    pub fn still(yaw: f32) -> Self {
        Self {
            yaw,
            ..Self::default()
        }
    }

    /// An intent that walks straight forwards along `yaw`.
    #[must_use]
    pub fn walking(yaw: f32) -> Self {
        Self {
            forward: 1.0,
            yaw,
            ..Self::default()
        }
    }
}

/// The raw key and axis state of one frame.
///
/// The client fills this in from the window; [`RawInput::to_intent`] turns it
/// into the intent the game logic consumes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RawInput {
    /// Forward movement, `-1.0` (backwards) ..= `1.0` (forwards).
    pub forward: f32,
    /// Sideways movement, `-1.0` (right) ..= `1.0` (left).
    pub strafe: f32,
    /// Whether the jump key is held.
    pub jump: bool,
    /// Whether the sneak key is held.
    pub sneak: bool,
    /// Whether the sprint key is held.
    pub sprint: bool,
    /// Whether the player uses an item (eats, drinks, blocks, draws a bow)
    /// on foot, which slows it down.
    pub using_item: bool,
    /// The horizontal view angle in degrees (MCP `rotationYaw`, ADR-0076).
    pub yaw: f32,
    /// The vertical view angle in degrees (MCP `rotationPitch`).
    pub pitch: f32,
}

impl RawInput {
    /// Converts the raw state into an [`InputIntent`].
    ///
    /// The movement axes are clamped to `[-1, 1]`, scaled by
    /// [`SNEAK_INPUT_SCALE`] while the sneak key is held and then by
    /// [`USING_ITEM_INPUT_SCALE`] while an item is in use, which is what MCP
    /// `net/minecraft/util/MovementInputFromOptions.java:42-46` does before
    /// the input reaches the entity.
    #[must_use]
    pub fn to_intent(&self) -> InputIntent {
        let mut forward = self.forward.clamp(-1.0, 1.0);
        let mut strafe = self.strafe.clamp(-1.0, 1.0);
        if self.sneak {
            forward *= SNEAK_INPUT_SCALE;
            strafe *= SNEAK_INPUT_SCALE;
        }
        if self.using_item {
            forward *= USING_ITEM_INPUT_SCALE;
            strafe *= USING_ITEM_INPUT_SCALE;
        }
        InputIntent {
            forward,
            strafe,
            jump: self.jump,
            sneak: self.sneak,
            sprint: self.sprint,
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }
}

/// The highest and lowest pitch, in degrees (MCP `Entity.setAngles`).
pub const MAX_PITCH: f32 = 90.0;

/// The degrees one unit of mouse turn moves the look (MCP `Entity.setAngles`:
/// `(double)yaw * 0.15D`).
pub const TURN_DEGREES: f64 = 0.15;

/// Turns a look by a mouse turn, the way `Entity.setAngles` does.
///
/// `turn` is what `EntityRenderer.updateCameraAndRender` hands to
/// `setAngles`: whole mouse pixels times the sensitivity factor, positive to
/// the right and downwards. The sums are made in `double` and narrowed to
/// `float`, and the pitch is clamped to ±[`MAX_PITCH`]; the yaw is never
/// wrapped. Returns the new `(yaw, pitch)` in degrees.
///
/// # Example
/// ```
/// use umicraft_game::input::set_angles;
/// // 10 pixels to the right at the default sensitivity (factor 1).
/// assert_eq!(set_angles(0.0, 0.0, (10.0, 0.0)), (1.5, 0.0));
/// assert_eq!(set_angles(0.0, 89.0, (0.0, 100.0)).1, 90.0);
/// ```
#[must_use]
pub fn set_angles(yaw: f32, pitch: f32, turn: (f32, f32)) -> (f32, f32) {
    let yaw = (f64::from(yaw) + f64::from(turn.0) * TURN_DEGREES) as f32;
    let pitch = (f64::from(pitch) + f64::from(turn.1) * TURN_DEGREES) as f32;
    (yaw, pitch.clamp(-MAX_PITCH, MAX_PITCH))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_raw_axes_are_clamped() {
        let raw = RawInput {
            forward: 3.0,
            strafe: -5.0,
            ..RawInput::default()
        };
        let intent = raw.to_intent();
        assert_eq!(intent.forward, 1.0);
        assert_eq!(intent.strafe, -1.0);
    }

    #[test]
    fn sneaking_scales_the_movement_axes() {
        // MCP `MovementInputFromOptions.java:44-45`: 0.3 while sneaking.
        let raw = RawInput {
            forward: 1.0,
            strafe: 1.0,
            sneak: true,
            ..RawInput::default()
        };
        let intent = raw.to_intent();
        assert!((intent.forward - 0.3).abs() < 1e-6);
        assert!((intent.strafe - 0.3).abs() < 1e-6);
        assert!(intent.sneak);
    }

    /// MCP `EntityPlayerSP.java:788-792`: 0.2 while an item is in use,
    /// applied after the sneak scale.
    #[test]
    fn using_an_item_slows_the_movement_axes() {
        let raw = RawInput {
            forward: 1.0,
            strafe: -1.0,
            using_item: true,
            ..RawInput::default()
        };
        let intent = raw.to_intent();
        assert!((intent.forward - 0.2).abs() < 1e-6);
        assert!((intent.strafe + 0.2).abs() < 1e-6);
        let sneaking = RawInput { sneak: true, ..raw };
        assert!((sneaking.to_intent().forward - 0.06).abs() < 1e-6);
    }

    #[test]
    fn the_view_angles_are_passed_through() {
        let raw = RawInput {
            yaw: 1.25,
            pitch: -0.5,
            ..RawInput::default()
        };
        let intent = raw.to_intent();
        assert_eq!(intent.yaw, 1.25);
        assert_eq!(intent.pitch, -0.5);
    }

    #[test]
    fn set_angles_adds_in_double_and_narrows_to_float() {
        // MCP `Entity.setAngles`: `(float)((double)yaw + (double)turn * 0.15D)`.
        let (yaw, _) = set_angles(123.456, 0.0, (7.0 * 1.3824, 0.0));
        let expected = (f64::from(123.456_f32) + f64::from(7.0_f32 * 1.3824) * 0.15) as f32;
        assert_eq!(yaw.to_bits(), expected.to_bits());
    }

    #[test]
    fn set_angles_never_wraps_the_yaw() {
        assert_eq!(
            set_angles(359.85, 0.0, (2.0, 0.0)).0,
            (359.85_f64 as f32 as f64 + 0.3) as f32
        );
        assert!(set_angles(0.0, 0.0, (-2.0, 0.0)).0 < 0.0);
    }

    #[test]
    fn set_angles_clamps_the_pitch() {
        assert_eq!(set_angles(0.0, 0.0, (0.0, 10_000.0)).1, MAX_PITCH);
        assert_eq!(set_angles(0.0, 0.0, (0.0, -10_000.0)).1, -MAX_PITCH);
    }
}
