// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: camera orbit (API 1.1).
//!
//! Shows the `render_camera` service: a wrapper lets the client place its camera
//! (`next`), then, while the orbit is on, moves the eye on a circle around where
//! the client put it and turns it to look back at that point. Only the picture
//! moves: the player's position, yaw and pitch, and what the server is told, do
//! not change. Wrapping needs `camera = true` and `wraps = ["render_camera"]`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use umicraft_modapi::{umicraft_mod, InitCx, KeyPressed, Mod, ModError};

/// The id of the key binding that starts and stops the orbit.
const ORBIT_KEY: &str = "orbit";
/// Its default binding.
const ORBIT_BINDING: &str = "KeyO";
/// Turns per second around the player.
const TURNS_PER_SECOND: f64 = 0.1;
/// How far above the player's eye the camera circles, in blocks.
const LIFT: f64 = 1.5;

/// Where the camera goes on the circle of `radius` blocks around `centre`, at
/// `angle` radians, and the yaw and pitch (Minecraft degrees) that look back at
/// `centre` from there.
fn orbit(centre: [f64; 3], radius: f64, angle: f64) -> ([f64; 3], f32, f32) {
    let eye = [
        centre[0] + radius * angle.cos(),
        centre[1] + LIFT,
        centre[2] + radius * angle.sin(),
    ];
    let (dx, dy, dz) = (centre[0] - eye[0], centre[1] - eye[1], centre[2] - eye[2]);
    // Minecraft yaw: 0 looks to +Z (south), 90 to -X (west); pitch is positive down.
    let yaw = (-dx).atan2(dz).to_degrees();
    let pitch = -dy.atan2(dx.hypot(dz)).to_degrees();
    (eye, yaw as f32, pitch as f32)
}

/// Everything lives in the closures.
struct CameraOrbit;

impl Mod for CameraOrbit {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let radius = cx.slider("radius", "Distance (blocks)", 2.0, 12.0, 0.5, 5.0)?;
        let on = Arc::new(AtomicBool::new(false));

        cx.key(ORBIT_KEY, "Start or stop the orbit", ORBIT_BINDING)?;
        let toggle = Arc::clone(&on);
        cx.observe::<KeyPressed>(0, move |event| {
            if event.key() == ORBIT_KEY {
                toggle.fetch_xor(true, Ordering::Relaxed);
            }
        })?;

        // Runs on the render thread every frame, after the client placed the
        // camera: the angle advances with the frame time, so the speed does not
        // depend on the frame rate.
        let mut angle = 0.0_f64;
        cx.wrap_camera(move |camera| {
            camera.next()?;
            if !on.load(Ordering::Relaxed) {
                return Ok(());
            }
            let turn = f64::from(camera.input().frame_seconds()) * TURNS_PER_SECOND;
            angle = (angle + turn * std::f64::consts::TAU) % std::f64::consts::TAU;
            let centre = camera.state().eye();
            let (eye, yaw, pitch) = orbit(centre, f64::from(radius.get()), angle);
            let state = camera.state_mut();
            state.set_eye(eye);
            state.set_yaw(yaw);
            state.set_pitch(pitch);
            Ok(())
        })?;
        Ok(Self)
    }
}

umicraft_mod!(CameraOrbit);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orbit_looks_back_at_the_centre() {
        let centre = [10.0, 64.0, -5.0];
        // East of the centre: looking west is yaw 90, down at the centre.
        let (eye, yaw, pitch) = orbit(centre, 4.0, 0.0);
        assert_eq!(eye, [14.0, 65.5, -5.0]);
        assert!((yaw - 90.0).abs() < 1e-4, "{yaw}");
        assert!(pitch > 0.0, "looks down at the centre: {pitch}");
        // South of the centre: looking north is yaw 180.
        let (_, yaw, _) = orbit(centre, 4.0, std::f64::consts::FRAC_PI_2);
        assert!((yaw.abs() - 180.0).abs() < 1e-3, "{yaw}");
    }
}
