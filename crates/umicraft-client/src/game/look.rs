// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mouse look, published between two ticks through a lock-free slot.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use glam::{DVec3, Vec3};
use umicraft_game::RenderState;

/// The camera angles of a player state, in radians.
///
/// The game logic uses the Minecraft angles (yaw zero looks towards +Z, a
/// positive pitch looks down), while the renderer camera uses the opposite
/// signs: the yaw is turned by half a turn and the pitch is flipped.
#[must_use]
pub fn camera_angles(state: &RenderState) -> (f32, f32) {
    (state.yaw + std::f32::consts::PI, -state.pitch)
}

/// What the mouse look changed since the newest tick.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    /// The view of the player: it replaces the newest frame's view without
    /// restarting the interpolation.
    pub view: RenderState,
    /// The pitch of the local body in degrees. `Entity.setAngles` moves the
    /// pitch and its previous value together, every frame, so the body looks
    /// where the mouse points at once; the yaw of the body and the head is
    /// the one of the tick, interpolated.
    pub body_pitch: f32,
}

/// Lock-free single-producer, single-consumer seqlock slot for the newest [`Look`].
///
/// The game thread writes the latest mouse-look angles via [`AtomicLookSlot::store`]
/// without taking a lock, and the render thread consumes the newest unread look via
/// [`AtomicLookSlot::take`]. When no mouse movement happened since the last frame,
/// [`AtomicLookSlot::take`] completes with two atomic loads on a 64-byte aligned
/// cache line without dirtying it.
#[derive(Debug, Default)]
#[repr(align(64))]
pub struct AtomicLookSlot {
    /// Even when quiescent (`2, 4, 6, ...`), odd while the writer updates fields.
    seq: AtomicU64,
    /// Sequence number of the last look consumed by `take()`.
    read_seq: AtomicU64,
    pos_x: AtomicU64,
    pos_y: AtomicU64,
    pos_z: AtomicU64,
    eye_x: AtomicU64,
    eye_y: AtomicU64,
    eye_z: AtomicU64,
    orbit_x: AtomicU32,
    orbit_y: AtomicU32,
    orbit_z: AtomicU32,
    yaw: AtomicU32,
    pitch: AtomicU32,
    body_pitch: AtomicU32,
    on_ground: AtomicBool,
}

impl AtomicLookSlot {
    /// Publishes `look` as the newest mouse look, overwriting any unread one.
    pub fn store(&self, look: Look) {
        let s1 = self.seq.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        self.pos_x
            .store(look.view.position.x.to_bits(), Ordering::Relaxed);
        self.pos_y
            .store(look.view.position.y.to_bits(), Ordering::Relaxed);
        self.pos_z
            .store(look.view.position.z.to_bits(), Ordering::Relaxed);
        self.eye_x
            .store(look.view.eye.x.to_bits(), Ordering::Relaxed);
        self.eye_y
            .store(look.view.eye.y.to_bits(), Ordering::Relaxed);
        self.eye_z
            .store(look.view.eye.z.to_bits(), Ordering::Relaxed);
        self.orbit_x
            .store(look.view.orbit_start.x.to_bits(), Ordering::Relaxed);
        self.orbit_y
            .store(look.view.orbit_start.y.to_bits(), Ordering::Relaxed);
        self.orbit_z
            .store(look.view.orbit_start.z.to_bits(), Ordering::Relaxed);
        self.yaw.store(look.view.yaw.to_bits(), Ordering::Relaxed);
        self.pitch
            .store(look.view.pitch.to_bits(), Ordering::Relaxed);
        self.body_pitch
            .store(look.body_pitch.to_bits(), Ordering::Relaxed);
        self.on_ground.store(look.view.on_ground, Ordering::Relaxed);
        self.seq.store(s1.wrapping_add(1), Ordering::Release);
    }

    /// Consumes the newest [`Look`] if one was stored since the last `take`.
    #[must_use]
    pub fn take(&self) -> Option<Look> {
        let last_read = self.read_seq.load(Ordering::Relaxed);
        let first_seq = self.seq.load(Ordering::Acquire);
        if first_seq == last_read {
            return None;
        }
        loop {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            if s1 == last_read {
                return None;
            }
            let position = DVec3::new(
                f64::from_bits(self.pos_x.load(Ordering::Relaxed)),
                f64::from_bits(self.pos_y.load(Ordering::Relaxed)),
                f64::from_bits(self.pos_z.load(Ordering::Relaxed)),
            );
            let eye = DVec3::new(
                f64::from_bits(self.eye_x.load(Ordering::Relaxed)),
                f64::from_bits(self.eye_y.load(Ordering::Relaxed)),
                f64::from_bits(self.eye_z.load(Ordering::Relaxed)),
            );
            let orbit_start = Vec3::new(
                f32::from_bits(self.orbit_x.load(Ordering::Relaxed)),
                f32::from_bits(self.orbit_y.load(Ordering::Relaxed)),
                f32::from_bits(self.orbit_z.load(Ordering::Relaxed)),
            );
            let yaw = f32::from_bits(self.yaw.load(Ordering::Relaxed));
            let pitch = f32::from_bits(self.pitch.load(Ordering::Relaxed));
            let body_pitch = f32::from_bits(self.body_pitch.load(Ordering::Relaxed));
            let on_ground = self.on_ground.load(Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Acquire);
            let s2 = self.seq.load(Ordering::Relaxed);
            if s1 == s2 {
                self.read_seq.store(s1, Ordering::Release);
                return Some(Look {
                    view: RenderState {
                        position,
                        eye,
                        orbit_start,
                        yaw,
                        pitch,
                        on_ground,
                        effects: umicraft_game::view::ViewEffects::default(),
                    },
                    body_pitch,
                });
            }
            std::hint::spin_loop();
        }
    }
}

/// The newest [`Look`], shared by the game thread and the render thread.
///
/// A mouse look overwrites it instead of queueing an event: the mouse sends
/// hundreds of looks a second, and as events they filled the frame queue until
/// the tick frames, which carry the body of the player and the entities, were
/// dropped.
pub type LookSlot = Arc<AtomicLookSlot>;
