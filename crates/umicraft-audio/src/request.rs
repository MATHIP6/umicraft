// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game asks the audio thread to do: a sound request and the listener.

use std::sync::Arc;

use crate::category::SoundCategory;

/// A request to play one sound event, the counterpart of MCP `ISound` /
/// `PositionedSoundRecord`.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundRequest {
    /// The event name as in `sounds.json`, without the `minecraft:` domain
    /// (for example `dig.stone` or `random.click`).
    pub event: Arc<str>,
    /// Overrides the category of the index; `None` keeps the one `sounds.json`
    /// gives the event.
    pub category: Option<SoundCategory>,
    /// The world position of the source; `None` plays it at the listener with
    /// no attenuation (GUI clicks, `AttenuationType.NONE`).
    pub position: Option<[f64; 3]>,
    /// `ISound.getVolume`; above `1.0` it widens the audible distance.
    pub volume: f32,
    /// `ISound.getPitch`, combined with the pitch of the picked entry.
    pub pitch: f32,
    /// Ticks (20 Hz) to wait before the sound starts (`SoundManager.playDelayedSound`).
    pub delay_ticks: u32,
}

impl SoundRequest {
    /// A sound at the listener (no position) with volume and pitch `1.0`.
    #[must_use]
    pub fn new(event: impl Into<Arc<str>>) -> Self {
        Self {
            event: event.into(),
            category: None,
            position: None,
            volume: 1.0,
            pitch: 1.0,
            delay_ticks: 0,
        }
    }

    /// Places the source at a world position.
    #[must_use]
    pub fn at(mut self, position: [f64; 3]) -> Self {
        self.position = Some(position);
        self
    }

    /// Sets the volume.
    #[must_use]
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    /// Sets the pitch.
    #[must_use]
    pub fn with_pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    /// Delays the start by `ticks` game ticks.
    #[must_use]
    pub fn delayed(mut self, ticks: u32) -> Self {
        self.delay_ticks = ticks;
        self
    }
}

/// The ears of the player (`SoundManager.setListener`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    /// The eye position in world coordinates.
    pub position: [f64; 3],
    /// The view yaw in degrees, as the game stores it (`0` faces `+z`, `90`
    /// faces `-x`). The pitch is ignored: a stereo pan only needs the heading.
    pub yaw: f32,
}

impl Listener {
    /// The unit vector pointing to the right of the listener in the `x`/`z`
    /// plane. `SoundManager.setListener` derives the heading from
    /// `(cos(yaw + 90°), sin(yaw + 90°))`; the right side is that heading turned
    /// a quarter turn clockwise.
    #[must_use]
    pub fn right(&self) -> [f64; 2] {
        let yaw = f64::from(self.yaw).to_radians();
        [-yaw.cos(), -yaw.sin()]
    }

    /// The distance from the listener to a point.
    #[must_use]
    pub fn distance_to(&self, point: [f64; 3]) -> f64 {
        let [dx, dy, dz] = offset(self.position, point);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// The stereo pan of a point, `-1.0` (left) to `1.0` (right); `0.0` when
    /// the point is on the listener or straight ahead or behind.
    #[must_use]
    pub fn pan_to(&self, point: [f64; 3]) -> f32 {
        let [dx, _, dz] = offset(self.position, point);
        let flat = dx.hypot(dz);
        if flat < 1e-6 {
            return 0.0;
        }
        let [rx, rz] = self.right();
        // Narrowing to f32 is fine: the value is within -1..=1.
        ((dx * rx + dz * rz) / flat) as f32
    }
}

fn offset(from: [f64; 3], to: [f64; 3]) -> [f64; 3] {
    [to[0] - from[0], to[1] - from[1], to[2] - from[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(yaw: f32) -> Listener {
        Listener {
            position: [0.0; 3],
            yaw,
        }
    }

    #[test]
    fn builder_sets_fields() {
        let request = SoundRequest::new("dig.stone")
            .at([1.0, 2.0, 3.0])
            .with_volume(0.5)
            .with_pitch(1.5)
            .delayed(4);
        assert_eq!(&*request.event, "dig.stone");
        assert_eq!(request.position, Some([1.0, 2.0, 3.0]));
        assert_eq!(
            (request.volume, request.pitch, request.delay_ticks),
            (0.5, 1.5, 4)
        );
    }

    #[test]
    fn facing_south_the_west_is_right() {
        // Yaw 0 faces +z; a source at -x is on the right.
        assert!(listener(0.0).pan_to([-5.0, 0.0, 0.0]) > 0.99);
        assert!(listener(0.0).pan_to([5.0, 0.0, 0.0]) < -0.99);
    }

    #[test]
    fn rotating_the_view_rotates_the_pan() {
        // Yaw 90 faces -x; north (-z) is then on the right.
        assert!(listener(90.0).pan_to([0.0, 0.0, -5.0]) > 0.99);
    }

    #[test]
    fn ahead_and_on_top_are_centred() {
        assert!(listener(0.0).pan_to([0.0, 0.0, 5.0]).abs() < 1e-6);
        assert_eq!(listener(0.0).pan_to([0.0, 3.0, 0.0]), 0.0);
    }

    #[test]
    fn distance_is_euclidean() {
        assert_eq!(listener(0.0).distance_to([3.0, 4.0, 0.0]), 5.0);
    }
}
