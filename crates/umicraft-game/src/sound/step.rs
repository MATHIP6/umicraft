// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sounds the local player makes by moving: footsteps, swimming, the
//! splash of entering water and the landing after a fall.
//!
//! All of them are played by the client in 1.8, from `Entity.moveEntity`:
//! `EntityPlayerSP.playSound` calls `World.playSound` directly, and the
//! server only tells the *other* players (`EntityPlayer.playSound` is
//! `playSoundToNearExcept(this, ...)`). The reference has no jump sound.
//!
//! [`StepFacts`] is what the physics step records; [`Stepper`] is the small
//! state machine (`distanceWalkedOnStepModified`, `nextStepDistance`) that
//! turns it into [`SoundCue`]s. The machine reads the world only for the
//! block under the feet, so it is testable without a game loop.
//!
//! Sources: MCP `Entity.moveEntity` (lines 806-885), `Entity.playStepSound`,
//! `Entity.handleWaterMovement` / `resetHeight`, `EntityLivingBase.fall`,
//! `EntityPlayer.getSwimSound` / `getSplashSound` / `getFallSoundString` /
//! `fall` / `canTriggerWalking`, `BlockSlime.onFallenUpon`.
//! The 1.7 name `liquid.swim` no longer exists: 1.8 plays
//! `game.player.swim`.

use std::borrow::Cow;

use glam::DVec3;

use super::cue::SoundCue;
use super::effect_packet::{spread, CueRandom};
use super::sound_packet::infer_category;
use super::{sound_type, SNOW};
use crate::block_data::{Material, BLOCKS};
use crate::physics::Player;
use crate::shapes::BlockSource;

/// `Block.snow_layer`, whose sound wins over the block under it.
const SNOW_LAYER: u16 = 78;
/// `Block.ladder`: the only block whose climbing counts as walked distance.
const LADDER: u16 = 65;
/// `Block.slime_block`, which cancels the fall damage of a non-sneaking player.
const SLIME_BLOCK: u16 = 165;
/// The fences, nether-brick fence, fence gates and walls that a player
/// standing on top of them hears, since their collision box is 1.5 high.
const FENCES: [u16; 14] = [
    85, 113, 188, 189, 190, 191, 192, 107, 183, 184, 185, 186, 187, 139,
];
/// Walked distance (per 0.6 of vanilla scaling) between two steps.
const DISTANCE_SCALE: f64 = 0.6;
/// `Entity.playStepSound`: footsteps play at 15 % of the block volume.
const STEP_VOLUME: f32 = 0.15;
/// `EntityLivingBase.fall`: the block sound of a hard landing.
const LANDING_VOLUME: f32 = 0.5;
const LANDING_PITCH: f32 = 0.75;
/// Fall damage above which the big hurt sound plays (`getFallSoundString`).
const BIG_FALL_DAMAGE: i32 = 4;

/// What one physics tick did that the movement sounds depend on; the physics
/// fills it in (`Player::step_facts`) and [`Stepper::tick`] reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StepFacts {
    /// `moveEntity` ran its collision branch (false for a spectator).
    pub moved: bool,
    /// The walking branch ran: the player was not flying and did not sneak
    /// on the ground (`canTriggerWalking`, the safe-walk flag).
    pub walks: bool,
    /// The displacement of the move (`posX - d0` and so on).
    pub delta: DVec3,
    /// The motion after the move, before `doBlockCollisions`.
    pub velocity: DVec3,
    /// The `fallDistance` at the moment of landing, 0 when not landing.
    pub landed_fall: f32,
    /// The motion at the moment the player entered water, if it did.
    pub splash_velocity: Option<DVec3>,
}

/// The walked-distance counters of `Entity` and the cues they trigger.
#[derive(Debug, Clone, PartialEq)]
pub struct Stepper {
    /// `distanceWalkedOnStepModified`.
    distance: f32,
    /// `nextStepDistance`, which the constructor sets to 1.
    next_step: i32,
    /// `firstUpdate`: no splash on the tick the entity appears.
    first_update: bool,
}

impl Default for Stepper {
    fn default() -> Self {
        Self {
            distance: 0.0,
            next_step: 1,
            first_update: true,
        }
    }
}

impl Stepper {
    /// Creates the counters of a newly spawned player.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends the cues of the tick that just ran to `out`.
    ///
    /// `before` is the position at the start of the tick (where the splash
    /// plays), `player.position` the one after the move. The order is the
    /// reference's: splash, landing, then the step.
    pub fn tick<W: BlockSource + ?Sized>(
        &mut self,
        player: &Player,
        before: DVec3,
        world: &W,
        rng: &mut impl CueRandom,
        out: &mut Vec<SoundCue>,
    ) {
        let facts = &player.step_facts;
        if let Some(velocity) = facts.splash_velocity.filter(|_| !self.first_update) {
            out.push(splash_cue(before, velocity, rng));
        }
        self.first_update = false;
        if !facts.moved {
            return;
        }
        let (feet, block) = underfoot(world, player.position);
        if facts.landed_fall > 0.0 && !player.allow_flying {
            landing_cues(player, world, block, out);
        }
        if facts.walks {
            self.walk(player, world, feet, block, rng, out);
        }
    }

    /// The step branch of `Entity.moveEntity`.
    fn walk<W: BlockSource + ?Sized>(
        &mut self,
        player: &Player,
        world: &W,
        feet: [i32; 3],
        block: u16,
        rng: &mut impl CueRandom,
        out: &mut Vec<SoundCue>,
    ) {
        let delta = player.step_facts.delta;
        // Climbing only counts on a ladder (`block1 != Blocks.ladder`).
        let rise = if block == LADDER { delta.y } else { 0.0 };
        let walked = f64::from(length_f32(
            delta.x * delta.x + rise * rise + delta.z * delta.z,
        ));
        self.distance = (f64::from(self.distance) + walked * DISTANCE_SCALE) as f32;
        if self.distance <= self.next_step as f32 || block == 0 {
            return;
        }
        self.next_step = self.distance as i32 + 1;
        if player.in_water {
            out.push(swim_cue(player, rng));
        }
        out.extend(step_cue(world, feet, block, player.position));
    }
}

/// `MathHelper.sqrt_double`: the square root, rounded to a float.
fn length_f32(squared: f64) -> f32 {
    squared.sqrt() as f32
}

/// The block that `moveEntity` treats as the one under the feet: the block
/// at `y - 0.2`, or the fence under an air block (`Entity.java:815-826`).
fn underfoot<W: BlockSource + ?Sized>(world: &W, position: DVec3) -> ([i32; 3], u16) {
    let at = [
        position.x.floor() as i32,
        (position.y - 0.2).floor() as i32,
        position.z.floor() as i32,
    ];
    let id = world.block(at[0], at[1], at[2]).id();
    if id == 0 {
        let below = world.block(at[0], at[1] - 1, at[2]).id();
        if FENCES.contains(&below) {
            return ([at[0], at[1] - 1, at[2]], below);
        }
    }
    (at, id)
}

/// The material of a block id; id 0 is air.
fn material(id: u16) -> Material {
    BLOCKS
        .get(usize::from(id))
        .map_or(Material::Other, |data| data.material)
}

/// A cue for a sound event of the player at `pos`.
fn cue(name: Cow<'static, str>, pos: DVec3, volume: f32, pitch: f32) -> SoundCue {
    SoundCue {
        category: infer_category(&name),
        name,
        pos: pos.to_array(),
        volume,
        pitch,
    }
}

/// `Entity.playStepSound`: a snow layer on top changes the sound, a liquid
/// is silent.
fn step_cue<W: BlockSource + ?Sized>(
    world: &W,
    feet: [i32; 3],
    block: u16,
    pos: DVec3,
) -> Option<SoundCue> {
    let snowy = world.block(feet[0], feet[1] + 1, feet[2]).id() == SNOW_LAYER;
    let kind = if snowy {
        SNOW
    } else if matches!(material(block), Material::Water | Material::Lava) {
        return None;
    } else {
        sound_type(block)
    };
    Some(cue(
        kind.step_sound().into_cow(),
        pos,
        kind.volume * STEP_VOLUME,
        kind.pitch,
    ))
}

/// `Entity.moveEntity` swimming branch: volume from the motion, pitch jitter.
fn swim_cue(player: &Player, rng: &mut impl CueRandom) -> SoundCue {
    let v = player.step_facts.velocity;
    let volume = (length_f32(v.x * v.x * 0.2 + v.y * v.y + v.z * v.z * 0.2) * 0.35).min(1.0);
    cue(
        Cow::Borrowed("game.player.swim"),
        player.position,
        volume,
        1.0 + spread(rng) * 0.4,
    )
}

/// `Entity.resetHeight`: the splash of entering water.
fn splash_cue(at: DVec3, v: DVec3, rng: &mut impl CueRandom) -> SoundCue {
    let volume = (length_f32(v.x * v.x * 0.2 + v.y * v.y + v.z * v.z * 0.2) * 0.2).min(1.0);
    cue(
        Cow::Borrowed("game.player.swim.splash"),
        at,
        volume,
        1.0 + spread(rng) * 0.4,
    )
}

/// `EntityLivingBase.fall`: the hurt sound of the damage the fall deals, then
/// the block under the feet again at half volume and a lower pitch.
/// The slime block turns the damage multiplier to 0 unless the player
/// sneaks (`BlockSlime.onFallenUpon`), which silences it.
fn landing_cues<W: BlockSource + ?Sized>(
    player: &Player,
    world: &W,
    block: u16,
    out: &mut Vec<SoundCue>,
) {
    let multiplier = if block == SLIME_BLOCK && !player.sneaking {
        0.0
    } else {
        1.0
    };
    let reduction = 3.0 + f32::from(player.jump_boost);
    let damage = ((player.step_facts.landed_fall - reduction) * multiplier).ceil() as i32;
    if damage <= 0 {
        return;
    }
    let name = if damage > BIG_FALL_DAMAGE {
        "game.player.hurt.fall.big"
    } else {
        "game.player.hurt.fall.small"
    };
    out.push(cue(Cow::Borrowed(name), player.position, 1.0, 1.0));
    let (x, y, z) = (
        player.position.x.floor() as i32,
        (player.position.y - 0.2).floor() as i32,
        player.position.z.floor() as i32,
    );
    let id = world.block(x, y, z).id();
    if id != 0 {
        let kind = sound_type(id);
        out.push(cue(
            kind.step_sound().into_cow(),
            player.position,
            kind.volume * LANDING_VOLUME,
            kind.pitch * LANDING_PITCH,
        ));
    }
}

#[cfg(test)]
mod tests;
