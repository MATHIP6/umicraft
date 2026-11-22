// SPDX-License-Identifier: MIT OR Apache-2.0

//! The camera of the player, interpolated between the two newest frames the
//! game thread published.

use std::sync::Arc;
use std::time::Instant;

use umicraft_game::entity::EntityFrameData;
use umicraft_game::gamemode::GameMode;
use umicraft_game::raycast::look_direction;
use umicraft_game::view::{Perspective, THIRD_PERSON_DISTANCE};
use umicraft_game::RenderState;
use umicraft_render::{BorderView, FogContext, RenderEnvironment};

use crate::game::{Frame, LookSlot, ViewAngle, TICK};
use crate::scene::fog::FogLight;
use crate::scene::overlay::VignetteShade;
use crate::scene::overlay_state::OverlayInputs;
use crate::scene::weather::WeatherView;

/// The camera of the player, interpolated between the two newest frames.
#[derive(Debug, Default)]
pub(super) struct View {
    /// Active camera perspective.
    pub(super) perspective: Perspective,
    /// Player's actual yaw in radians (Minecraft simulation convention).
    pub(super) player_yaw: f32,
    /// Player's actual pitch in radians (Minecraft simulation convention).
    pub(super) player_pitch: f32,
    /// Third-person camera orbit distance from the eye, accounting for wall collisions.
    pub(super) orbit_distance: f64,
    /// The state of the previous tick.
    pub(super) previous: Option<RenderState>,
    /// The state of the newest tick.
    pub(super) current: Option<RenderState>,
    /// The instant of the newest tick.
    pub(super) tick_time: Option<Instant>,
    /// When the latest frame arrived on the render thread (anchors interpolation).
    pub(super) tick_instant: Option<Instant>,
    /// Whether the render thread is actively driving mouse look directly via `apply_look_delta`.
    pub(super) local_mouse_look: bool,
    /// Where the game thread leaves the mouse look.
    pub(super) look_slot: Option<LookSlot>,
    /// The newest server placement count the view snapped to (see
    /// [`Frame::placement`]).
    pub(super) placement: u32,
    /// The pitch of the local body in degrees, as of the newest look or tick.
    pub(super) body_pitch: Option<f32>,
    /// The newest signed world time in ticks.
    pub(super) world_time: i64,
    /// The dimension of the newest frame.
    pub(super) dimension: i8,
    /// Whether the world of the newest frame is a superflat one.
    pub(super) flat_world: bool,
    /// The game mode of the local player in the newest frame.
    pub(super) gamemode: GameMode,
    /// The newest announced world spawn point.
    pub(super) spawn: Option<(i32, i32, i32)>,
    /// The biome temperature at the player, for the sky colour.
    pub(super) sky_temperature: f32,
    /// The biome id at the player, `None` while unknown.
    pub(super) sky_biome: Option<u8>,
    /// The fog inputs of the newest frame (P7g).
    pub(super) fog: FogContext,
    /// The eye light of the fog, eased over ticks.
    pub(super) fog_light: FogLight,
    /// The world border of the newest frame.
    pub(super) border: Option<BorderView>,
    /// Rain, thunder, lightning and the precipitation columns of the newest frame.
    pub(super) weather: WeatherView,
    /// The particles of the newest frame (P8).
    pub(super) particles: Arc<[crate::scene::particles::ParticleDraw]>,
    /// What decides the screen overlays, of the newest frame (P8-f).
    pub(super) overlays: OverlayInputs,
    /// The facts the F3 screen prints, of the newest frame (P9-h).
    pub(super) debug: Arc<crate::debug::world::DebugWorld>,
    /// The vignette brightness, eased frame by frame.
    pub(super) vignette: VignetteShade,
}

impl View {
    /// Computes the camera yaw, pitch, and world position from the player's true angles and eye position.
    fn camera_pose_from_player(&self, eye: glam::DVec3) -> (f32, f32, glam::DVec3) {
        match self.perspective {
            Perspective::FirstPerson => (self.player_yaw, self.player_pitch, eye),
            Perspective::ThirdPersonBack => {
                let dir = look_direction(self.player_yaw, self.player_pitch) * -1.0;
                (
                    self.player_yaw,
                    self.player_pitch,
                    eye + dir * self.orbit_distance,
                )
            }
            Perspective::ThirdPersonFront => {
                let dir = look_direction(self.player_yaw, self.player_pitch) * 1.0;
                (
                    (self.player_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU),
                    -self.player_pitch,
                    eye + dir * self.orbit_distance,
                )
            }
        }
    }

    /// Stores a new frame.
    pub(super) fn push(&mut self, frame: Frame) {
        let perspective_changed = self.perspective != frame.perspective;
        self.perspective = frame.perspective;

        // Preserve previous state from the preceding tick so that eye interpolation
        // (moving, jumping, falling, knockback, strafing) remains continuous across
        // perspective switches, eliminating 50ms freezing or visual stair-stepping.
        self.previous = self.current;

        self.orbit_distance = if frame.perspective == Perspective::FirstPerson {
            0.0
        } else {
            let dist = (frame.state.position - frame.state.eye).length();
            if dist > 0.1 {
                dist
            } else {
                THIRD_PERSON_DISTANCE
            }
        };

        let mut state = frame.state;
        if self.local_mouse_look || perspective_changed {
            if !self.local_mouse_look {
                self.player_yaw = frame.local_body.yaw.to_radians();
                self.player_pitch = frame.local_body.pitch.to_radians();
                self.body_pitch = Some(frame.local_body.pitch);
            }
            let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(state.eye);
            let orbit = (cam_pos - state.eye).as_vec3();
            state.yaw = cam_yaw;
            state.pitch = cam_pitch;
            state.position = cam_pos;
            state.orbit_start = orbit;
        } else {
            self.player_yaw = frame.local_body.yaw.to_radians();
            self.player_pitch = frame.local_body.pitch.to_radians();
            self.body_pitch = Some(frame.local_body.pitch);
        }
        self.current = Some(state);

        self.tick_instant = Some(Instant::now());
        self.tick_time = Some(frame.tick_time);
        self.world_time = frame.world_time;
        self.dimension = frame.dimension;
        self.flat_world = frame.flat_world;
        self.gamemode = frame.gamemode;
        self.spawn = frame.spawn;
        self.sky_temperature = frame.sky_temperature;
        self.sky_biome = frame.sky_biome;
        self.fog = frame.fog;
        self.fog_light.tick(frame.fog.light_brightness);
        self.border = Some(frame.border);
        self.weather = frame.weather;
        self.particles = frame.particles;
        self.overlays = frame.overlays;
        self.debug = frame.debug;

        if frame.placement != self.placement {
            self.catch_up_teleport(frame.placement, &frame.local_body);
        }
    }

    /// Snaps the view to a teleport whose [`RenderEvent::Teleport`] the full
    /// frame queue dropped.
    ///
    /// The mouse look of the window owns the view yaw once it moved
    /// (`local_mouse_look`), so without this the camera kept the look from
    /// before the teleport while the physics walked with the server's: the
    /// keys then moved the player sideways or backwards on screen. The body
    /// of the frame carries the look of the game thread, server look included.
    ///
    /// [`RenderEvent::Teleport`]: crate::game::RenderEvent::Teleport
    fn catch_up_teleport(&mut self, placement: u32, body: &EntityFrameData) {
        self.placement = placement;
        let Some(eye) = self.current.as_ref().map(|current| current.eye) else {
            return;
        };
        self.teleport(
            eye,
            ViewAngle::To(body.yaw.to_radians()),
            ViewAngle::To(body.pitch.to_radians()),
        );
    }

    /// Applies the mouse look the game thread left in the slot since the last
    /// call: the camera turns at once and the body looks up or down with it.
    pub(super) fn take_look(&mut self) {
        let Some(look) = self.look_slot.as_ref().and_then(|slot| slot.take()) else {
            return;
        };
        if !self.local_mouse_look {
            self.body_pitch = Some(look.body_pitch);
            if self.perspective == Perspective::ThirdPersonFront {
                self.player_yaw =
                    (look.view.yaw - std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
                self.player_pitch = -look.view.pitch;
            } else {
                self.player_yaw = look.view.yaw;
                self.player_pitch = look.view.pitch;
            }
            if let Some(eye) = self.current.as_ref().map(|c| c.eye) {
                let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(eye);
                let orbit = (cam_pos - eye).as_vec3();
                if let Some(current) = self.current.as_mut() {
                    current.yaw = cam_yaw;
                    current.pitch = cam_pitch;
                    current.position = cam_pos;
                    current.orbit_start = orbit;
                }
            }
        }
    }

    /// Immediately applies relative yaw and pitch deltas (in radians) to the current
    /// render view angles.
    ///
    /// This allows zero-latency raw mouse look on the render thread before the
    /// update has made a full round trip to the game thread and back.
    pub(super) fn apply_look_delta(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.local_mouse_look = true;
        const MAX_PITCH: f32 = 90.0_f32.to_radians();
        self.player_yaw = (self.player_yaw + yaw_delta).rem_euclid(std::f32::consts::TAU);
        self.player_pitch = (self.player_pitch + pitch_delta).clamp(-MAX_PITCH, MAX_PITCH);
        self.body_pitch = Some(self.player_pitch.to_degrees());

        if let Some(eye) = self.current.as_ref().map(|c| c.eye) {
            let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(eye);
            let orbit = (cam_pos - eye).as_vec3();
            if let Some(current) = self.current.as_mut() {
                current.yaw = cam_yaw;
                current.pitch = cam_pitch;
                current.position = cam_pos;
                current.orbit_start = orbit;
            }
        }
    }

    /// Snaps the render camera immediately to the exact eye position and angles on a
    /// server teleport (packet 0x08), resetting previous state to eliminate visual gliding.
    ///
    /// A relative angle turns the view the window has now: the mouse may have
    /// turned it after the game thread built the event.
    pub(super) fn teleport(&mut self, position: glam::DVec3, yaw: ViewAngle, pitch: ViewAngle) {
        const MAX_PITCH: f32 = 90.0_f32.to_radians();
        self.player_yaw = yaw.applied_to(self.player_yaw);
        self.player_pitch = pitch
            .applied_to(self.player_pitch)
            .clamp(-MAX_PITCH, MAX_PITCH);
        self.body_pitch = Some(self.player_pitch.to_degrees());

        let new_eye = position;
        let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(new_eye);
        let orbit = (cam_pos - new_eye).as_vec3();

        if let Some(current) = self.current.as_mut() {
            current.eye = new_eye;
            current.position = cam_pos;
            current.yaw = cam_yaw;
            current.pitch = cam_pitch;
            current.orbit_start = orbit;
        } else {
            self.current = Some(RenderState {
                position: cam_pos,
                eye: new_eye,
                orbit_start: orbit,
                yaw: cam_yaw,
                pitch: cam_pitch,
                on_ground: true,
                effects: Default::default(),
            });
        }
        self.previous = self.current;
        self.tick_instant = Some(Instant::now());
    }

    /// The interpolation fraction of the two newest ticks for `now`.
    ///
    /// The value is clamped to `0.0..=1.0` and is the same fraction
    /// [`View::sample`] uses, so the camera and the world clock stay in step.
    pub(super) fn partial_tick(&self, now: Instant) -> f32 {
        if self.previous.is_none() {
            return 0.0;
        }
        let Some(tick_time) = self.tick_time else {
            return 0.0;
        };
        let tick_secs = TICK.as_secs_f32();
        let elapsed = if now < tick_time {
            if let Some(instant) = self.tick_instant {
                now.saturating_duration_since(instant).as_secs_f32()
            } else {
                0.0
            }
        } else {
            let from_tick = now.saturating_duration_since(tick_time).as_secs_f32();
            if from_tick <= tick_secs {
                from_tick
            } else if let Some(instant) = self.tick_instant {
                // If the snapshot arrived late due to network transit or catch-up,
                // interpolate based on wall-clock time since it was accepted on the
                // render thread so frames are not frozen at 1.0 during network jitter.
                let from_arrival = now.saturating_duration_since(instant).as_secs_f32();
                from_arrival.min(tick_secs)
            } else {
                from_tick
            }
        };
        (elapsed / tick_secs).clamp(0.0, 1.0)
    }

    /// The render environment for `now`.
    ///
    /// The world clock and dimension are the newest values the game thread
    /// published; the render thread never advances the clock by itself
    /// (ADR-0008 §4).
    pub(super) fn environment(&self, now: Instant, view_distance: u8) -> RenderEnvironment {
        let partial_tick = self.partial_tick(now);
        RenderEnvironment {
            sky_temperature: self.sky_temperature,
            fog: FogContext {
                light_brightness: self.fog_light.at(partial_tick),
                ..self.fog
            },
            rain_strength: self.weather.rain_strength,
            thunder_strength: self.weather.thunder_strength,
            lightning_ticks: self.weather.lightning_ticks,
            flat_world: self.flat_world,
            ..RenderEnvironment::new(self.dimension, self.world_time, partial_tick, view_distance)
        }
    }

    /// The interpolated state for `now`.
    ///
    /// The renderer shows the simulation one tick behind and interpolates
    /// between the two newest ticks, so the motion is smooth and independent
    /// of the frame rate (GAM-8).
    pub(super) fn sample(&self, now: Instant) -> Option<RenderState> {
        let current = self.current?;
        let Some(previous) = self.previous else {
            return Some(current);
        };
        let alpha = self.partial_tick(now);
        // `orientCamera`: the interpolated eye plus the orbit of the current
        // angles. Interpolating the camera positions of two ticks instead
        // would drag the third-person camera behind its own angle.
        // The eye is lerped in f64: casting it first would round it to the
        // 6-12 cm grain of f32 around x = 1 000 000 (F2-1a). The orbit is a
        // small offset, so f32 is exact enough for it.
        let eye = previous.eye.lerp(current.eye, f64::from(alpha));
        let orbit = current
            .orbit_start
            .lerp((current.position - current.eye).as_vec3(), alpha);
        Some(RenderState {
            position: eye + orbit.as_dvec3(),
            eye,
            orbit_start: current.orbit_start,
            yaw: current.yaw,
            pitch: current.pitch,
            on_ground: current.on_ground,
            effects: previous.effects.lerp(current.effects, alpha),
        })
    }
}
