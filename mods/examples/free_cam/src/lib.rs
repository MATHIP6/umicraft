// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: free camera (API 1.3).
//!
//! Shows `Camera::capture_input`: while the free camera is on, a `render_camera`
//! wrapper flies the eye with the movement keys (jump up, sneak down) and turns it
//! with the mouse, and asks for the input on every frame, so the player stands
//! still and does not turn, as if the keys were let go. Pressing the key again (or
//! switching the mod off) gives the input back and the camera returns to the
//! player. Needs `camera = true` and `wraps = ["render_camera"]`.
//!
//! The world only exists where the server sent chunks, around the player: the
//! camera can fly beyond, into the void.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use umicraft_modapi::{umicraft_mod, Camera, InitCx, KeyPressed, Mod, ModError, Setting};

/// The id of the key binding that starts and stops the free camera.
const KEY: &str = "toggle";
/// Its default binding.
const BINDING: &str = "KeyV";

/// Where the free camera is, while it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
}

impl View {
    /// The view after one frame: turned by the mouse, then moved `speed` blocks per
    /// second along the keys, horizontally as the player walks (Minecraft yaw: 0
    /// looks to +Z, 90 to -X) and straight up or down for jump and sneak.
    fn step(self, look: (f32, f32), keys: [f32; 3], speed: f64, seconds: f64) -> Self {
        let yaw = self.yaw + look.0;
        let pitch = (self.pitch + look.1).clamp(-90.0, 90.0);
        let [forward, strafe, up] = keys.map(f64::from);
        let (sin, cos) = f64::from(yaw).to_radians().sin_cos();
        let distance = speed * seconds;
        let eye = [
            self.eye[0] + (strafe * cos - forward * sin) * distance,
            self.eye[1] + up * distance,
            self.eye[2] + (forward * cos + strafe * sin) * distance,
        ];
        Self { eye, yaw, pitch }
    }
}

/// The layer: lets the client place the camera, then, while on, replaces it with
/// the free view and keeps the input for it.
fn fly(
    camera: &mut Camera<'_>,
    view: &mut Option<View>,
    on: bool,
    speed: Setting<f32>,
) -> Result<(), ModError> {
    camera.next()?;
    if !on {
        // Off: the camera is the player's again, and starts from there next time.
        *view = None;
        return Ok(());
    }
    let state = camera.state();
    let current = view.unwrap_or(View {
        eye: state.eye(),
        yaw: state.yaw(),
        pitch: state.pitch(),
    });
    let input = camera.input();
    let keys = [input.forward(), input.strafe(), input.up()];
    let look = (input.look_dx(), input.look_dy());
    let next = current.step(
        look,
        keys,
        f64::from(speed.get()),
        f64::from(input.frame_seconds()),
    );
    let state = camera.state_mut();
    state.set_eye(next.eye);
    state.set_yaw(next.yaw);
    state.set_pitch(next.pitch);
    *view = Some(next);
    camera.capture_input()
}

/// Everything lives in the closures.
struct FreeCam;

impl Mod for FreeCam {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let speed = cx.slider("speed", "Speed (blocks/s)", 1.0, 50.0, 1.0, 10.0)?;
        let on = Arc::new(AtomicBool::new(false));

        cx.key(KEY, "Start or stop the free camera", BINDING)?;
        let toggle = Arc::clone(&on);
        cx.observe::<KeyPressed>(0, move |event| {
            if event.key() == KEY {
                toggle.fetch_xor(true, Ordering::Relaxed);
            }
        })?;

        let mut view = None;
        cx.wrap_camera(move |camera| fly(camera, &mut view, on.load(Ordering::Relaxed), speed))?;
        Ok(FreeCam)
    }
}

umicraft_mod!(FreeCam);

#[cfg(test)]
mod tests {
    use super::*;

    const START: View = View {
        eye: [0.0, 64.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
    };

    #[test]
    fn forward_at_yaw_zero_goes_south_and_strafe_left_goes_east() {
        let ahead = START.step((0.0, 0.0), [1.0, 0.0, 0.0], 10.0, 0.5);
        assert!((ahead.eye[2] - 5.0).abs() < 1e-9 && ahead.eye[0].abs() < 1e-9);
        let left = START.step((0.0, 0.0), [0.0, 1.0, 0.0], 10.0, 0.5);
        assert!((left.eye[0] - 5.0).abs() < 1e-9 && left.eye[2].abs() < 1e-9);
    }

    #[test]
    fn the_mouse_turns_first_and_the_pitch_stops_at_the_vertical() {
        let turned = START.step((90.0, 120.0), [1.0, 0.0, 1.0], 2.0, 1.0);
        assert_eq!((turned.yaw, turned.pitch), (90.0, 90.0));
        // Yaw 90 looks to -X; jump lifts the eye whatever the pitch.
        assert!((turned.eye[0] + 2.0).abs() < 1e-9);
        assert!((turned.eye[1] - 66.0).abs() < 1e-9);
    }
}
