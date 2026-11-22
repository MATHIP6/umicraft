// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of a boat (MCP 1.8.9 `EntityBoat`).
//!
//! The reference client moves a boat two ways. A boat the local player does
//! not ride ("empty" on the client, `isBoatEmpty`) glides to each server
//! position over eight ticks, then keeps drifting with the velocity the server
//! sent, slowing down each tick. That drift is [`BoatMotion::drift`].
//!
//! A boat the local player boarded is simulated by the client itself
//! ([`BoatMotion::drive`]): it floats, takes the rider's keys and collides with
//! the blocks, and the server positions within a block of it are ignored. That
//! is why a boat answers the keys at once in the reference client.

use glam::{DVec3, Vec3};
use umicraft_world::fluid::Liquid;

use crate::collision::{boat_box, move_entity, Aabb, MoveOptions};
use crate::entity::angle::wrap_degrees;
use crate::shapes::BlockSource;

/// Ticks a boat the local player does not ride takes to reach a reported
/// position: the 3 of the packet plus 5 (`setPositionAndRotation2`).
pub const EMPTY_BOAT_INTERPOLATION_TICKS: u8 = 3 + 5;

/// What a drifting boat keeps of its motion each tick
/// (`0.99F`, `0.95F` and `0.99F` widened to `double`).
const DRIFT_DAMPING: DVec3 = DVec3::new(
    0.990_000_009_536_743_2,
    0.949_999_988_079_071,
    0.990_000_009_536_743_2,
);

/// Slices of the boat's height tested for water.
const WATER_SLICES: u32 = 5;
/// The lift of a boat partly in water, scaled by how much of it is in.
const PARTIAL_BUOYANCY: f64 = 0.039_999_999_105_930_33;
/// The lift of a boat fully in water.
const FULL_BUOYANCY: f64 = 0.007_000_000_216_066_837;
/// How much of the rider's keys a tick turns into motion.
const PADDLE: f64 = 0.050_000_000_745_058_06;
/// The top horizontal speed, in blocks per tick.
const MAX_SPEED: f64 = 0.35;
/// The speed multiplier a boat starts with and falls back to.
const MIN_SPEED_MULTIPLIER: f64 = 0.07;
/// A turn of the boat is at most this many degrees a tick.
const MAX_TURN: f64 = 20.0;

/// The keys of the rider of a driven boat: `moveForward` and `moveStrafing`
/// (`-1` to `1`, left positive) and the rider's yaw in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiderInput {
    /// Forward key, `-1.0..=1.0`.
    pub forward: f32,
    /// Sideways key, `-1.0` (right) to `1.0` (left).
    pub strafe: f32,
    /// The rider's yaw (`rotationYaw`).
    pub yaw: f32,
}

/// Where one driven tick left the boat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Driven {
    /// The new feet position.
    pub position: DVec3,
    /// The new yaw; the pitch of a driven boat is always 0.
    pub yaw: f32,
    /// Whether it rests on a block.
    pub on_ground: bool,
}

/// The motion of a boat on the client (`motionX`, `motionY`, `motionZ`) and
/// whether the client simulates it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoatMotion {
    /// Blocks per tick.
    pub motion: DVec3,
    /// `speedMultiplier`: how much a key press adds, from 0.07 up to 0.35 as
    /// the boat speeds up.
    pub speed_multiplier: f64,
    /// `!isBoatEmpty`: the local player boarded it, so the client drives it.
    /// It stays set after the player leaves, until another entity boards it
    /// (`NetHandlerPlayClient.handleEntityAttach`).
    pub driven: bool,
    /// The local player rides it now (its `riddenByEntity`).
    pub boarded: bool,
}

impl Default for BoatMotion {
    fn default() -> Self {
        Self {
            motion: DVec3::ZERO,
            speed_multiplier: MIN_SPEED_MULTIPLIER,
            driven: false,
            boarded: false,
        }
    }
}

impl BoatMotion {
    /// Takes the velocity the server sent (`setVelocity`, and
    /// `setPositionAndRotation2` copies it again on every move).
    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.motion = velocity.as_dvec3();
    }

    /// One tick of a boat that reached its last reported position: it moves
    /// by its motion, which halves on the ground and then fades
    /// (`EntityBoat.onUpdate`, `isRemote && isBoatEmpty`, no interpolation
    /// step left). Returns the new position.
    #[must_use]
    pub fn drift(&mut self, position: DVec3, on_ground: bool) -> DVec3 {
        let moved = position + self.motion;
        if on_ground {
            self.motion *= 0.5;
        }
        self.motion *= DRIFT_DAMPING;
        moved
    }
}

impl BoatMotion {
    /// One tick of a boat the client simulates (`EntityBoat.onUpdate`, the
    /// branch that is not `isRemote && isBoatEmpty`), from its feet
    /// `position`, `yaw` and `on_ground`. `rider` is `None` once the local
    /// player left it.
    ///
    /// Left out: the splash particles, the snow and lily pads the boat
    /// breaks (the server breaks them too and sends the change), the bounce
    /// on slime and beds, and cobwebs. The turn uses the exact `atan2`
    /// where the reference uses its table (`MathHelper.func_181159_b`).
    pub fn drive<S: BlockSource + ?Sized>(
        &mut self,
        position: DVec3,
        yaw: f32,
        on_ground: bool,
        rider: Option<RiderInput>,
        world: &S,
    ) -> Driven {
        let bounds = boat_box(position);
        let in_water = water_fraction(&bounds, world);
        let speed_before = self.motion.x.hypot(self.motion.z);
        if in_water < 1.0 {
            self.motion.y += PARTIAL_BUOYANCY * (in_water * 2.0 - 1.0);
        } else {
            if self.motion.y < 0.0 {
                self.motion.y /= 2.0;
            }
            self.motion.y += FULL_BUOYANCY;
        }
        if let Some(rider) = rider {
            self.paddle(rider);
        }
        self.cap_speed(speed_before);
        if on_ground {
            self.motion *= 0.5;
        }
        let moved = move_entity(
            bounds,
            self.motion,
            world,
            MoveOptions {
                on_ground,
                ..MoveOptions::default()
            },
        );
        // `Entity.moveEntity` stops each blocked axis (`Block.onLanded` for y).
        if moved.blocked_x {
            self.motion.x = 0.0;
        }
        if moved.blocked_z {
            self.motion.z = 0.0;
        }
        if moved.blocked_vertically {
            self.motion.y = 0.0;
        }
        // A fast crash breaks the boat on the server; the client only skips
        // the damping that tick.
        if !(moved.blocked_horizontally && speed_before > 0.2975) {
            self.motion *= DRIFT_DAMPING;
        }
        let new_position = feet_of(&moved.bounds);
        Driven {
            position: new_position,
            yaw: turn_towards_motion(yaw, position, new_position),
            on_ground: moved.on_ground,
        }
    }

    /// Adds the rider's keys, along the rider's yaw turned by the strafe.
    fn paddle(&mut self, rider: RiderInput) {
        // Float maths, as `riddenByEntity.rotationYaw + -moveStrafing * 90F`.
        let heading = rider.yaw + -rider.strafe * 90.0;
        let radians = f64::from(heading * std::f32::consts::PI / 180.0);
        let push = self.speed_multiplier * f64::from(rider.forward) * PADDLE;
        self.motion.x += -radians.sin() * push;
        self.motion.z += radians.cos() * push;
    }

    /// Caps the horizontal speed at 0.35 and lets the speed multiplier grow
    /// while the boat speeds up, shrink otherwise.
    fn cap_speed(&mut self, speed_before: f64) {
        let mut speed = self.motion.x.hypot(self.motion.z);
        if speed > MAX_SPEED {
            let scale = MAX_SPEED / speed;
            self.motion.x *= scale;
            self.motion.z *= scale;
            speed = MAX_SPEED;
        }
        if speed > speed_before && self.speed_multiplier < MAX_SPEED {
            self.speed_multiplier += (MAX_SPEED - self.speed_multiplier) / 35.0;
            self.speed_multiplier = self.speed_multiplier.min(MAX_SPEED);
        } else {
            self.speed_multiplier -= (self.speed_multiplier - MIN_SPEED_MULTIPLIER) / 35.0;
            self.speed_multiplier = self.speed_multiplier.max(MIN_SPEED_MULTIPLIER);
        }
    }
}

/// How much of the boat is in water, in fifths: each slice of its height,
/// lowered by 1/8, that touches water counts (`World.isAABBInMaterial`).
fn water_fraction<S: BlockSource + ?Sized>(bounds: &Aabb, world: &S) -> f64 {
    let height = bounds.max.y - bounds.min.y;
    let slices = f64::from(WATER_SLICES);
    (0..WATER_SLICES)
        .filter(|&slice| {
            let low = bounds.min.y + height * f64::from(slice) / slices - 0.125;
            let high = bounds.min.y + height * f64::from(slice + 1) / slices - 0.125;
            touches_water(
                &Aabb::new(
                    DVec3::new(bounds.min.x, low, bounds.min.z),
                    DVec3::new(bounds.max.x, high, bounds.max.z),
                ),
                world,
            )
        })
        .count() as f64
        / slices
}

/// `World.isAABBInMaterial(box, water)`: a water block in the cells of the
/// box whose surface is at or above the bottom of the box.
fn touches_water<S: BlockSource + ?Sized>(bounds: &Aabb, world: &S) -> bool {
    let cells = |low: f64, high: f64| low.floor() as i32..(high + 1.0).floor() as i32;
    for x in cells(bounds.min.x, bounds.max.x) {
        for y in cells(bounds.min.y, bounds.max.y) {
            for z in cells(bounds.min.z, bounds.max.z) {
                let state = world.block(x, y, z);
                if !Liquid::Water.contains(state) {
                    continue;
                }
                let level = state.metadata();
                let surface = if level < 8 {
                    f64::from(y + 1) - f64::from(level) / 8.0
                } else {
                    f64::from(y + 1)
                };
                if surface >= bounds.min.y {
                    return true;
                }
            }
        }
    }
    false
}

/// The feet of an entity from its box (`Entity.resetPositionToBB`).
fn feet_of(bounds: &Aabb) -> DVec3 {
    DVec3::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        bounds.min.y,
        (bounds.min.z + bounds.max.z) / 2.0,
    )
}

/// Turns `yaw` at most 20 degrees towards the way the boat moved from `from`
/// to `to`; a boat that barely moved keeps its yaw.
fn turn_towards_motion(yaw: f32, from: DVec3, to: DVec3) -> f32 {
    let (dx, dz) = (from.x - to.x, from.z - to.z);
    let heading = if dx * dx + dz * dz > 0.001 {
        f64::from((dz.atan2(dx) * 180.0 / std::f64::consts::PI) as f32)
    } else {
        f64::from(yaw)
    };
    let turn =
        f64::from(wrap_degrees((heading - f64::from(yaw)) as f32)).clamp(-MAX_TURN, MAX_TURN);
    (f64::from(yaw) + turn) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_world::BlockState;

    /// Still water from y 60 to 63 (surface at 64) over a stone bed, with a
    /// stone wall at `x = wall` when given.
    struct Lake {
        wall: Option<i32>,
    }

    impl BlockSource for Lake {
        fn block(&self, x: i32, y: i32, _z: i32) -> BlockState {
            if self.wall == Some(x) && y >= 59 {
                return BlockState::new(1, 0);
            }
            match y {
                59 => BlockState::new(1, 0),
                60..=63 => BlockState::new(9, 0),
                _ => BlockState::AIR,
            }
        }
    }

    fn run(
        boat: &mut BoatMotion,
        mut at: Driven,
        rider: Option<RiderInput>,
        lake: &Lake,
        ticks: usize,
    ) -> Driven {
        for _ in 0..ticks {
            at = boat.drive(at.position, at.yaw, at.on_ground, rider, lake);
        }
        at
    }

    fn afloat() -> Driven {
        Driven {
            position: DVec3::new(0.0, 63.6, 0.0),
            yaw: 0.0,
            on_ground: false,
        }
    }

    const AHEAD: RiderInput = RiderInput {
        forward: 1.0,
        strafe: 0.0,
        yaw: 0.0,
    };

    #[test]
    fn an_empty_driven_boat_settles_at_the_water_surface() {
        let lake = Lake { wall: None };
        let mut boat = BoatMotion::default();
        let rest = run(&mut boat, afloat(), None, &lake, 400);
        // It floats: neither sunk to the bed nor out of the water.
        assert!(
            rest.position.y > 63.0 && rest.position.y < 64.2,
            "{}",
            rest.position.y
        );
        assert!(boat.motion.y.abs() < 0.02, "{}", boat.motion.y);
        assert!(rest.position.x.abs() < 1e-9);
    }

    #[test]
    fn the_forward_key_pushes_the_boat_along_the_rider_yaw_up_to_the_top_speed() {
        let lake = Lake { wall: None };
        let mut boat = BoatMotion::default();
        let start = run(&mut boat, afloat(), None, &lake, 100);
        let before = start.position;
        let after = run(&mut boat, start, Some(AHEAD), &lake, 200);
        // Yaw 0 faces +z.
        assert!(after.position.z - before.z > 10.0, "{after:?}");
        assert!((after.position.x - before.x).abs() < 1e-6);
        let speed = boat.motion.x.hypot(boat.motion.z);
        assert!(speed <= MAX_SPEED + 1e-9, "{speed}");
        assert!(boat.speed_multiplier > 0.3, "{}", boat.speed_multiplier);
    }

    #[test]
    fn the_boat_turns_twenty_degrees_a_tick_at_most_towards_its_course() {
        let from = DVec3::ZERO;
        // Moving towards -x is a yaw of 0 in the `atan2(dz, dx)` of the boat.
        assert_eq!(
            turn_towards_motion(90.0, from, DVec3::new(-1.0, 0.0, 0.0)),
            70.0
        );
        assert_eq!(
            turn_towards_motion(5.0, from, DVec3::new(-1.0, 0.0, 0.0)),
            0.0
        );
        // A boat that barely moved keeps its yaw.
        assert_eq!(
            turn_towards_motion(42.0, from, DVec3::new(0.01, 0.0, 0.0)),
            42.0
        );
    }

    #[test]
    fn a_wall_stops_the_boat() {
        let lake = Lake { wall: Some(3) };
        let mut boat = BoatMotion::default();
        let start = run(&mut boat, afloat(), None, &lake, 100);
        let east = RiderInput {
            yaw: -90.0,
            ..AHEAD
        };
        let after = run(&mut boat, start, Some(east), &lake, 100);
        // Half the 1.5 width short of the wall at x = 3.
        assert!((after.position.x - 2.25).abs() < 1e-6, "{after:?}");
        assert!(boat.motion.x.abs() < 0.05);
    }

    #[test]
    fn a_drifting_boat_moves_by_its_motion_then_slows_down() {
        let mut boat = BoatMotion::default();
        boat.set_velocity(Vec3::new(0.2, 0.0, 0.0));
        let next = boat.drift(DVec3::ZERO, false);
        assert!((next.x - 0.2).abs() < 1e-6);
        assert!((boat.motion.x - 0.2 * 0.99).abs() < 1e-6);
    }

    #[test]
    fn a_boat_on_the_ground_loses_half_its_motion_more() {
        let mut boat = BoatMotion::default();
        boat.set_velocity(Vec3::new(0.2, 0.0, 0.0));
        let _ = boat.drift(DVec3::ZERO, true);
        assert!((boat.motion.x - 0.1 * 0.99).abs() < 1e-6);
    }
}
