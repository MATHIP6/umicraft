// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-game
//!
//! The pure game logic of a 1.8 client: the movement rules, the block
//! collision, the client-side prediction with server reconciliation, the 20 Hz
//! tick and the block ray cast.
//!
//! The crate has no I/O, no GPU, no async runtime and no networking: it only
//! depends on [`umicraft_world`] for the block lookups and on `glam` for the
//! maths. That keeps the physics testable without a server and without a
//! window, which is what the physics conformance tests of spec §11 need.
//!
//! ```
//! use umicraft_game::{GameState, InputIntent};
//! use umicraft_world::World;
//!
//! let world = World::new(0);
//! let mut state = GameState::new(glam::DVec3::new(0.0, 64.0, 0.0));
//! state.tick(&InputIntent::walking(0.0), &world);
//! let view = state.interpolated(0.5);
//! assert_eq!(view.yaw, 0.0);
//! ```
//!
//! ## The tick
//!
//! [`GameState::tick`] advances the simulation by exactly one tick. It reads
//! no wall clock and uses no randomness: the same [`InputIntent`] sequence
//! against the same world always produces the same trajectory.
//!
//! The rules are the 1.8 ones and every constant is cited in
//! `docs/sources.md`; the physics itself is documented in [`physics`], the
//! collision in [`collision`], the block geometry in [`shapes`], the
//! reconciliation in [`prediction`], the block targeting in [`raycast`] and
//! the gamemodes in [`gamemode`], the block interaction rules in
//! [`interaction`], the serverbound packets of a tick in [`traffic`] and the protocol state kept
//! for later milestones (tab list, scoreboard, effects, border, titles) in
//! [`server`].

mod block_data;
pub mod chat_component;
pub mod chat_input;
pub mod collision;
pub mod digging;
pub mod entity;
pub mod gamemode;
pub mod hand;
pub mod input;
pub mod interaction;
pub mod inventory;
pub mod item_nbt;
pub mod item_use;
pub mod items;
pub mod media;
pub mod movement;
pub mod nbt;
pub mod physics;
pub mod pick;
pub mod player_list;
pub mod prediction;
pub mod raycast;
pub mod riding;
pub mod scoreboard;
pub mod selection;
pub mod server;
pub mod shapes;
pub mod title;
pub mod traffic;
pub mod view;

use glam::{DVec3, Vec3};

pub use entity::{
    BossInfo, BossTracker, EntityFlags, EntityId, EntityItem, EntityKind, EntityRecord,
    EntityRenderSnapshot, EntityStore,
};
pub use input::InputIntent;
pub use physics::Player;
pub use prediction::{PendingTick, PredictionHistory};
pub use shapes::{Aabb, BlockSource};

/// The distance within which a server correction is matched against a
/// pending tick.
///
/// The server does not tell the client which tick a correction belongs to, so
/// the closest recorded position is used. The tolerance keeps a teleport
/// (which is far away from any predicted position) from being mistaken for a
/// normal correction.
pub const BASE_MATCH_TOLERANCE: f64 = 1.0;

/// An authoritative correction from the server.
///
/// The client receives it with `Player Position and Look` (0x08) or
/// `Entity Teleport` (0x18) and applies it on the game thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerCorrection {
    /// The authoritative X of the player's feet.
    pub x: f64,
    /// The authoritative Y of the player's feet.
    pub y: f64,
    /// The authoritative Z of the player's feet.
    pub z: f64,
    /// The authoritative horizontal view angle in radians.
    pub yaw: f32,
    /// The authoritative vertical view angle in radians.
    pub pitch: f32,
    /// Whether the server has the player on the ground.
    pub on_ground: bool,
}

impl ServerCorrection {
    /// Creates a correction.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64, yaw: f32, pitch: f32, on_ground: bool) -> Self {
        Self {
            x,
            y,
            z,
            yaw,
            pitch,
            on_ground,
        }
    }

    /// The authoritative position of the player's feet.
    #[must_use]
    pub fn position(&self) -> DVec3 {
        DVec3::new(self.x, self.y, self.z)
    }
}

/// What the renderer needs: plain data, no game types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderState {
    /// The eye position in world space.
    pub position: Vec3,
    /// The horizontal view angle in radians.
    pub yaw: f32,
    /// The vertical view angle in radians.
    pub pitch: f32,
    /// Whether the player rests on the ground.
    pub on_ground: bool,
    /// The first-person view bobbing and field-of-view modifier.
    pub effects: view::ViewEffects,
}

/// The mutable game state owned by the game thread.
///
/// The state is not shared: it lives on the game thread and the render thread
/// only ever receives [`RenderState`] snapshots.
#[derive(Debug, Clone)]
pub struct GameState {
    player: Player,
    previous_position: DVec3,
    tick: u64,
    history: PredictionHistory,
}

impl GameState {
    /// Creates a game state for a player at the given feet position.
    #[must_use]
    pub fn new(position: DVec3) -> Self {
        Self {
            player: Player::new(position),
            previous_position: position,
            tick: 0,
            history: PredictionHistory::new(),
        }
    }

    /// Advances the simulation by exactly one tick.
    ///
    /// The intent is latched, the 1.8 movement rules are applied and the
    /// result is recorded in the pending-input history, so that a later
    /// server correction can be replayed.
    ///
    /// The function is deterministic: it reads no clock and uses no
    /// randomness.
    ///
    /// The tick allocates nothing: the pending-input history is a fixed
    /// capacity ring buffer and the physics only uses stack values.
    pub fn tick<S: BlockSource + ?Sized>(&mut self, intent: &InputIntent, world: &S) {
        self.previous_position = self.player.position;
        physics::step(&mut self.player, intent, world);
        self.tick += 1;
        self.history.push(PendingTick {
            intent: *intent,
            position: self.player.position,
            velocity: self.player.velocity,
            on_ground: self.player.on_ground,
            yaw: self.player.yaw,
            pitch: self.player.pitch,
        });
    }

    /// Applies an authoritative server position and replays the pending
    /// inputs.
    ///
    /// The correction replaces the predicted position. The pending inputs
    /// that the server had not seen yet are replayed on top of it, so the
    /// client ends up where the server would have put it. A correction that
    /// does not match any pending tick is a teleport: the position is taken
    /// over as it is and the pending inputs are dropped.
    pub fn apply_correction<S: BlockSource + ?Sized>(
        &mut self,
        correction: &ServerCorrection,
        world: &S,
    ) {
        let authoritative = correction.position();
        let base = self
            .history
            .find_base(authoritative, BASE_MATCH_TOLERANCE)
            .and_then(|index| self.history.get(index).copied().map(|tick| (index, tick)));

        match base {
            Some((index, state)) => {
                self.player.position = authoritative;
                self.player.velocity = state.velocity;
                self.player.on_ground = correction.on_ground;
                self.player.yaw = correction.yaw;
                self.player.pitch = correction.pitch;

                // The matched tick is the one the server confirmed: its
                // position *is* the authoritative one, so its input has
                // already been applied and must not be replayed. The ticks
                // that come after it are the ones the server has not seen
                // yet, and they are replayed on top of the correction.
                let mut replayed = PredictionHistory::new();
                replayed.push(PendingTick {
                    intent: state.intent,
                    position: authoritative,
                    velocity: state.velocity,
                    on_ground: correction.on_ground,
                    yaw: correction.yaw,
                    pitch: correction.pitch,
                });

                let mut previous = authoritative;
                for replay_index in (index + 1)..self.history.len() {
                    let Some(tick) = self.history.get(replay_index).copied() else {
                        break;
                    };
                    previous = self.player.position;
                    physics::step(&mut self.player, &tick.intent, world);
                    replayed.push(PendingTick {
                        intent: tick.intent,
                        position: self.player.position,
                        velocity: self.player.velocity,
                        on_ground: self.player.on_ground,
                        yaw: self.player.yaw,
                        pitch: self.player.pitch,
                    });
                }
                self.previous_position = previous;
                // The replayed ticks replace the old history: the next
                // correction must be able to find a base among them.
                self.history = replayed;
            }
            None => {
                self.player.position = authoritative;
                self.player.velocity = DVec3::ZERO;
                self.player.on_ground = correction.on_ground;
                self.player.yaw = correction.yaw;
                self.player.pitch = correction.pitch;
                self.previous_position = authoritative;
                self.history.clear();
            }
        }
    }

    /// The interpolated view of the player for the renderer.
    ///
    /// `alpha` is the fraction of a tick that elapsed since the last tick:
    /// `0.0` is the state of the previous tick, `1.0` the current one.
    #[must_use]
    pub fn interpolated(&self, alpha: f32) -> RenderState {
        let alpha = f64::from(alpha.clamp(0.0, 1.0));
        let feet = self.previous_position.lerp(self.player.position, alpha);
        let eye_height = if self.player.sneaking {
            physics::EYE_HEIGHT - physics::SNEAKING_EYE_HEIGHT_OFFSET
        } else {
            physics::EYE_HEIGHT
        };
        let eye = feet + DVec3::Y * eye_height;
        RenderState {
            position: Vec3::new(eye.x as f32, eye.y as f32, eye.z as f32),
            yaw: self.player.yaw,
            pitch: self.player.pitch,
            on_ground: self.player.on_ground,
            effects: view::ViewEffects::at(&self.player, alpha as f32),
        }
    }

    /// Applies the flight abilities the server announced
    /// (`Player Abilities`, 0x39) and the no-clip flag of the gamemode.
    pub fn set_abilities(&mut self, allow_flying: bool, flying: bool, fly_speed: f32) {
        self.player.allow_flying = allow_flying;
        self.player.flying = flying;
        self.player.fly_speed = fly_speed;
    }

    /// Applies the movement stats of the server state (speed attribute, Jump
    /// Boost, Blindness), the walking speed of the abilities and whether the
    /// food level allows a sprint.
    pub fn set_movement(
        &mut self,
        stats: movement::MovementStats,
        walk_speed: f32,
        well_fed: bool,
    ) {
        self.player.movement_speed = stats.speed;
        self.player.jump_boost = stats.jump_boost;
        self.player.blinded = stats.blinded;
        self.player.walk_speed = walk_speed;
        self.player.well_fed = well_fed;
    }

    /// Sets the Depth Strider level of the worn boots.
    pub fn set_depth_strider(&mut self, level: u8) {
        self.player.depth_strider = level;
    }

    /// Replaces the motion of the player (`Entity Velocity`, MCP
    /// `Entity.setVelocity`).
    pub fn set_velocity(&mut self, velocity: DVec3) {
        self.player.velocity = velocity;
    }

    /// Adds to the motion of the player (the push of an `Explosion`).
    pub fn add_velocity(&mut self, velocity: DVec3) {
        self.player.velocity += velocity;
    }

    /// Puts the player on the seat of its vehicle instead of simulating it.
    ///
    /// MCP `EntityPlayerSP.onUpdate` calls `Entity.updateRidden` while the
    /// player rides: the motion is cleared and the vehicle sets the position.
    /// The prediction history is dropped, the server never corrects a rider.
    pub fn ride(&mut self, feet: DVec3) {
        self.previous_position = self.player.position;
        self.player.position = feet;
        self.player.velocity = DVec3::ZERO;
        self.tick += 1;
        self.history.clear();
    }

    /// Makes the player walk through blocks (spectator mode).
    pub fn set_no_clip(&mut self, no_clip: bool) {
        self.player.no_clip = no_clip;
    }

    /// The kinematic state of the player.
    #[must_use]
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// The feet position of the player on the previous tick.
    #[must_use]
    pub fn previous_position(&self) -> DVec3 {
        self.previous_position
    }

    /// The feet position of the player.
    #[must_use]
    pub fn position(&self) -> DVec3 {
        self.player.position
    }

    /// The number of ticks that were simulated.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// The pending-input history.
    #[must_use]
    pub fn history(&self) -> &PredictionHistory {
        &self.history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use umicraft_world::BlockState;

    /// A block source backed by a hash map, for tests.
    #[derive(Debug, Default)]
    struct TestBlocks {
        blocks: HashMap<(i32, i32, i32), BlockState>,
    }

    impl TestBlocks {
        fn flat() -> Self {
            let mut blocks = Vec::new();
            for x in -16..16 {
                for z in -16..16 {
                    blocks.push((x, 0, z, BlockState::new(1, 0)));
                }
            }
            Self::with(&blocks)
        }

        fn with(blocks: &[(i32, i32, i32, BlockState)]) -> Self {
            let mut source = Self::default();
            for (x, y, z, state) in blocks {
                source.blocks.insert((*x, *y, *z), *state);
            }
            source
        }
    }

    impl BlockSource for TestBlocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.blocks
                .get(&(x, y, z))
                .copied()
                .unwrap_or(BlockState::AIR)
        }
    }

    #[test]
    fn a_rider_is_put_on_the_seat_and_its_motion_is_cleared() {
        let mut state = GameState::new(DVec3::new(0.0, 5.0, 0.0));
        state.set_velocity(DVec3::new(0.3, 0.2, 0.1));
        state.ride(DVec3::new(4.0, 6.0, 4.0));
        assert_eq!(state.position(), DVec3::new(4.0, 6.0, 4.0));
        assert_eq!(state.previous_position(), DVec3::new(0.0, 5.0, 0.0));
        assert_eq!(state.player().velocity, DVec3::ZERO);
    }

    #[test]
    fn the_simulation_is_deterministic() {
        let world = TestBlocks::flat();
        let intents: Vec<InputIntent> = (0..30)
            .map(|tick| {
                let mut intent = InputIntent::walking(0.0);
                intent.jump = tick % 7 == 0;
                intent
            })
            .collect();

        let run = |world: &TestBlocks| {
            let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
            for intent in &intents {
                state.tick(intent, world);
            }
            state.position()
        };

        assert_eq!(run(&world), run(&world));
    }

    #[test]
    fn the_trajectory_of_a_walk_is_the_analytic_one() {
        // The player walks on a flat stone floor from z = 0 towards +Z (yaw 0).
        //
        // On the ground the friction is the slipperiness of stone times 0.91,
        // `f = 0.6 * 0.91` (MCP `EntityLivingBase.java:1599-1604`); the
        // movement factor is `0.16277136 / f^3` (line 1606), the movement
        // speed attribute 0.1 (`EntityPlayer.java:193`) and the input is
        // damped by 0.98 (line 2020). Every tick adds the acceleration to
        // the motion, moves by it, then damps it by `f` (line 1670).
        //
        // The first two ticks are still in the air: the player starts with
        // `onGround == false` and only lands once the gravity has pulled it
        // onto the floor, so those ticks use the air control factor of 0.02
        // (`EntityLivingBase.java:1615`) and the air friction of 0.91.
        let world = TestBlocks::flat();
        let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);

        for _ in 0..20 {
            state.tick(&intent, &world);
        }

        let ground = 0.6 * physics::GROUND_FRICTION;
        let mut motion = 0.0;
        let mut expected = 0.0;
        for tick in 0..20 {
            let (acceleration, friction) = if tick < 2 {
                (0.98 * 0.02, physics::GROUND_FRICTION)
            } else {
                (0.98 * 0.1 * 0.162_771_36 / ground.powi(3), ground)
            };
            motion += acceleration;
            expected += motion;
            motion *= friction;
        }
        assert!(
            (state.position().z - expected).abs() < 1e-12,
            "expected {expected}, got {}",
            state.position().z
        );
    }

    #[test]
    fn a_correction_converges_back_to_the_authoritative_position() {
        let world = TestBlocks::flat();
        let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        for _ in 0..10 {
            state.tick(&intent, &world);
        }
        let predicted = state.position();

        // The server was three ticks behind and disagrees by a little.
        let authoritative = DVec3::new(predicted.x, predicted.y, predicted.z - 0.5);
        let correction = ServerCorrection::new(
            authoritative.x,
            authoritative.y,
            authoritative.z,
            0.0,
            0.0,
            true,
        );
        state.apply_correction(&correction, &world);

        // The client is now anchored at the authoritative position and the
        // pending inputs were replayed on top of it, so the remaining error
        // is the distance the server did not know about yet.
        let error = (state.position() - authoritative).length();
        assert!(
            error < 1.0,
            "the client must stay close to the server, got {error}"
        );
        assert!(
            (state.position().z - authoritative.z) > 0.0,
            "the pending inputs must have been replayed"
        );
    }

    #[test]
    fn a_correction_replays_the_pending_ticks_without_replaying_the_base() {
        // Regression: the matched tick is the one the server confirmed, so
        // its input has already been applied. Replaying it as well moved the
        // player one tick too far.
        let world = TestBlocks::flat();
        let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        for _ in 0..10 {
            state.tick(&intent, &world);
        }
        let predicted = state.position();

        // The server confirms the position of the fifth tick.
        let base = state.history().get(5).expect("a recorded tick").position;
        let correction = ServerCorrection::new(base.x, base.y, base.z, 0.0, 0.0, true);
        state.apply_correction(&correction, &world);

        // The five ticks that follow the confirmed one are replayed, so the
        // player ends up where the prediction had put it, not one tick
        // further.
        let expected = predicted.z;
        assert!(
            (state.position().z - expected).abs() < 1e-12,
            "expected {expected}, got {}",
            state.position().z
        );
    }

    #[test]
    fn a_correction_keeps_the_replayed_ticks_for_the_next_correction() {
        // Regression: the history used to be cleared right after the
        // replay, so the next correction could not find a base any more.
        let world = TestBlocks::flat();
        let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        for _ in 0..10 {
            state.tick(&intent, &world);
        }
        let base = state.history().get(5).expect("a recorded tick").position;
        let correction = ServerCorrection::new(base.x, base.y, base.z, 0.0, 0.0, true);
        state.apply_correction(&correction, &world);

        assert!(
            state.history().len() >= 5,
            "the replayed ticks must stay in the history, got {}",
            state.history().len()
        );
        // A second correction on the same base lands on the same place: the
        // replay is deterministic and the base is still in the history.
        let after_first = state.position();
        state.apply_correction(&correction, &world);
        assert!(
            (state.position().z - after_first.z).abs() < 1e-12,
            "the second correction must not drift, got {} instead of {}",
            state.position().z,
            after_first.z
        );
        assert!(
            state.position().z > base.z,
            "the pending ticks must have been replayed again"
        );
    }

    #[test]
    fn a_teleport_moves_the_player_without_replaying() {
        let world = TestBlocks::flat();
        let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
        for _ in 0..10 {
            state.tick(&InputIntent::walking(0.0), &world);
        }
        let correction = ServerCorrection::new(100.0, 70.0, 100.0, 1.0, 0.0, true);
        state.apply_correction(&correction, &world);
        assert_eq!(state.position(), DVec3::new(100.0, 70.0, 100.0));
        assert!(state.history().is_empty());
    }

    #[test]
    fn the_render_state_is_interpolated() {
        let world = TestBlocks::flat();
        let mut state = GameState::new(DVec3::new(0.0, 1.0, 0.0));
        state.tick(&InputIntent::walking(0.0), &world);
        let previous = state.interpolated(0.0).position;
        let current = state.interpolated(1.0).position;
        let middle = state.interpolated(0.5).position;
        assert!((middle.z - (previous.z + current.z) / 2.0).abs() < 1e-5);
        // The eye sits 1.62 above the feet.
        assert!((current.y - (1.0 + physics::EYE_HEIGHT) as f32).abs() < 1e-5);
    }
}
