// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 movement rules.
//!
//! Every constant of this module is taken from the MCP reference and cited
//! in `docs/sources.md`. The step itself follows MCP
//! `net/minecraft/entity/EntityLivingBase.java`:
//!
//! 1. the sprint state machine of `EntityPlayerSP.onLivingUpdate` (line 782),
//! 2. the drop of tiny motions (line 1963),
//! 3. the jump (line 1997),
//! 4. the travel step (line 2020) that calls
//!    `moveEntityWithHeading` (line 1591) and `moveEntity` (line 1646).
//!
//! The step is deterministic: it reads no clock and uses no randomness, so
//! the same [`InputIntent`] sequence against the same world always produces
//! the same trajectory.

use glam::DVec3;

use crate::collision::{self, MoveOptions};
use crate::input::{InputIntent, SPRINT_FORWARD_THRESHOLD};
use crate::shapes::{Aabb, BlockSource};

/// The width of the player bounding box (MCP `Entity.java:259`).
pub const PLAYER_WIDTH: f64 = 0.6;

/// The height of the player bounding box (MCP `Entity.java:260`).
pub const PLAYER_HEIGHT: f64 = 1.8;

/// Half the width of the player bounding box.
pub const PLAYER_HALF_WIDTH: f64 = PLAYER_WIDTH / 2.0;

/// The height of the player's eyes (MCP `EntityPlayer.java:2328`).
pub const EYE_HEIGHT: f64 = 1.62;

/// How much lower the eyes are while sneaking (MCP `EntityPlayer.java:2337`).
pub const SNEAKING_EYE_HEIGHT_OFFSET: f64 = 0.08;

/// The base value of the movement speed attribute of a player
/// (MCP `EntityPlayer.java:193` and `PlayerCapabilities.java:24`).
pub const WALK_SPEED: f64 = 0.1;

/// The sprinting bonus of the movement speed attribute
/// (MCP `EntityLivingBase.java:57`: `0.3` with operation `2`, multiply total).
pub const SPRINT_SPEED_BONUS: f64 = 0.3;

/// The upwards motion of a jump (MCP `EntityLivingBase.java:1550`).
pub const JUMP_MOTION: f64 = 0.42;

/// The forward boost of a sprinting jump (MCP `EntityLivingBase.java:1568`).
pub const SPRINT_JUMP_BOOST: f64 = 0.2;

/// The friction applied to the horizontal motion on the ground
/// (MCP `EntityLivingBase.java:1599`).
pub const GROUND_FRICTION: f64 = 0.91;

/// The numerator of the movement speed factor (MCP `EntityLivingBase.java:1606`).
pub const MOVE_SPEED_NUMERATOR: f64 = 0.16277136;

/// The damping applied to the latched input before it is used
/// (MCP `EntityLivingBase.java:2020-2021`).
pub const INPUT_DAMPING: f64 = 0.98;

/// The acceleration factor of a player in the air
/// (MCP `EntityLivingBase.java:112` and `EntityPlayer.java:164`).
pub const AIR_CONTROL: f64 = 0.02;

/// The gravity applied every tick (MCP `EntityLivingBase.java:1666`).
pub const GRAVITY: f64 = 0.08;

/// The damping applied to the vertical motion (MCP `EntityLivingBase.java:1669`).
pub const VERTICAL_DAMPING: f64 = 0.98;

/// The height of a ledge a player can step up (MCP `EntityLivingBase.java:208`).
pub const STEP_HEIGHT: f64 = 0.6;

/// Motions smaller than this are dropped (MCP `EntityLivingBase.java:1963`).
pub const MOTION_EPSILON: f64 = 0.005;

/// The number of ticks between two jumps (MCP `EntityLivingBase.java:2010`).
pub const JUMP_COOLDOWN_TICKS: u32 = 10;

/// The number of ticks the double-tap sprint window lasts
/// (MCP `EntityPlayerSP.java:805`).
pub const SPRINT_TOGGLE_TICKS: u32 = 7;

/// The number of ticks a sprint lasts after it was enabled
/// (MCP `EntityPlayerSP.java:519`).
pub const SPRINT_DURATION_TICKS: u32 = 600;

/// The kinematic state of the local player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    /// The feet position: the centre of the bounding box on X and Z and its
    /// bottom on Y (MCP `Entity.posX/posY/posZ`).
    pub position: DVec3,
    /// The motion vector (MCP `Entity.motionX/motionY/motionZ`).
    pub velocity: DVec3,
    /// The horizontal view angle in radians.
    pub yaw: f32,
    /// The vertical view angle in radians.
    pub pitch: f32,
    /// Whether the player rests on the ground.
    pub on_ground: bool,
    /// Whether the player is sprinting.
    pub sprinting: bool,
    /// Whether the player is sneaking.
    pub sneaking: bool,
    /// The ticks an enabled sprint still lasts
    /// (MCP `EntityPlayerSP.sprintingTicksLeft`, line 114).
    pub sprinting_ticks_left: u32,
    /// The ticks the double-tap sprint window still lasts
    /// (MCP `EntityPlayerSP.sprintToggleTimer`, line 111).
    pub sprint_toggle_timer: u32,
    /// The ticks before the player may jump again
    /// (MCP `EntityLivingBase.jumpTicks`, line 186).
    pub jump_ticks: u32,
    /// The sprint multiplier in effect for the current tick.
    ///
    /// The vanilla code refreshes the movement speed attribute *after* the
    /// movement (MCP `EntityPlayer.java:627-634`), so the sprint state of the
    /// previous tick decides the speed of the current one.
    pub speed_multiplier: f64,
    /// The forward input of the previous tick, used by the double-tap sprint.
    pub previous_forward: f32,
    /// Whether the last move was blocked horizontally.
    pub collided_horizontally: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            sprinting: false,
            sneaking: false,
            sprinting_ticks_left: 0,
            sprint_toggle_timer: 0,
            jump_ticks: 0,
            speed_multiplier: 1.0,
            previous_forward: 0.0,
            collided_horizontally: false,
        }
    }
}

impl Player {
    /// Creates a player at the given feet position.
    #[must_use]
    pub fn new(position: DVec3) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }

    /// The bounding box of the player.
    ///
    /// MCP `Entity.setPosition` (line 365) builds the box as
    /// `(x - width / 2, y, z - width / 2) .. (x + width / 2, y + height, z + width / 2)`.
    #[must_use]
    pub fn bounding_box(&self) -> Aabb {
        Aabb::new(
            DVec3::new(
                self.position.x - PLAYER_HALF_WIDTH,
                self.position.y,
                self.position.z - PLAYER_HALF_WIDTH,
            ),
            DVec3::new(
                self.position.x + PLAYER_HALF_WIDTH,
                self.position.y + PLAYER_HEIGHT,
                self.position.z + PLAYER_HALF_WIDTH,
            ),
        )
    }

    /// The position of the player's eyes.
    #[must_use]
    pub fn eye_position(&self) -> DVec3 {
        let height = if self.sneaking {
            EYE_HEIGHT - SNEAKING_EYE_HEIGHT_OFFSET
        } else {
            EYE_HEIGHT
        };
        self.position + DVec3::Y * height
    }

    /// Enables or disables the sprint.
    ///
    /// MCP `EntityPlayerSP.setSprinting` (line 516) restarts the sprint timer
    /// when the sprint is enabled and clears it when it is disabled.
    pub fn set_sprinting(&mut self, sprinting: bool) {
        self.sprinting = sprinting;
        self.sprinting_ticks_left = if sprinting { SPRINT_DURATION_TICKS } else { 0 };
    }
}

/// Advances the player by exactly one tick.
///
/// The function is pure with respect to the world: it only reads blocks, and
/// the same intent sequence always produces the same trajectory.
///
/// The step allocates nothing: the player state is a plain `Copy` struct and
/// the collision solver only ever holds a few boxes on the stack (see
/// [`crate::collision`]).
pub fn step<S: BlockSource + ?Sized>(player: &mut Player, intent: &InputIntent, world: &S) {
    player.yaw = intent.yaw;
    player.pitch = intent.pitch;
    player.sneaking = intent.sneak;

    update_sprint(player, intent);

    // Drop the tiny motions, MCP `EntityLivingBase.java:1963-1976`.
    if player.velocity.x.abs() < MOTION_EPSILON {
        player.velocity.x = 0.0;
    }
    if player.velocity.y.abs() < MOTION_EPSILON {
        player.velocity.y = 0.0;
    }
    if player.velocity.z.abs() < MOTION_EPSILON {
        player.velocity.z = 0.0;
    }

    // The jump, MCP `EntityLivingBase.java:1997-2016`.
    if player.jump_ticks > 0 {
        player.jump_ticks -= 1;
    }
    if intent.jump {
        if player.on_ground && player.jump_ticks == 0 {
            player.velocity.y = JUMP_MOTION;
            if player.sprinting {
                let (sin_yaw, cos_yaw) = player.yaw.sin_cos();
                player.velocity.x -= f64::from(sin_yaw) * SPRINT_JUMP_BOOST;
                player.velocity.z += f64::from(cos_yaw) * SPRINT_JUMP_BOOST;
            }
            player.jump_ticks = JUMP_COOLDOWN_TICKS;
        }
    } else {
        player.jump_ticks = 0;
    }

    // The travel step, MCP `EntityLivingBase.java:2020-2023`.
    let strafe = f64::from(intent.strafe) * INPUT_DAMPING;
    let forward = f64::from(intent.forward) * INPUT_DAMPING;
    travel(player, strafe, forward, world);

    // The movement speed attribute is refreshed after the movement,
    // MCP `EntityPlayer.java:627-634`.
    player.speed_multiplier = if player.sprinting {
        1.0 + SPRINT_SPEED_BONUS
    } else {
        1.0
    };
}

/// The sprint state machine.
///
/// MCP `net/minecraft/client/entity/EntityPlayerSP.java:782-821`. The forward
/// threshold is compared against the input of the *previous* tick, because
/// the vanilla code reads the flag before it refreshes the input
/// (line 785 before line 786); that is what makes the double tap work.
fn update_sprint(player: &mut Player, intent: &InputIntent) {
    if player.sprinting_ticks_left > 0 {
        player.sprinting_ticks_left -= 1;
        if player.sprinting_ticks_left == 0 {
            player.set_sprinting(false);
        }
    }
    if player.sprint_toggle_timer > 0 {
        player.sprint_toggle_timer -= 1;
    }

    let was_above_threshold = player.previous_forward >= SPRINT_FORWARD_THRESHOLD;
    let above_threshold = intent.forward >= SPRINT_FORWARD_THRESHOLD;

    // The double tap: the forward input crossed the threshold while the
    // player is on the ground and does not sneak.
    if player.on_ground
        && !intent.sneak
        && !was_above_threshold
        && above_threshold
        && !player.sprinting
    {
        if player.sprint_toggle_timer == 0 && !intent.sprint {
            player.sprint_toggle_timer = SPRINT_TOGGLE_TICKS;
        } else {
            player.set_sprinting(true);
        }
    }

    // The sprint key starts the sprint on its own.
    if !player.sprinting && above_threshold && intent.sprint {
        player.set_sprinting(true);
    }

    // The sprint ends when the player slows down or hits a wall.
    if player.sprinting && (!above_threshold || player.collided_horizontally) {
        player.set_sprinting(false);
    }

    player.previous_forward = intent.forward;
}

/// The horizontal and vertical integration of one tick.
///
/// MCP `net/minecraft/entity/EntityLivingBase.java:1591-1672`.
fn travel<S: BlockSource + ?Sized>(player: &mut Player, strafe: f64, forward: f64, world: &S) {
    let friction = GROUND_FRICTION;
    let speed = if player.on_ground {
        // MCP `EntityLivingBase.java:1606-1612`.
        WALK_SPEED
            * player.speed_multiplier
            * (MOVE_SPEED_NUMERATOR / (friction * friction * friction))
    } else {
        // MCP `EntityLivingBase.java:1613-1616`.
        AIR_CONTROL * player.speed_multiplier
    };

    // `moveFlying`, MCP `net/minecraft/entity/Entity.java:1214-1235`.
    let mut strafe = strafe;
    let mut forward = forward;
    let norm = strafe * strafe + forward * forward;
    if norm >= 1.0e-4 {
        let mut length = norm.sqrt();
        if length < 1.0 {
            length = 1.0;
        }
        let scale = speed / length;
        strafe *= scale;
        forward *= scale;
        let (sin_yaw, cos_yaw) = player.yaw.sin_cos();
        let (sin_yaw, cos_yaw) = (f64::from(sin_yaw), f64::from(cos_yaw));
        player.velocity.x += strafe * cos_yaw - forward * sin_yaw;
        player.velocity.z += forward * cos_yaw + strafe * sin_yaw;
    }

    // `moveEntity`, MCP `net/minecraft/entity/Entity.java:588-844`.
    let result = collision::move_entity(
        player.bounding_box(),
        player.velocity,
        world,
        MoveOptions {
            step_height: STEP_HEIGHT,
            sneaking: player.sneaking,
            on_ground: player.on_ground,
        },
    );
    player.position += result.delta;
    player.collided_horizontally = result.blocked_horizontally;
    player.on_ground = result.on_ground;
    if result.blocked_x {
        player.velocity.x = 0.0;
    }
    if result.blocked_z {
        player.velocity.z = 0.0;
    }

    // Gravity and friction, MCP `EntityLivingBase.java:1666-1671`.
    player.velocity.y -= GRAVITY;
    player.velocity.y *= VERTICAL_DAMPING;
    player.velocity.x *= friction;
    player.velocity.z *= friction;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{RawInput, SNEAK_INPUT_SCALE};
    use crate::shapes::BlockSource;
    use std::collections::HashMap;
    use umicraft_world::BlockState;

    /// A block source backed by a hash map, for tests.
    #[derive(Debug, Default)]
    struct TestBlocks {
        blocks: HashMap<(i32, i32, i32), BlockState>,
    }

    impl TestBlocks {
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

    fn flat_world() -> TestBlocks {
        // A stone floor at y = 0.
        let mut blocks = Vec::new();
        for x in -64..64 {
            for z in -64..64 {
                blocks.push((x, 0, z, BlockState::new(1, 0)));
            }
        }
        TestBlocks::with(&blocks)
    }

    /// The acceleration the player gains per tick while walking.
    ///
    /// MCP `EntityLivingBase.java:1606` gives `0.16277136 / 0.91^3 = 0.216`,
    /// the movement speed attribute is `0.1` and the input is damped by
    /// `0.98` (line 2020).
    fn walk_acceleration() -> f64 {
        INPUT_DAMPING * WALK_SPEED * (MOVE_SPEED_NUMERATOR / GROUND_FRICTION.powi(3))
    }

    /// The speed a walking player converges to.
    ///
    /// The motion is damped *after* the acceleration was added to it
    /// (MCP `EntityLivingBase.java:1618-1671`), so the steady state is
    /// `acceleration * 0.91 / (1 - 0.91)`.
    fn walk_speed() -> f64 {
        walk_acceleration() * GROUND_FRICTION / (1.0 - GROUND_FRICTION)
    }

    #[test]
    fn a_walking_player_reaches_the_expected_speed() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        // The player walks towards +Z (yaw 0 is south in Minecraft). The speed
        // converges with the 0.91 friction, so the test waits for it.
        for _ in 0..200 {
            step(&mut player, &intent, &world);
        }
        let speed = player.velocity.z;
        let expected = walk_speed();
        assert!(
            (speed - expected).abs() < 1e-6,
            "expected {expected}, got {speed}"
        );
    }

    #[test]
    fn a_sprinting_player_is_thirty_per_cent_faster() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        let mut intent = InputIntent::walking(0.0);
        intent.sprint = true;
        for _ in 0..200 {
            step(&mut player, &intent, &world);
        }
        let speed = player.velocity.z;
        let expected = walk_speed() * (1.0 + SPRINT_SPEED_BONUS);
        assert!(
            (speed - expected).abs() < 1e-6,
            "expected {expected}, got {speed}"
        );
        assert!(player.sprinting);
    }

    #[test]
    fn a_sneaking_player_is_thirty_per_cent_slower() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        // The sneak scaling happens in the input layer, MCP
        // `MovementInputFromOptions.java:44-45`.
        let intent = RawInput {
            forward: 1.0,
            sneak: true,
            ..RawInput::default()
        }
        .to_intent();
        for _ in 0..200 {
            step(&mut player, &intent, &world);
        }
        let speed = player.velocity.z;
        let expected = walk_speed() * SNEAK_INPUT_SCALE as f64;
        assert!(
            (speed - expected).abs() < 1e-6,
            "expected {expected}, got {speed}"
        );
    }

    #[test]
    fn a_jump_reaches_the_expected_height() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        let mut intent = InputIntent::still(0.0);
        intent.jump = true;
        let mut highest = player.position.y;
        for _ in 0..20 {
            step(&mut player, &intent, &world);
            highest = highest.max(player.position.y);
        }
        // The jump rises 0.42 in the first tick and then decays by
        // `(v - 0.08) * 0.98` every tick (MCP `EntityLivingBase.java:1666`),
        // which sums up to 1.2491870944 blocks. The last rise of 0.00298 is
        // dropped because it is below the 0.005 motion epsilon
        // (MCP `EntityLivingBase.java:1968`).
        let expected = 1.249_187_094_4;
        assert!(
            (highest - 1.0 - expected).abs() < 1e-9,
            "expected {expected}, got {}",
            highest - 1.0
        );
    }

    #[test]
    fn gravity_pulls_the_player_down() {
        let world = TestBlocks::default();
        let mut player = Player::new(DVec3::new(0.0, 10.0, 0.0));
        step(&mut player, &InputIntent::still(0.0), &world);
        // The first tick: the player moves by the old velocity and then
        // gravity is applied (MCP `EntityLivingBase.java:1666`).
        assert_eq!(player.position.y, 10.0);
        assert!((player.velocity.y + 0.0784).abs() < 1e-9);
    }

    #[test]
    fn the_player_lands_on_the_ground() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 5.0, 0.0));
        for _ in 0..200 {
            step(&mut player, &InputIntent::still(0.0), &world);
        }
        assert!(player.on_ground);
        assert!((player.position.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_air_control_is_weaker_than_the_ground_speed() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 10.0, 0.0));
        let intent = InputIntent::walking(0.0);
        // The player is in the air, so the air control factor of 0.02 is used
        // (MCP `EntityLivingBase.java:1615`).
        step(&mut player, &intent, &world);
        let air = player.velocity.z;
        assert!(!player.on_ground);
        // 0.98 * 0.02 = 0.0196, damped by 0.91.
        assert!((air - 0.0196 * 0.91).abs() < 1e-9);

        // On the ground the acceleration is the movement speed attribute
        // instead (MCP `EntityLivingBase.java:1611`).
        let mut grounded = Player::new(DVec3::new(0.0, 1.0, 0.0));
        grounded.on_ground = true;
        step(&mut grounded, &intent, &world);
        assert!(grounded.velocity.z > air);
    }

    #[test]
    fn the_player_stands_on_a_slab() {
        // A stone floor at y = 0 and a stone slab on top of the block at z = 2,
        // so the slab's top is at y = 1.5.
        let world = TestBlocks::with(&[
            (0, 0, 0, BlockState::new(1, 0)),
            (0, 0, 1, BlockState::new(1, 0)),
            (0, 0, 2, BlockState::new(1, 0)),
            (0, 1, 2, BlockState::new(44, 0)),
        ]);
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        for _ in 0..40 {
            step(&mut player, &intent, &world);
            if player.position.z > 2.2 {
                break;
            }
        }
        assert!(
            (player.position.y - 1.5).abs() < 1e-9,
            "the player must stand on the slab, got {}",
            player.position.y
        );
    }

    #[test]
    fn a_double_tap_starts_the_sprint() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        // Let the player settle on the ground first.
        step(&mut player, &InputIntent::still(0.0), &world);
        step(&mut player, &InputIntent::still(0.0), &world);
        assert!(player.on_ground);

        // The first tap starts the toggle timer (MCP `EntityPlayerSP.java:805`).
        step(&mut player, &InputIntent::walking(0.0), &world);
        assert!(!player.sprinting);
        assert_eq!(player.sprint_toggle_timer, SPRINT_TOGGLE_TICKS);

        // Releasing the key and pressing it again inside the window starts the
        // sprint (MCP `EntityPlayerSP.java:801-811`).
        step(&mut player, &InputIntent::still(0.0), &world);
        step(&mut player, &InputIntent::walking(0.0), &world);
        assert!(player.sprinting);
        assert_eq!(player.sprinting_ticks_left, SPRINT_DURATION_TICKS);
    }
}
