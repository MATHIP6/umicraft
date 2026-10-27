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
use crate::shapes::{is_normal_cube, Aabb, BlockSource};
use crate::view;
use umicraft_world::BlockState;

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

/// The default walking speed of the abilities
/// (MCP `PlayerCapabilities.walkSpeed`, line 26).
pub const DEFAULT_WALK_SPEED: f32 = 0.1;

/// The upwards motion a Jump Boost level adds (MCP `EntityLivingBase.java:1562`).
const JUMP_BOOST_PER_LEVEL: f64 = 0.1;

/// The food level above which the player may sprint
/// (MCP `EntityPlayerSP.java:799`: `foodLevel > 6`).
pub const SPRINT_FOOD_LIMIT: f32 = 6.0;

/// The default flying speed (MCP `PlayerCapabilities.flySpeed`, line 25).
pub const DEFAULT_FLY_SPEED: f32 = 0.05;

/// The ticks in which a second jump press toggles flight
/// (MCP `EntityPlayerSP.java:834`).
pub const FLY_TOGGLE_TICKS: u32 = 7;

/// The vertical flying acceleration per unit of flying speed
/// (MCP `EntityPlayerSP.java:852,857`).
const FLY_VERTICAL_FACTOR: f64 = 3.0;

/// The part of the vertical motion a flying player keeps each tick
/// (MCP `EntityPlayer.java:1800`).
const FLY_VERTICAL_KEEP: f64 = 0.6;

/// The upwards motion of a jump (MCP `EntityLivingBase.java:1550`).
pub const JUMP_MOTION: f64 = 0.42;

/// The forward boost of a sprinting jump (MCP `EntityLivingBase.java:1568`).
pub const SPRINT_JUMP_BOOST: f64 = 0.2;

/// The friction applied to the horizontal motion in the air; on the ground it
/// is multiplied by the slipperiness of the block below
/// (MCP `EntityLivingBase.java:1599-1604`).
pub const GROUND_FRICTION: f64 = 0.91;

/// The slipperiness of an ordinary block (MCP `Block.java:289`).
pub const DEFAULT_SLIPPERINESS: f64 = 0.6;

/// The horizontal motion the push out of a block sets
/// (MCP `EntityPlayerSP.java:483`).
pub const PUSH_OUT_SPEED: f64 = 0.1;

/// The slime block id, the one block that bounces a landing entity.
const SLIME_BLOCK: u16 = 165;

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
///
/// The value is the **client** player's (0.6). The server player sets its
/// step height to 0 (MCP `EntityPlayerMP.java:193`), which looks like a
/// disagreement but is not one:
///
/// * the client reports the position it *reached*, vertical rise included;
/// * the server resolves the axes in the order Y, X, Z
///   (MCP `Entity.java:691`, `:699`, `:706`), so the rise is applied first
///   and the horizontal move is no longer blocked by the ledge.
///
/// The two therefore end up at the same place, and the client must keep the
/// 0.6 to be able to walk up a slab at all.
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
    /// The movement speed attribute without the sprint boost
    /// (MCP `generic.movementSpeed`): the walk speed plus the effects and
    /// modifiers of the server. See [`crate::movement`].
    pub movement_speed: f64,
    /// The walking speed of the abilities (MCP `PlayerCapabilities.walkSpeed`),
    /// the reference of the FOV modifier.
    pub walk_speed: f32,
    /// The level of the Jump Boost effect, `0` without it
    /// (`amplifier + 1`).
    pub jump_boost: u8,
    /// Whether the food level lets the player sprint (above 6 points).
    pub well_fed: bool,
    /// Whether the Blindness effect stops a sprint from starting.
    pub blinded: bool,
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
    /// Whether the player may toggle flight
    /// (MCP `PlayerCapabilities.allowFlying`).
    pub allow_flying: bool,
    /// Whether the player is flying (MCP `PlayerCapabilities.isFlying`).
    pub flying: bool,
    /// The flying speed (MCP `PlayerCapabilities.flySpeed`).
    pub fly_speed: f32,
    /// Whether the player moves through blocks (MCP `Entity.noClip`).
    pub no_clip: bool,
    /// Whether the jump key was held on the previous tick, for the
    /// double-tap flight toggle.
    pub previous_jump: bool,
    /// The ticks the double-tap flight window still lasts
    /// (MCP `EntityPlayerSP.flyToggleTimer`, line 112).
    pub fly_toggle_timer: u32,
    /// The distance walked, scaled for the view bobbing
    /// (MCP `Entity.distanceWalkedModified`: 0.6 per block).
    pub walk_distance: f32,
    /// [`Player::walk_distance`] of the previous tick.
    pub prev_walk_distance: f32,
    /// The smoothed horizontal speed that drives the sway
    /// (MCP `EntityPlayer.cameraYaw`).
    pub camera_sway: f32,
    /// [`Player::camera_sway`] of the previous tick.
    pub prev_camera_sway: f32,
    /// The smoothed vertical-speed tilt in degrees
    /// (MCP `EntityPlayer.cameraPitch`).
    pub camera_tilt: f32,
    /// [`Player::camera_tilt`] of the previous tick.
    pub prev_camera_tilt: f32,
    /// The smoothed field-of-view modifier
    /// (MCP `EntityRenderer.fovModifierHand`).
    pub fov_modifier: f32,
    /// [`Player::fov_modifier`] of the previous tick.
    pub prev_fov_modifier: f32,
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
            movement_speed: WALK_SPEED,
            walk_speed: DEFAULT_WALK_SPEED,
            jump_boost: 0,
            well_fed: true,
            blinded: false,
            speed_multiplier: 1.0,
            previous_forward: 0.0,
            collided_horizontally: false,
            allow_flying: false,
            flying: false,
            fly_speed: DEFAULT_FLY_SPEED,
            no_clip: false,
            previous_jump: false,
            fly_toggle_timer: 0,
            walk_distance: 0.0,
            prev_walk_distance: 0.0,
            camera_sway: 0.0,
            prev_camera_sway: 0.0,
            camera_tilt: 0.0,
            prev_camera_tilt: 0.0,
            fov_modifier: 1.0,
            prev_fov_modifier: 1.0,
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
    player.prev_walk_distance = player.walk_distance;
    player.prev_camera_sway = player.camera_sway;
    player.prev_camera_tilt = player.camera_tilt;

    update_sprint(player, intent);

    update_flight(player, intent);

    // MCP `EntityPlayerSP.java:795-798`, before the living update; the
    // override of `pushOutOfBlocks` returns early for a no-clip player.
    if !player.no_clip {
        push_out_of_blocks(player, world);
    }

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
            player.velocity.y = JUMP_MOTION + f64::from(player.jump_boost) * JUMP_BOOST_PER_LEVEL;
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

    // MCP `EntityPlayerSP.java:904-907`: touching the ground ends a flight,
    // except for a spectator.
    if player.on_ground && player.flying && !player.no_clip {
        player.flying = false;
    }

    // The movement speed attribute is refreshed after the movement,
    // MCP `EntityPlayer.java:627-634`.
    player.speed_multiplier = if player.sprinting {
        1.0 + SPRINT_SPEED_BONUS
    } else {
        1.0
    };
    update_view_effects(player);
}

/// Refreshes the camera sway and the FOV modifier after the movement.
///
/// MCP `EntityPlayer.onLivingUpdate`, lines 636-654: the sway follows the
/// horizontal speed (capped, zero in the air) and the tilt follows the
/// vertical speed (zero on the ground). MCP `EntityRenderer`
/// `updateFovModifierHand` smooths the FOV modifier of the player.
fn update_view_effects(player: &mut Player) {
    let speed = player.velocity.x.hypot(player.velocity.z) as f32;
    let sway = if player.on_ground {
        speed.min(view::MAX_SWAY_SPEED)
    } else {
        0.0
    };
    let tilt = if player.on_ground {
        0.0
    } else {
        ((-player.velocity.y * 0.2).atan() * view::TILT_DEGREES) as f32
    };
    player.camera_sway += (sway - player.camera_sway) * view::SWAY_SMOOTHING;
    player.camera_tilt += (tilt - player.camera_tilt) * view::TILT_SMOOTHING;

    player.prev_fov_modifier = player.fov_modifier;
    let target = view::target_fov_modifier(
        player.flying,
        player.movement_speed * player.speed_multiplier / f64::from(player.walk_speed),
    );
    player.fov_modifier = view::smooth_fov_modifier(player.fov_modifier, target);
}

/// The slipperiness of a block (MCP `Block.slipperiness`): ice and packed ice
/// 0.98 (`BlockIce.java:23`, `BlockPackedIce.java:12`), slime 0.8
/// (`BlockSlime.java:17`), every other block 0.6 (`Block.java:289`).
fn slipperiness(state: BlockState) -> f64 {
    match state.id() {
        79 | 174 => 0.98,
        SLIME_BLOCK => 0.8,
        _ => DEFAULT_SLIPPERINESS,
    }
}

/// The block the friction is read from: the one under the feet, at
/// `(floor(x), floor(minY) - 1, floor(z))` (MCP `EntityLivingBase.java:1603`).
fn block_below_feet<S: BlockSource + ?Sized>(player: &Player, world: &S) -> BlockState {
    world.block(
        player.position.x.floor() as i32,
        player.position.y.floor() as i32 - 1,
        player.position.z.floor() as i32,
    )
}

/// The vertical collision of a move: MCP `Entity.java:836-839` calls
/// `onLanded` on the block at `floor(y - 0.2)`. Every block clears the
/// vertical motion (`Block.java:1111`), except a slime block, which reverses
/// a downward motion unless the player sneaks (`BlockSlime.java:44-54`).
fn land<S: BlockSource + ?Sized>(player: &mut Player, world: &S) {
    let landed_on = world.block(
        player.position.x.floor() as i32,
        (player.position.y - 0.2).floor() as i32,
        player.position.z.floor() as i32,
    );
    if landed_on.id() == SLIME_BLOCK && !player.sneaking {
        if player.velocity.y < 0.0 {
            player.velocity.y = -player.velocity.y;
        }
    } else {
        player.velocity.y = 0.0;
    }
}

/// Pushes a player whose corners are inside a normal cube sideways.
///
/// MCP `EntityPlayerSP.java:795-798` tests the four corners of the box, shrunk
/// to 35 % of the width, half a block above the feet. The vanilla client
/// never lifts the player out of a block.
fn push_out_of_blocks<S: BlockSource + ?Sized>(player: &mut Player, world: &S) {
    let inset = PLAYER_WIDTH * 0.35;
    let y = player.position.y + 0.5;
    let (x, z) = (player.position.x, player.position.z);
    for (dx, dz) in [
        (-inset, inset),
        (-inset, -inset),
        (inset, -inset),
        (inset, inset),
    ] {
        push_out_of_block(player, world, x + dx, y, z + dz);
    }
}

/// One corner of [`push_out_of_blocks`], MCP `EntityPlayerSP.java:437-505`:
/// the nearest open side among west, east, north and south, in that order of
/// preference on ties, sets the horizontal motion towards it.
fn push_out_of_block<S: BlockSource + ?Sized>(
    player: &mut Player,
    world: &S,
    x: f64,
    y: f64,
    z: f64,
) {
    let (block_x, block_y, block_z) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let open = |bx: i32, bz: i32| {
        !is_normal_cube(world.block(bx, block_y, bz))
            && !is_normal_cube(world.block(bx, block_y + 1, bz))
    };
    if open(block_x, block_z) {
        return;
    }
    let fraction_x = x - f64::from(block_x);
    let fraction_z = z - f64::from(block_z);
    let candidates = [
        (
            open(block_x - 1, block_z),
            fraction_x,
            DVec3::new(-PUSH_OUT_SPEED, 0.0, 0.0),
        ),
        (
            open(block_x + 1, block_z),
            1.0 - fraction_x,
            DVec3::new(PUSH_OUT_SPEED, 0.0, 0.0),
        ),
        (
            open(block_x, block_z - 1),
            fraction_z,
            DVec3::new(0.0, 0.0, -PUSH_OUT_SPEED),
        ),
        (
            open(block_x, block_z + 1),
            1.0 - fraction_z,
            DVec3::new(0.0, 0.0, PUSH_OUT_SPEED),
        ),
    ];
    let mut nearest: Option<(f64, DVec3)> = None;
    for (is_open, distance, push) in candidates {
        if is_open && nearest.map_or(distance < 9999.0, |(best, _)| distance < best) {
            nearest = Some((distance, push));
        }
    }
    if let Some((_, push)) = nearest {
        if push.x != 0.0 {
            player.velocity.x = push.x;
        } else {
            player.velocity.z = push.z;
        }
    }
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

    // MCP `EntityPlayerSP.java:799`: sprinting needs food above 6 points
    // (or the ability to fly); Blindness only stops a sprint from starting
    // (lines 801 and 811).
    let may_sprint = player.well_fed || player.allow_flying;
    let may_start = may_sprint && !player.blinded;

    // The double tap: the forward input crossed the threshold while the
    // player is on the ground and does not sneak.
    if may_start
        && player.on_ground
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
    if may_start && !player.sprinting && above_threshold && intent.sprint {
        player.set_sprinting(true);
    }

    // The sprint ends when the player slows down or hits a wall.
    if player.sprinting && (!above_threshold || player.collided_horizontally || !may_sprint) {
        player.set_sprinting(false);
    }

    player.previous_forward = intent.forward;
}

/// The flight toggle and the vertical flying input.
///
/// MCP `EntityPlayerSP.onLivingUpdate`, lines 823-861: a spectator always
/// flies; otherwise the second press of the jump key within
/// [`FLY_TOGGLE_TICKS`] ticks toggles the flight. While flying, jump and
/// sneak accelerate upwards and downwards.
fn update_flight(player: &mut Player, intent: &InputIntent) {
    if player.fly_toggle_timer > 0 {
        player.fly_toggle_timer -= 1;
    }
    if player.allow_flying {
        if player.no_clip {
            player.flying = true;
        } else if !player.previous_jump && intent.jump {
            if player.fly_toggle_timer == 0 {
                player.fly_toggle_timer = FLY_TOGGLE_TICKS;
            } else {
                player.flying = !player.flying;
                player.fly_toggle_timer = 0;
            }
        }
    }
    player.previous_jump = intent.jump;

    if player.flying {
        let lift = f64::from(player.fly_speed) * FLY_VERTICAL_FACTOR;
        if intent.sneak {
            player.velocity.y -= lift;
        }
        if intent.jump {
            player.velocity.y += lift;
        }
    }
}

/// The horizontal and vertical integration of one tick.
///
/// MCP `net/minecraft/entity/EntityLivingBase.java:1591-1672`.
fn travel<S: BlockSource + ?Sized>(player: &mut Player, strafe: f64, forward: f64, world: &S) {
    // MCP `EntityLivingBase.java:1599-1604`: the block below the feet sets
    // the friction on the ground; it is read before the move, so the same
    // value drives the acceleration and the damping.
    let friction = if player.on_ground {
        slipperiness(block_below_feet(player, world)) * GROUND_FRICTION
    } else {
        GROUND_FRICTION
    };
    let flying = player.flying;
    let kept_vertical = player.velocity.y;
    let speed = if flying && !player.on_ground {
        // MCP `EntityPlayer.java:1798`: the flying speed replaces the air
        // control and doubles while sprinting.
        f64::from(player.fly_speed) * if player.sprinting { 2.0 } else { 1.0 }
    } else if player.on_ground {
        // MCP `EntityLivingBase.java:1606-1612`.
        player.movement_speed
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
    if player.no_clip {
        // MCP `Entity.java:595-600`: no collision at all.
        player.position += player.velocity;
        player.on_ground = false;
        player.collided_horizontally = false;
        finish_travel(player, friction, flying, kept_vertical);
        return;
    }
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
    if !player.flying {
        // MCP `Entity.moveEntity` line 862, skipped while flying
        // (`EntityPlayer.canTriggerWalking`).
        player.walk_distance += result.delta.x.hypot(result.delta.z) as f32 * 0.6;
    }
    player.collided_horizontally = result.blocked_horizontally;
    player.on_ground = result.on_ground;
    if result.blocked_x {
        player.velocity.x = 0.0;
    }
    if result.blocked_z {
        player.velocity.z = 0.0;
    }
    if result.blocked_vertically {
        land(player, world);
    }

    finish_travel(player, friction, flying, kept_vertical);
}

/// Gravity and friction, MCP `EntityLivingBase.java:1666-1671`; a flying
/// player keeps 60 % of the vertical motion it had before the move instead
/// (MCP `EntityPlayer.java:1800`).
fn finish_travel(player: &mut Player, friction: f64, flying: bool, kept_vertical: f64) {
    player.velocity.y -= GRAVITY;
    player.velocity.y *= VERTICAL_DAMPING;
    player.velocity.x *= friction;
    player.velocity.z *= friction;
    if flying {
        player.velocity.y = kept_vertical * FLY_VERTICAL_KEEP;
    }
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

    /// The friction on an ordinary block: `0.6 * 0.91`
    /// (MCP `EntityLivingBase.java:1603`).
    fn stone_friction() -> f64 {
        DEFAULT_SLIPPERINESS * GROUND_FRICTION
    }

    /// The acceleration the player gains per tick while walking.
    ///
    /// MCP `EntityLivingBase.java:1606` gives `0.16277136 / f^3`, the
    /// movement speed attribute is `0.1` and the input is damped by `0.98`
    /// (line 2020).
    fn walk_acceleration() -> f64 {
        INPUT_DAMPING * WALK_SPEED * (MOVE_SPEED_NUMERATOR / stone_friction().powi(3))
    }

    /// The speed a walking player converges to.
    ///
    /// The motion is damped *after* the acceleration was added to it
    /// (MCP `EntityLivingBase.java:1618-1671`), so the steady state is
    /// `acceleration * f / (1 - f)`.
    fn walk_speed() -> f64 {
        walk_acceleration() * stone_friction() / (1.0 - stone_friction())
    }

    #[test]
    fn a_walking_player_reaches_the_expected_speed() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        // The player walks towards +Z (yaw 0 is south in Minecraft). The speed
        // converges with the ground friction, so the test waits for it.
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

    /// A stone floor at y = 10 for z < 3 and at y = 9 for z >= 3: a one
    /// block ledge in front of a player walking towards +Z.
    fn ledge() -> TestBlocks {
        let mut blocks = Vec::new();
        for x in -4..4 {
            for z in -4..32 {
                let top = if z < 3 { 10 } else { 9 };
                blocks.push((x, top, z, BlockState::new(1, 0)));
            }
        }
        TestBlocks::with(&blocks)
    }

    /// Regression: the vertical motion piled up while the player walked, so
    /// walking off a ledge dropped the whole block in one tick.
    #[test]
    fn walking_off_a_ledge_falls_with_gravity() {
        let world = ledge();
        let mut player = Player::new(DVec3::new(0.5, 11.0, 0.5));
        let intent = InputIntent::walking(0.0);
        let mut previous = player.position.y;
        let mut falling_ticks = 0;
        for _ in 0..60 {
            step(&mut player, &intent, &world);
            let drop = previous - player.position.y;
            assert!(drop < 0.5, "the player dropped {drop} blocks in one tick");
            if drop > 0.0 {
                falling_ticks += 1;
            }
            previous = player.position.y;
        }
        assert!(
            (player.position.y - 10.0).abs() < 1e-9,
            "landed below the ledge"
        );
        assert!(falling_ticks >= 4, "the fall took {falling_ticks} ticks");
    }

    /// MCP `Block.onLanded` (`Block.java:1111`) clears the vertical motion on
    /// every vertical collision.
    #[test]
    fn the_vertical_motion_does_not_pile_up_on_the_ground() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        for _ in 0..40 {
            step(&mut player, &InputIntent::still(0.0), &world);
        }
        assert!(player.on_ground);
        // Cleared on landing, then one tick of gravity: (0 - 0.08) * 0.98.
        assert!(
            (player.velocity.y + 0.0784).abs() < 1e-9,
            "{}",
            player.velocity.y
        );
    }

    /// The distance a walking player covers per tick on a plain block:
    /// `a / (1 - f)` with `f = 0.6 * 0.91` and
    /// `a = 0.98 * 0.1 * 0.16277136 / f^3` (MCP `EntityLivingBase.java:1599-1671`).
    #[test]
    fn the_walking_distance_per_tick_matches_vanilla() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.0, 1.0, 0.0));
        let intent = InputIntent::walking(0.0);
        for _ in 0..100 {
            step(&mut player, &intent, &world);
        }
        let before = player.position.z;
        step(&mut player, &intent, &world);
        let per_tick = player.position.z - before;
        let friction = 0.6 * GROUND_FRICTION;
        let acceleration = 0.98 * WALK_SPEED * MOVE_SPEED_NUMERATOR / friction.powi(3);
        let expected = acceleration / (1.0 - friction);
        assert!(
            (per_tick - expected).abs() < 1e-6,
            "expected {expected}, got {per_tick}"
        );
    }

    /// The distance a player slides after releasing the key, on `floor`.
    fn slide(floor: BlockState) -> f64 {
        let mut blocks = Vec::new();
        for x in -4..4 {
            for z in -4..64 {
                blocks.push((x, 0, z, floor));
            }
        }
        let world = TestBlocks::with(&blocks);
        let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
        for _ in 0..40 {
            step(&mut player, &InputIntent::walking(0.0), &world);
        }
        let released = player.position.z;
        for _ in 0..40 {
            step(&mut player, &InputIntent::still(0.0), &world);
        }
        player.position.z - released
    }

    /// Ice (0.98) is more slippery than a plain block (0.6), MCP
    /// `BlockIce.java:23`.
    #[test]
    fn ice_slides_further_than_stone() {
        let stone = slide(BlockState::new(1, 0));
        let ice = slide(BlockState::new(79, 0));
        assert!(ice > stone * 3.0, "ice {ice}, stone {stone}");
    }

    /// MCP `BlockSlime.onLanded` (`BlockSlime.java:44`): a falling player
    /// bounces back, unless it sneaks.
    #[test]
    fn a_slime_block_bounces_a_player_that_does_not_sneak() {
        let world = TestBlocks::with(&[(0, 0, 0, BlockState::new(165, 0))]);
        let mut player = Player::new(DVec3::new(0.5, 3.0, 0.5));
        let mut bounced = false;
        for _ in 0..30 {
            step(&mut player, &InputIntent::still(0.0), &world);
            bounced |= player.velocity.y > 0.0;
        }
        assert!(bounced, "the slime block did not bounce");

        let mut sneaking = Player::new(DVec3::new(0.5, 3.0, 0.5));
        let mut intent = InputIntent::still(0.0);
        intent.sneak = true;
        for _ in 0..30 {
            step(&mut sneaking, &intent, &world);
            assert!(sneaking.velocity.y <= 0.0, "a sneaking player bounced");
        }
    }

    /// MCP `EntityPlayerSP.pushOutOfBlocks` (`EntityPlayerSP.java:437`): a
    /// player whose corner is inside a normal cube is pushed sideways towards
    /// the nearest open side, never lifted.
    #[test]
    fn a_player_inside_a_full_block_is_pushed_sideways() {
        let mut blocks = Vec::new();
        for x in -4..4 {
            for z in -4..4 {
                blocks.push((x, 0, z, BlockState::new(1, 0)));
            }
        }
        // A two-block stone pillar that the player's west corners overlap.
        blocks.push((0, 1, 0, BlockState::new(1, 0)));
        blocks.push((0, 2, 0, BlockState::new(1, 0)));
        let world = TestBlocks::with(&blocks);
        let mut player = Player::new(DVec3::new(0.25, 1.0, 0.5));
        player.on_ground = true;
        step(&mut player, &InputIntent::still(0.0), &world);
        assert!(
            player.position.x < 0.25,
            "pushed west, got {}",
            player.position.x
        );
        assert!((player.position.y - 1.0).abs() < 1e-9, "never lifted");
    }

    /// Plants have no collision box and are not normal cubes: standing in
    /// tall grass neither lifts nor pushes the player.
    #[test]
    fn a_player_in_tall_grass_stays_put() {
        let mut blocks = Vec::new();
        for x in -4..4 {
            for z in -4..4 {
                blocks.push((x, 0, z, BlockState::new(2, 0)));
            }
        }
        blocks.push((0, 1, 0, BlockState::new(31, 1)));
        let world = TestBlocks::with(&blocks);
        let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
        player.on_ground = true;
        for _ in 0..5 {
            step(&mut player, &InputIntent::still(0.0), &world);
        }
        assert_eq!(player.position, DVec3::new(0.5, 1.0, 0.5));
    }

    fn flying_player() -> Player {
        let mut player = Player::new(DVec3::new(0.5, 10.0, 0.5));
        player.allow_flying = true;
        player
    }

    fn jump_intent(jump: bool) -> InputIntent {
        InputIntent {
            jump,
            ..InputIntent::default()
        }
    }

    #[test]
    fn a_double_tap_of_jump_toggles_the_flight() {
        let world = TestBlocks::default();
        let mut player = flying_player();
        for jump in [true, false, true] {
            step(&mut player, &jump_intent(jump), &world);
        }
        assert!(player.flying);
        for jump in [false, true] {
            step(&mut player, &jump_intent(jump), &world);
            step(&mut player, &jump_intent(false), &world);
        }
        assert!(player.flying, "one press alone does not toggle");
    }

    #[test]
    fn a_player_who_may_not_fly_ignores_the_double_tap() {
        let world = TestBlocks::default();
        let mut player = Player::new(DVec3::new(0.5, 10.0, 0.5));
        for jump in [true, false, true] {
            step(&mut player, &jump_intent(jump), &world);
        }
        assert!(!player.flying);
    }

    #[test]
    fn a_flying_player_rises_while_jump_is_held_and_hovers_after() {
        let world = TestBlocks::default();
        let mut player = flying_player();
        player.flying = true;
        for _ in 0..10 {
            step(&mut player, &jump_intent(true), &world);
        }
        assert!(player.position.y > 10.5, "y = {}", player.position.y);
        // The lift dies out (60 % per tick) instead of turning into a fall.
        for _ in 0..30 {
            step(&mut player, &jump_intent(false), &world);
        }
        let height = player.position.y;
        for _ in 0..30 {
            step(&mut player, &jump_intent(false), &world);
        }
        assert!(
            (player.position.y - height).abs() < 0.01,
            "the flight must not fall like a jump"
        );
    }

    #[test]
    fn a_flying_player_that_lands_stops_flying() {
        let world = flat_world();
        let mut player = flying_player();
        player.position = DVec3::new(0.5, 1.0, 0.5);
        player.flying = true;
        let sneak = InputIntent {
            sneak: true,
            ..InputIntent::default()
        };
        for _ in 0..20 {
            step(&mut player, &sneak, &world);
        }
        assert!(player.on_ground);
        assert!(!player.flying);
    }

    #[test]
    fn a_no_clip_player_flies_through_blocks_and_stays_flying() {
        let world = flat_world();
        let mut player = flying_player();
        player.no_clip = true;
        player.position = DVec3::new(0.5, 3.0, 0.5);
        let sneak = InputIntent {
            sneak: true,
            ..InputIntent::default()
        };
        for _ in 0..40 {
            step(&mut player, &sneak, &world);
        }
        assert!(player.position.y < 0.0, "y = {}", player.position.y);
        assert!(player.flying, "a spectator keeps flying");
    }

    #[test]
    fn walking_accumulates_the_bobbing_distance_and_sway() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
        for _ in 0..20 {
            step(&mut player, &InputIntent::walking(0.0), &world);
        }
        // About 0.22 blocks per tick at the end, 0.6 per block.
        assert!(player.walk_distance > 1.0, "{}", player.walk_distance);
        assert!(player.camera_sway > 0.05, "{}", player.camera_sway);
        assert!(player.prev_walk_distance < player.walk_distance);
    }

    #[test]
    fn a_standing_player_does_not_bob() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
        for _ in 0..20 {
            step(&mut player, &InputIntent::still(0.0), &world);
        }
        assert_eq!(player.walk_distance, 0.0);
        assert!(player.camera_sway < 1e-3);
    }

    #[test]
    fn a_flight_does_not_count_as_walking() {
        let world = TestBlocks::default();
        let mut player = flying_player();
        player.flying = true;
        for _ in 0..10 {
            step(&mut player, &InputIntent::walking(0.0), &world);
        }
        assert_eq!(player.walk_distance, 0.0);
        assert!(player.fov_modifier > 1.0, "flying widens the view");
    }

    #[test]
    fn sprinting_converges_to_the_wider_field_of_view() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
        let sprint = InputIntent {
            sprint: true,
            ..InputIntent::walking(0.0)
        };
        for _ in 0..30 {
            step(&mut player, &sprint, &world);
        }
        assert!(
            (player.fov_modifier - 1.15).abs() < 0.01,
            "{}",
            player.fov_modifier
        );
    }

    fn sprint_intent() -> InputIntent {
        InputIntent {
            sprint: true,
            ..InputIntent::walking(0.0)
        }
    }

    #[test]
    fn a_hungry_player_cannot_sprint_and_loses_a_running_sprint() {
        let world = flat_world();
        let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
        for _ in 0..5 {
            step(&mut player, &sprint_intent(), &world);
        }
        assert!(player.sprinting);

        player.well_fed = false;
        step(&mut player, &sprint_intent(), &world);
        assert!(!player.sprinting, "food at 6 or less ends the sprint");
        for _ in 0..5 {
            step(&mut player, &sprint_intent(), &world);
        }
        assert!(!player.sprinting, "and it does not start again");

        // The ability to fly lifts the hunger rule (MCP `|| allowFlying`).
        player.allow_flying = true;
        step(&mut player, &sprint_intent(), &world);
        assert!(player.sprinting);
    }

    #[test]
    fn blindness_stops_a_sprint_from_starting_but_not_a_running_one() {
        let world = flat_world();
        let mut blind = Player::new(DVec3::new(0.5, 1.0, 0.5));
        blind.blinded = true;
        for _ in 0..5 {
            step(&mut blind, &sprint_intent(), &world);
        }
        assert!(!blind.sprinting);

        let mut runner = Player::new(DVec3::new(0.5, 1.0, 0.5));
        for _ in 0..5 {
            step(&mut runner, &sprint_intent(), &world);
        }
        runner.blinded = true;
        step(&mut runner, &sprint_intent(), &world);
        assert!(runner.sprinting);
    }

    #[test]
    fn jump_boost_adds_a_tenth_per_level_to_the_jump() {
        let world = flat_world();
        let jump = |level: u8| {
            let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
            for _ in 0..3 {
                step(&mut player, &InputIntent::still(0.0), &world);
            }
            player.jump_boost = level;
            let before = player.position.y;
            step(&mut player, &jump_intent(true), &world);
            player.position.y - before
        };
        assert!(jump(2) > jump(0) + 0.15, "{} vs {}", jump(2), jump(0));
    }

    #[test]
    fn the_movement_speed_attribute_scales_the_walk() {
        let world = flat_world();
        let distance = |speed: f64| {
            let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5));
            player.movement_speed = speed;
            for _ in 0..20 {
                step(&mut player, &InputIntent::walking(0.0), &world);
            }
            player.position.z - 0.5
        };
        let ratio = distance(0.14) / distance(0.1);
        assert!((ratio - 1.4).abs() < 0.02, "{ratio}");
    }
}
