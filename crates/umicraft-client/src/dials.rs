// SPDX-License-Identifier: MIT OR Apache-2.0

//! The compass and clock sprites (P3): the frame follows the game state.
//!
//! Ports of MCP `TextureCompass.updateCompass` and `TextureClock.updateAnimation`.
//! Both ease a dial angle towards a target with a damped spring, once per
//! 20 Hz tick, then show frame `(angle + 1) * frames` of their strip. The
//! compass targets the world spawn relative to the player's heading, the
//! clock the celestial angle. Outside the overworld the target is random,
//! like the reference.
//!
//! [`DialAnimator::advance`] hands each changed layer to a writer (the block
//! array of the renderer) and records the frame so that an icon key can name
//! it: the icon cache holds one picture per frame.

use std::f64::consts::{PI, TAU};
use std::time::Duration;

use umicraft_assets::{DialKind, DialSprite};

/// Length of a game tick.
const TICK: Duration = Duration::from_millis(50);
/// Ticks replayed after a stall, at most (as in the texture animator).
const MAX_CATCH_UP_TICKS: u32 = 4;
/// Spring constants of `updateCompass` / `updateAnimation`.
const PULL: f64 = 0.1;
const DAMPING: f64 = 0.8;

/// The compass item id.
const COMPASS_ITEM: i16 = 345;
/// The clock item id.
const CLOCK_ITEM: i16 = 347;

/// Whether an item shows a dial sprite whose frame changes.
#[must_use]
pub fn is_dial(item_id: i16) -> bool {
    matches!(item_id, COMPASS_ITEM | CLOCK_ITEM)
}

/// The frame each dial shows, part of the icon key of the dial items so
/// that the icon cache keeps one picture per frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DialFrames {
    /// Frame of the compass strip.
    pub compass: u16,
    /// Frame of the clock strip.
    pub clock: u16,
}

impl DialFrames {
    /// The frame shown by the dial item `item_id`, `None` for other items.
    #[must_use]
    pub fn frame_of(self, item_id: i16) -> Option<u16> {
        match item_id {
            COMPASS_ITEM => Some(self.compass),
            CLOCK_ITEM => Some(self.clock),
            _ => None,
        }
    }
}

/// What the dials read from the game each tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialInputs {
    /// Whether the dimension is the overworld (`WorldProvider.isSurfaceWorld`).
    pub surface: bool,
    /// The player's `x` and `z`.
    pub position: (f64, f64),
    /// The player's yaw in degrees (`rotationYaw`).
    pub yaw_degrees: f64,
    /// The spawn point's `x` and `z`, when the server sent one.
    pub spawn: Option<(f64, f64)>,
    /// `World.getCelestialAngle(1.0)`.
    pub celestial_angle: f64,
}

/// One dial and its spring state.
#[derive(Debug)]
struct Dial {
    sprite: DialSprite,
    angle: f64,
    velocity: f64,
    frame: usize,
}

/// The compass and the clock of the bound block array.
#[derive(Debug, Default)]
pub struct DialAnimator {
    dials: Vec<Dial>,
    pending: Duration,
    random: Xorshift,
}

impl DialAnimator {
    /// An animator for `sprites`, each on its first frame.
    #[must_use]
    pub fn new(sprites: &[DialSprite]) -> Self {
        Self {
            dials: sprites
                .iter()
                .cloned()
                .map(|sprite| Dial {
                    sprite,
                    angle: 0.0,
                    velocity: 0.0,
                    frame: 0,
                })
                .collect(),
            pending: Duration::ZERO,
            random: Xorshift::default(),
        }
    }

    /// The frames of both dials, for the icon keys.
    #[must_use]
    pub fn frames(&self) -> DialFrames {
        DialFrames {
            compass: self.frame(DialKind::Compass) as u16,
            clock: self.frame(DialKind::Clock) as u16,
        }
    }

    /// The frame a dial shows now (`0` when the atlas has no such dial).
    #[must_use]
    pub fn frame(&self, kind: DialKind) -> usize {
        self.dials
            .iter()
            .find(|dial| dial.sprite.kind() == kind)
            .map_or(0, |dial| dial.frame)
    }

    /// Runs the ticks that `elapsed` completes and calls `write(layer,
    /// pixels)` for every dial whose frame changed.
    pub fn advance(
        &mut self,
        elapsed: Duration,
        inputs: Option<&DialInputs>,
        mut write: impl FnMut(u32, &[u8]),
    ) {
        if self.dials.is_empty() {
            return;
        }
        self.pending += elapsed;
        let mut ticks = 0;
        while self.pending >= TICK {
            self.pending -= TICK;
            ticks += 1;
        }
        for _ in 0..ticks.min(MAX_CATCH_UP_TICKS) {
            for dial in &mut self.dials {
                let layer = dial.sprite.layer();
                if let Some(pixels) = dial.tick(inputs, &mut self.random) {
                    write(layer, pixels);
                }
            }
        }
    }
}

impl Dial {
    /// One tick; the pixels of the new frame when it changed.
    fn tick(&mut self, inputs: Option<&DialInputs>, random: &mut Xorshift) -> Option<&[u8]> {
        let frames = self.sprite.frame_count();
        match self.sprite.kind() {
            DialKind::Compass => {
                let target = compass_target(inputs, random);
                self.angle += self.spring(wrap(target - self.angle, PI, TAU));
            }
            DialKind::Clock => {
                let target = match inputs {
                    Some(inputs) if inputs.surface => inputs.celestial_angle,
                    Some(_) => random.next_unit(),
                    None => 0.0,
                };
                self.angle += self.spring(wrap(target - self.angle, 0.5, 1.0));
            }
        }
        let scaled = match self.sprite.kind() {
            DialKind::Compass => self.angle / TAU + 1.0,
            DialKind::Clock => self.angle + 1.0,
        };
        let frame = frame_index(scaled, frames);
        if frame == self.frame {
            return None;
        }
        self.frame = frame;
        self.sprite.frame(frame)
    }

    /// The damped pull towards the target: the change of the angle.
    fn spring(&mut self, difference: f64) -> f64 {
        self.velocity += difference.clamp(-1.0, 1.0) * PULL;
        self.velocity *= DAMPING;
        self.velocity
    }
}

/// The compass target angle (`updateCompass`): the spawn bearing relative to
/// the heading, or a random angle outside the overworld.
fn compass_target(inputs: Option<&DialInputs>, random: &mut Xorshift) -> f64 {
    let Some(inputs) = inputs else { return 0.0 };
    if !inputs.surface {
        return random.next_unit() * TAU;
    }
    let Some((spawn_x, spawn_z)) = inputs.spawn else {
        return 0.0;
    };
    let dx = spawn_x - inputs.position.0;
    let dz = spawn_z - inputs.position.1;
    let heading = (inputs.yaw_degrees % 360.0 - 90.0).to_radians();
    -(heading - dz.atan2(dx))
}

/// `value` brought into `[-half, half)` by adding or removing `full`.
fn wrap(mut value: f64, half: f64, full: f64) -> f64 {
    while value < -half {
        value += full;
    }
    while value >= half {
        value -= full;
    }
    value
}

/// `(int) scaled % frames`, made non-negative (the Java loop).
fn frame_index(scaled: f64, frames: usize) -> usize {
    let count = frames as i64;
    (((scaled * frames as f64) as i64 % count) + count) as usize % frames
}

/// A tiny deterministic generator for the random dial targets outside the
/// overworld (the reference uses `Math.random`).
#[derive(Debug)]
struct Xorshift(u64);

impl Default for Xorshift {
    fn default() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }
}

impl Xorshift {
    /// A value in `0..1`.
    fn next_unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(kind: DialKind, frames: usize) -> DialSprite {
        let atlas_frames: Vec<Vec<u8>> = (0..frames).map(|index| vec![index as u8; 4]).collect();
        DialSprite::new(1, kind, atlas_frames).expect("two frames")
    }

    fn inputs() -> DialInputs {
        DialInputs {
            surface: true,
            position: (0.0, 0.0),
            yaw_degrees: 0.0,
            spawn: Some((10.0, 0.0)),
            celestial_angle: 0.0,
        }
    }

    fn run(animator: &mut DialAnimator, inputs: &DialInputs, ticks: u32) -> Vec<u8> {
        let mut written = Vec::new();
        for _ in 0..ticks {
            animator.advance(TICK, Some(inputs), |_, pixels| written.push(pixels[0]));
        }
        written
    }

    #[test]
    fn frame_indices_wrap_like_the_java_loop() {
        assert_eq!(frame_index(1.0, 32), 0);
        assert_eq!(frame_index(1.5, 32), 16);
        assert_eq!(frame_index(0.5, 32), 16);
        assert_eq!(frame_index(-0.25, 32), 24);
    }

    #[test]
    fn the_compass_settles_on_the_bearing_of_the_spawn() {
        let mut animator = DialAnimator::new(&[sprite(DialKind::Compass, 32)]);
        run(&mut animator, &inputs(), 200);
        let facing_spawn = animator.frame(DialKind::Compass);
        // Turn the player a quarter turn: the needle ends on another frame.
        let turned = DialInputs {
            yaw_degrees: 90.0,
            ..inputs()
        };
        run(&mut animator, &turned, 200);
        assert_ne!(animator.frame(DialKind::Compass), facing_spawn);
    }

    #[test]
    fn the_clock_follows_the_celestial_angle() {
        let mut animator = DialAnimator::new(&[sprite(DialKind::Clock, 64)]);
        let noon = DialInputs {
            celestial_angle: 0.0,
            ..inputs()
        };
        run(&mut animator, &noon, 300);
        assert_eq!(animator.frame(DialKind::Clock), 0);
        let midnight = DialInputs {
            celestial_angle: 0.5,
            ..inputs()
        };
        run(&mut animator, &midnight, 300);
        assert_eq!(animator.frame(DialKind::Clock), 32);
    }

    #[test]
    fn a_dial_writes_only_when_its_frame_changes_and_no_dial_writes_nothing() {
        let mut animator = DialAnimator::new(&[sprite(DialKind::Clock, 64)]);
        let steady = inputs();
        run(&mut animator, &steady, 300);
        assert!(run(&mut animator, &steady, 10).is_empty());
        let mut none = DialAnimator::default();
        none.advance(TICK, Some(&steady), |_, _| panic!("no dial"));
    }

    #[test]
    fn a_stall_replays_at_most_a_few_ticks() {
        let mut animator = DialAnimator::new(&[sprite(DialKind::Clock, 64)]);
        let midnight = DialInputs {
            celestial_angle: 0.5,
            ..inputs()
        };
        let mut writes = 0;
        animator.advance(Duration::from_secs(60), Some(&midnight), |_, _| writes += 1);
        assert!(writes <= MAX_CATCH_UP_TICKS as usize);
    }

    #[test]
    fn outside_the_overworld_the_targets_are_random_but_stay_deterministic() {
        let nether = DialInputs {
            surface: false,
            ..inputs()
        };
        let frames_of = |seed_runs: u32| {
            let mut animator = DialAnimator::new(&[sprite(DialKind::Compass, 32)]);
            run(&mut animator, &nether, seed_runs);
            animator.frame(DialKind::Compass)
        };
        assert_eq!(frames_of(50), frames_of(50));
    }
}
