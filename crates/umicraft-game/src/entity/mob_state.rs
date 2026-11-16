// SPDX-License-Identifier: MIT OR Apache-2.0

//! The animation state a mob keeps on the client between packets: timers an
//! `Entity Status` starts (a rabbit's hop, a golem's swing, a wolf's shake, a
//! sheep's grazing) and the state a mob updates from its own motion (a
//! chicken's wings, a wolf's begging tilt).
//!
//! The server sends only the trigger; each client runs the timers down tick by
//! tick, as the reference's `onLivingUpdate` does, and the model reads them
//! through [`MobPose`] at the sub-tick fraction of the frame.

use glam::DVec3;

/// Ticks of a hop started by status 1 (`EntityRabbit.handleStatusUpdate`).
const RABBIT_HOP_TICKS: u8 = 10;
/// Ticks of a golem's swing started by status 4 (`EntityIronGolem`).
const GOLEM_ATTACK_TICKS: u8 = 10;
/// Ticks a golem holds out a poppy after status 11 (`EntityIronGolem`).
const GOLEM_ROSE_TICKS: u16 = 400;
/// Ticks of a sheep's grazing started by status 10 (`EntitySheep`).
const SHEEP_GRAZE_TICKS: u8 = 40;
/// How far a wolf's shake advances each tick, and where it ends
/// (`EntityWolf.onUpdate`: `timeWolfIsShaking += 0.05`, ends at 2).
const WOLF_SHAKE_STEP: f32 = 0.05;
const WOLF_SHAKE_END: f32 = 2.0;

/// What a mob does with its state each tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MobTick {
    /// Whether it rests on the ground.
    pub on_ground: bool,
    /// Whether a tamed wolf begs (metadata), which tilts its head.
    pub begging: bool,
}

/// The per-kind animation state.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MobState {
    /// The kind keeps no such state.
    #[default]
    None,
    /// A rabbit's hop (`field_175540_bm` of `field_175535_bn` ticks).
    Rabbit {
        /// Ticks into the hop.
        ticks: u8,
        /// Length of the hop, `0` when it rests.
        duration: u8,
    },
    /// An iron golem's swing and poppy.
    IronGolem {
        /// Ticks left of the swing (`attackTimer`).
        attack: u8,
        /// Ticks left holding the poppy (`holdRoseTick`).
        rose: u16,
    },
    /// A wolf's shake and begging tilt.
    Wolf(WolfShake),
    /// A sheep's grazing (`sheepTimer`).
    Sheep {
        /// Ticks of grazing left.
        timer: u8,
    },
    /// A chicken's wings (`EntityChicken` wing fields).
    Chicken(ChickenWings),
    /// Where a wither's side heads look.
    Wither(WitherHeads),
    /// A guardian's tail, spikes and pupil.
    Guardian(GuardianSwim),
}

/// A guardian's swimming (`EntityGuardian.onLivingUpdate`, the client part):
/// the tail's phase and speed, how far the spikes are out, and where the
/// pupil looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuardianSwim {
    tail: f32,
    prev_tail: f32,
    tail_speed: f32,
    spikes: f32,
    prev_spikes: f32,
    /// Whether it was in water at its last tick.
    pub in_water: bool,
    /// Where the pupil sits, vanilla `(x, y)` pixels.
    pub eye: [f32; 2],
    /// Source of the random flapping of the spikes on land.
    seed: u32,
    /// The entity it beams at, `0` for none (metadata entry 17).
    aimed_at: i32,
    /// Ticks the beam has charged (`field_175479_bo`), at most
    /// `charge_ticks`.
    charged: u16,
    /// Ticks the beam takes to charge (`func_175464_ck`).
    charge_ticks: u16,
}

/// Ticks a guardian's beam takes to charge, and an elder's
/// (`EntityGuardian.func_175464_ck`).
const GUARDIAN_CHARGE_TICKS: u16 = 80;
const ELDER_CHARGE_TICKS: u16 = 60;

/// What a guardian's model reads at one sub-tick fraction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GuardianPose {
    /// The phase of the tail's swing (`func_175471_a`).
    pub tail: f32,
    /// How far the spikes are out, `0..=1` (`func_175469_o`).
    pub extension: f32,
    /// Where the pupil sits, vanilla `(x, y)` pixels.
    pub eye: [f32; 2],
    /// How far the beam has charged (`func_175477_p`): `0` when it starts,
    /// `1` once fully charged (slightly over within the last tick).
    pub charge: f32,
}

/// The look of a wither's two side heads in degrees: yaw `field_82221_e`
/// and pitch `field_82220_d` of `EntityWither`. The store turns them towards
/// their targets each tick ([`WitherHeads::track`]); the model reads them
/// as they are, as `ModelWither.setLivingAnimations` does.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WitherHeads {
    /// The world yaw of each side head.
    pub yaw: [f32; 2],
    /// The pitch of each side head.
    pub pitch: [f32; 2],
}

/// A wolf's shake (`timeWolfIsShaking`) and begging course
/// (`headRotationCourse`), with their values one tick ago.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WolfShake {
    shaking: bool,
    time: f32,
    prev_time: f32,
    course: f32,
    prev_course: f32,
}

/// A chicken's wing beat and how far its wings are spread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChickenWings {
    rotation: f32,
    prev_rotation: f32,
    spread: f32,
    prev_spread: f32,
    delta: f32,
}

impl Default for ChickenWings {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            prev_rotation: 0.0,
            spread: 0.0,
            prev_spread: 0.0,
            delta: 1.0,
        }
    }
}

/// What the model of a mob reads from its state at one sub-tick fraction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MobPose {
    /// Nothing beyond the common inputs.
    #[default]
    None,
    /// How far through its hop a rabbit is, `0..=1`
    /// (`EntityRabbit.func_175521_o`).
    Rabbit {
        /// `0` at rest.
        hop: f32,
    },
    /// What an iron golem's arms do.
    IronGolem(GolemArms),
    /// A wolf's shake and head tilt.
    Wolf {
        /// `timeWolfIsShaking` at the fraction, `None` when it does not
        /// shake.
        shake: Option<f32>,
        /// The begging tilt of the head in radians
        /// (`EntityWolf.getInterestedAngle`).
        interest: f32,
    },
    /// A sheep's head while grazing.
    Sheep(SheepGraze),
    /// The roll of a chicken's wings (`RenderChicken.handleRotationFloat`).
    Chicken {
        /// `(sin(wingRotation) + 1) * destPos`.
        flap: f32,
    },
    /// Where a wither's side heads look.
    Wither(WitherHeads),
    /// A guardian's tail, spikes and pupil.
    Guardian(GuardianPose),
}

/// What an iron golem's arms do.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum GolemArms {
    /// They swing with the stride.
    #[default]
    Walking,
    /// They swing down on a target: `attackTimer - partial`, `10` to `0`.
    Attacking(f32),
    /// The right one holds out a poppy: `holdRoseTick` left.
    HoldingRose(u16),
}

/// Where a sheep's head is while it grazes (`EntitySheep.getHeadRotationPointY`
/// and `getHeadRotationAngleX`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SheepGraze {
    /// How far the head is lowered, `0..=1` (the model lowers it 9 pixels).
    pub lowered: f32,
    /// The head pitch in radians while grazing; `None` keeps the look pitch.
    pub angle: Option<f32>,
}

impl MobState {
    /// The state a newly spawned mob of `kind` starts with.
    #[must_use]
    /// `seed` makes the random draws of two mobs differ (the entity id).
    pub fn for_kind(kind: super::EntityKind, seed: u32) -> Self {
        use super::EntityKind as K;
        match kind {
            K::Rabbit => Self::Rabbit {
                ticks: 0,
                duration: 0,
            },
            K::IronGolem => Self::IronGolem { attack: 0, rose: 0 },
            K::Wolf => Self::Wolf(WolfShake::default()),
            K::Sheep => Self::Sheep { timer: 0 },
            K::Chicken => Self::Chicken(ChickenWings::default()),
            K::Wither => Self::Wither(WitherHeads::default()),
            K::Guardian => Self::Guardian(GuardianSwim::new(seed)),
            _ => Self::None,
        }
    }

    /// Applies an `Entity Status`: 1 starts a rabbit's hop, 4 a golem's
    /// swing, 11 its poppy, 8 a wolf's shake and 10 a sheep's grazing (their
    /// `handleStatusUpdate`). Returns whether the status was one of them.
    pub fn status(&mut self, status: i8) -> bool {
        match (self, status) {
            (Self::Rabbit { ticks, duration }, 1) => {
                *ticks = 0;
                *duration = RABBIT_HOP_TICKS;
            }
            (Self::IronGolem { attack, .. }, 4) => *attack = GOLEM_ATTACK_TICKS,
            // 1.8 sends 11 for both `setHoldingRose(true)` and `(false)`,
            // and the client always starts the 400 ticks.
            (Self::IronGolem { rose, .. }, 11) => *rose = GOLEM_ROSE_TICKS,
            (Self::Wolf(wolf), 8) => {
                wolf.shaking = true;
                wolf.time = 0.0;
                wolf.prev_time = 0.0;
            }
            (Self::Sheep { timer }, 10) => *timer = SHEEP_GRAZE_TICKS,
            _ => return false,
        }
        true
    }

    /// Advances one tick (the client part of each `onLivingUpdate`).
    pub fn tick(&mut self, tick: MobTick) {
        match self {
            // The wither's heads need the other entities and the guardian
            // needs the water: the store updates them.
            Self::None | Self::Wither(_) | Self::Guardian(_) => {}
            Self::Rabbit { ticks, duration } => {
                if ticks != duration {
                    *ticks += 1;
                } else if *duration != 0 {
                    *ticks = 0;
                    *duration = 0;
                }
            }
            Self::IronGolem { attack, rose } => {
                *attack = attack.saturating_sub(1);
                *rose = rose.saturating_sub(1);
            }
            Self::Wolf(wolf) => wolf.tick(tick.begging),
            Self::Sheep { timer } => *timer = timer.saturating_sub(1),
            Self::Chicken(wings) => wings.tick(tick.on_ground),
        }
    }

    /// `timeWolfIsShaking` of a wolf that shakes, as of the last tick (the
    /// reference spawns its water drops from it), `None` for any other mob
    /// or a wolf that does not shake.
    #[must_use]
    pub fn shake_time(&self) -> Option<f32> {
        match self {
            Self::Wolf(wolf) if wolf.shaking => Some(wolf.time),
            _ => None,
        }
    }

    /// What the model reads at the sub-tick fraction `alpha`.
    #[must_use]
    pub fn pose(&self, alpha: f32) -> MobPose {
        match *self {
            Self::None => MobPose::None,
            Self::Rabbit { ticks, duration } => MobPose::Rabbit {
                hop: if duration == 0 {
                    0.0
                } else {
                    (f32::from(ticks) + alpha) / f32::from(duration)
                },
            },
            Self::IronGolem { attack, rose } => MobPose::IronGolem(if attack > 0 {
                GolemArms::Attacking(f32::from(attack) - alpha)
            } else if rose > 0 {
                GolemArms::HoldingRose(rose)
            } else {
                GolemArms::Walking
            }),
            Self::Wolf(wolf) => MobPose::Wolf {
                shake: wolf
                    .shaking
                    .then_some(wolf.prev_time + (wolf.time - wolf.prev_time) * alpha),
                interest: (wolf.prev_course + (wolf.course - wolf.prev_course) * alpha)
                    * 0.15
                    * std::f32::consts::PI,
            },
            Self::Sheep { timer } => MobPose::Sheep(sheep_graze(timer, alpha)),
            Self::Chicken(wings) => MobPose::Chicken {
                flap: wings.flap(alpha),
            },
            Self::Wither(heads) => MobPose::Wither(heads),
            Self::Guardian(swim) => MobPose::Guardian(swim.pose(alpha)),
        }
    }
}

/// The three heads of a wither standing at `feet` with its torso turned to
/// `body_yaw` degrees: the centre one 3 blocks up, the side ones 2.2 up and
/// 1.3 to either side (`EntityWither.func_82214_u`, `func_82208_v`,
/// `func_82213_w`).
#[must_use]
pub fn wither_head_positions(feet: DVec3, body_yaw: f32) -> [DVec3; 3] {
    let side = |head: f32| {
        let angle = (body_yaw + 180.0 * head) / 180.0 * std::f32::consts::PI;
        feet + DVec3::new(
            f64::from(angle.cos()) * 1.3,
            2.2,
            f64::from(angle.sin()) * 1.3,
        )
    };
    [feet + DVec3::new(0.0, 3.0, 0.0), side(0.0), side(1.0)]
}

impl GuardianSwim {
    /// A guardian at rest, spikes in; `seed` makes the flapping of two
    /// stranded guardians differ.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self {
            tail: 0.0,
            prev_tail: 0.0,
            tail_speed: 0.0,
            spikes: 0.0,
            prev_spikes: 0.0,
            in_water: false,
            eye: [0.0; 2],
            seed: seed.wrapping_mul(0x9E37_79B9) | 1,
            aimed_at: 0,
            charged: 0,
            charge_ticks: GUARDIAN_CHARGE_TICKS,
        }
    }

    /// One tick (`EntityGuardian.onLivingUpdate` on the client): on land the
    /// tail thrashes and the spikes flap at random; in water the tail beats
    /// fast and the spikes slide in while it swims, and the tail slows and
    /// the spikes come out while it hovers.
    pub fn tick(&mut self, in_water: bool, moving: bool) {
        self.in_water = in_water;
        self.prev_tail = self.tail;
        if !in_water {
            self.tail_speed = 2.0;
        } else if moving {
            self.tail_speed = if self.tail_speed < 0.5 {
                4.0
            } else {
                self.tail_speed + (0.5 - self.tail_speed) * 0.1
            };
        } else {
            self.tail_speed += (0.125 - self.tail_speed) * 0.2;
        }
        self.tail += self.tail_speed;
        self.prev_spikes = self.spikes;
        if !in_water {
            self.spikes = self.next_unit();
        } else if moving {
            self.spikes += (0.0 - self.spikes) * 0.25;
        } else {
            self.spikes += (1.0 - self.spikes) * 0.06;
        }
    }

    /// One tick of the beam at `target` (metadata entry 17, `0` for none):
    /// a new target restarts the charge (`onDataWatcherUpdate(17)`), which
    /// then grows a tick at a time up to 60 ticks for an elder and 80
    /// otherwise (`onLivingUpdate`).
    pub fn aim(&mut self, target: i32, elder: bool) {
        if target != self.aimed_at {
            self.aimed_at = target;
            self.charged = 0;
        }
        self.charge_ticks = if elder {
            ELDER_CHARGE_TICKS
        } else {
            GUARDIAN_CHARGE_TICKS
        };
        if target != 0 && self.charged < self.charge_ticks {
            self.charged += 1;
        }
    }

    /// How far the beam has charged at the sub-tick fraction `alpha`
    /// (`func_175477_p`, which adds the fraction even when fully charged).
    #[must_use]
    pub fn charge(&self, alpha: f32) -> f32 {
        (f32::from(self.charged) + alpha) / f32::from(self.charge_ticks)
    }

    /// The values at the sub-tick fraction `alpha`.
    fn pose(&self, alpha: f32) -> GuardianPose {
        GuardianPose {
            tail: self.prev_tail + (self.tail - self.prev_tail) * alpha,
            extension: self.prev_spikes + (self.spikes - self.prev_spikes) * alpha,
            eye: self.eye,
            charge: self.charge(alpha),
        }
    }

    /// `rand.nextFloat()` of a small xorshift generator.
    fn next_unit(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Moves the pupil towards `watched` (MCP `ModelGuardian
    /// .setRotationAngles`): up when it is above the eyes, and to the side
    /// it lies on, relative to where the guardian looks.
    pub fn look_at(&mut self, eyes: DVec3, look_yaw: f32, look_pitch: f32, watched: DVec3) {
        let up = if watched.y - eyes.y > 0.0 { 0.0 } else { 1.0 };
        let look = crate::raycast::look_direction(look_yaw.to_radians(), look_pitch.to_radians());
        let look = DVec3::new(look.x, 0.0, look.z);
        let away = DVec3::new(eyes.x - watched.x, 0.0, eyes.z - watched.z);
        let away = if away.length() < 1.0e-4 {
            DVec3::ZERO
        } else {
            away.normalize()
        };
        // `Vec3.rotateYaw(PI / 2)`.
        let side = DVec3::new(away.z, 0.0, -away.x);
        let d = look.dot(side) as f32;
        let sign = if d > 0.0 {
            1.0
        } else if d < 0.0 {
            -1.0
        } else {
            0.0
        };
        self.eye = [d.abs().sqrt() * 2.0 * sign, up];
    }
}

impl WitherHeads {
    /// `EntityWither.onLivingUpdate`, the part the client runs: each side
    /// head turns towards the eyes of its target, by at most 10 degrees of
    /// yaw and 40 of pitch a tick, or back to the torso without one.
    pub fn track(&mut self, feet: DVec3, body_yaw: f32, targets: [Option<DVec3>; 2]) {
        let [_, first, second] = wither_head_positions(feet, body_yaw);
        for (head, (from, target)) in [first, second].into_iter().zip(targets).enumerate() {
            let Some(eyes) = target else {
                self.yaw[head] = approach(self.yaw[head], body_yaw, 10.0);
                continue;
            };
            let to = eyes - from;
            let flat = to.x.hypot(to.z);
            let yaw = (to.z.atan2(to.x).to_degrees() as f32) - 90.0;
            let pitch = -(to.y.atan2(flat).to_degrees() as f32);
            self.pitch[head] = approach(self.pitch[head], pitch, 40.0);
            self.yaw[head] = approach(self.yaw[head], yaw, 10.0);
        }
    }
}

/// `EntityWither.func_82204_b`: `from` turned towards `to` by at most `step`
/// degrees, the short way round.
fn approach(from: f32, to: f32, step: f32) -> f32 {
    from + super::angle::wrap_degrees(to - from).clamp(-step, step)
}

impl WolfShake {
    /// `EntityWolf.onUpdate`, the part the client runs: the begging course
    /// eases towards 1 or 0, and a shake runs for 2 units of 0.05 per tick.
    fn tick(&mut self, begging: bool) {
        self.prev_course = self.course;
        let goal = if begging { 1.0 } else { 0.0 };
        self.course += (goal - self.course) * 0.4;
        if self.shaking {
            self.prev_time = self.time;
            self.time += WOLF_SHAKE_STEP;
            if self.prev_time >= WOLF_SHAKE_END {
                *self = Self {
                    course: self.course,
                    prev_course: self.prev_course,
                    ..Self::default()
                };
            }
        }
    }
}

impl ChickenWings {
    /// `EntityChicken.onLivingUpdate`: the wings spread in the air and fold
    /// on the ground, and beat faster just after a take-off.
    fn tick(&mut self, on_ground: bool) {
        self.prev_rotation = self.rotation;
        self.prev_spread = self.spread;
        let step = if on_ground { -1.0 } else { 4.0 };
        self.spread = (self.spread + step * 0.3).clamp(0.0, 1.0);
        if !on_ground && self.delta < 1.0 {
            self.delta = 1.0;
        }
        self.delta *= 0.9;
        self.rotation += self.delta * 2.0;
    }

    /// `RenderChicken.handleRotationFloat`.
    fn flap(&self, alpha: f32) -> f32 {
        let rotation = self.prev_rotation + (self.rotation - self.prev_rotation) * alpha;
        let spread = self.prev_spread + (self.spread - self.prev_spread) * alpha;
        (rotation.sin() + 1.0) * spread
    }
}

/// `EntitySheep.getHeadRotationPointY` and `getHeadRotationAngleX` for
/// `timer` ticks of grazing left: the head goes down over 4 ticks, nibbles
/// for 32 and comes back up over 4.
fn sheep_graze(timer: u8, alpha: f32) -> SheepGraze {
    let ticks = f32::from(timer);
    let lowered = match timer {
        0 => 0.0,
        4..=36 => 1.0,
        1..=3 => (ticks - alpha) / 4.0,
        _ => -((ticks - 40.0) - alpha) / 4.0,
    };
    let angle = match timer {
        0 => None,
        5..=36 => {
            let nibble = ((ticks - 4.0) - alpha) / 32.0;
            Some(std::f32::consts::PI / 5.0 + std::f32::consts::PI * 0.07 * (nibble * 28.7).sin())
        }
        _ => Some(std::f32::consts::PI / 5.0),
    };
    SheepGraze { lowered, angle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityKind;

    fn ticked(state: &mut MobState, ticks: usize) {
        for _ in 0..ticks {
            state.tick(MobTick::default());
        }
    }

    #[test]
    fn a_rabbit_hops_over_ten_ticks_after_status_one() {
        let mut rabbit = MobState::for_kind(EntityKind::Rabbit, 0);
        assert_eq!(rabbit.pose(0.5), MobPose::Rabbit { hop: 0.0 });
        assert!(rabbit.status(1));
        assert_eq!(rabbit.pose(0.5), MobPose::Rabbit { hop: 0.05 });
        ticked(&mut rabbit, 5);
        assert_eq!(rabbit.pose(0.0), MobPose::Rabbit { hop: 0.5 });
        ticked(&mut rabbit, 5);
        assert_eq!(rabbit.pose(0.0), MobPose::Rabbit { hop: 1.0 });
        ticked(&mut rabbit, 1);
        assert_eq!(rabbit.pose(0.5), MobPose::Rabbit { hop: 0.0 }, "landed");
    }

    #[test]
    fn a_golem_swings_then_holds_a_poppy() {
        let mut golem = MobState::for_kind(EntityKind::IronGolem, 0);
        assert_eq!(golem.pose(0.0), MobPose::IronGolem(GolemArms::Walking));
        golem.status(11);
        golem.status(4);
        assert_eq!(
            golem.pose(0.25),
            MobPose::IronGolem(GolemArms::Attacking(9.75))
        );
        ticked(&mut golem, 10);
        assert_eq!(
            golem.pose(0.0),
            MobPose::IronGolem(GolemArms::HoldingRose(390)),
            "the swing wins over the poppy while it lasts"
        );
        ticked(&mut golem, 390);
        assert_eq!(golem.pose(0.0), MobPose::IronGolem(GolemArms::Walking));
    }

    #[test]
    fn a_wolf_shakes_for_two_seconds_after_status_eight() {
        let mut wolf = MobState::for_kind(EntityKind::Wolf, 0);
        assert!(matches!(wolf.pose(0.0), MobPose::Wolf { shake: None, .. }));
        wolf.status(8);
        ticked(&mut wolf, 10);
        let MobPose::Wolf { shake, .. } = wolf.pose(0.5) else {
            panic!("a wolf pose");
        };
        let shake = shake.expect("shaking");
        assert!((shake - 0.475).abs() < 1e-5, "{shake}");
        ticked(&mut wolf, 32);
        assert!(matches!(wolf.pose(0.0), MobPose::Wolf { shake: None, .. }));
    }

    #[test]
    fn a_begging_wolf_tilts_its_head_gradually() {
        let mut wolf = MobState::for_kind(EntityKind::Wolf, 0);
        let tilt = |wolf: &MobState| match wolf.pose(1.0) {
            MobPose::Wolf { interest, .. } => interest,
            _ => f32::NAN,
        };
        let begging = MobTick {
            begging: true,
            ..MobTick::default()
        };
        wolf.tick(begging);
        let first = tilt(&wolf);
        assert!(
            first > 0.0 && first < 0.15 * std::f32::consts::PI * 0.5,
            "{first}"
        );
        for _ in 0..30 {
            wolf.tick(begging);
        }
        assert!((tilt(&wolf) - 0.15 * std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn a_sheep_lowers_its_head_nibbles_and_looks_up() {
        let mut sheep = MobState::for_kind(EntityKind::Sheep, 0);
        assert_eq!(sheep.pose(0.0), MobPose::Sheep(SheepGraze::default()));
        sheep.status(10);
        // Timer 40 at the start: the head is still up.
        let MobPose::Sheep(start) = sheep.pose(0.0) else {
            panic!("a sheep pose");
        };
        assert_eq!(start.lowered, 0.0);
        ticked(&mut sheep, 10);
        let MobPose::Sheep(grazing) = sheep.pose(0.0) else {
            panic!("a sheep pose");
        };
        assert_eq!(grazing.lowered, 1.0);
        assert!(grazing.angle.is_some());
        ticked(&mut sheep, 30);
        assert_eq!(sheep.pose(0.0), MobPose::Sheep(SheepGraze::default()));
    }

    #[test]
    fn a_chicken_spreads_its_wings_in_the_air_and_folds_them_on_the_ground() {
        let mut chicken = MobState::for_kind(EntityKind::Chicken, 0);
        let flap = |chicken: &MobState| match chicken.pose(1.0) {
            MobPose::Chicken { flap } => flap,
            _ => f32::NAN,
        };
        let ground = MobTick {
            on_ground: true,
            ..MobTick::default()
        };
        chicken.tick(ground);
        assert_eq!(flap(&chicken), 0.0, "folded on the ground");
        let mut flaps = Vec::new();
        for _ in 0..10 {
            chicken.tick(MobTick::default());
            flaps.push(flap(&chicken));
        }
        assert!(flaps.iter().any(|&f| f > 0.5), "{flaps:?}");
        for _ in 0..5 {
            chicken.tick(ground);
        }
        assert_eq!(flap(&chicken), 0.0);
    }

    #[test]
    fn a_wither_head_turns_towards_its_target_ten_degrees_a_tick() {
        let mut heads = WitherHeads::default();
        let feet = DVec3::ZERO;
        // A target straight along +z of the first side head, at its height.
        let [_, first, _] = wither_head_positions(feet, 0.0);
        let target = first + DVec3::new(0.0, 0.0, 10.0);
        heads.track(feet, 0.0, [Some(target), None]);
        // `atan2(10, 0) - 90` is 0: already facing it.
        assert!(heads.yaw[0].abs() < 1e-4, "{:?}", heads.yaw);
        let behind = first + DVec3::new(0.0, 0.0, -10.0);
        heads.track(feet, 0.0, [Some(behind), None]);
        assert!((heads.yaw[0].abs() - 10.0).abs() < 1e-4, "{:?}", heads.yaw);
        // Without a target the head comes back to the torso.
        heads.yaw[1] = 30.0;
        heads.track(feet, 0.0, [None, None]);
        assert!((heads.yaw[1] - 20.0).abs() < 1e-4);
    }

    #[test]
    fn a_swimming_guardian_draws_its_spikes_in_and_beats_its_tail() {
        let mut swim = GuardianSwim::new(3);
        for _ in 0..60 {
            swim.tick(true, false);
        }
        let hovering = swim.pose(1.0);
        assert!(hovering.extension > 0.9, "{hovering:?}");
        for _ in 0..20 {
            swim.tick(true, true);
        }
        let swimming = swim.pose(1.0);
        assert!(swimming.extension < 0.01, "{swimming:?}");
        assert!(swimming.tail > hovering.tail + 10.0, "the tail beats fast");
        // Stranded, the spikes flap at random within 0..1.
        for _ in 0..20 {
            swim.tick(false, false);
            assert!((0.0..1.0).contains(&swim.spikes));
        }
    }

    #[test]
    fn a_guardian_beam_charges_and_restarts_on_a_new_target() {
        let mut swim = GuardianSwim::new(1);
        swim.aim(0, false);
        assert_eq!(swim.charged, 0, "no target, no charge");
        for _ in 0..100 {
            swim.aim(7, false);
        }
        assert_eq!(swim.charged, 80, "a guardian charges for 80 ticks");
        assert!((swim.pose(0.0).charge - 1.0).abs() < 1e-6);
        swim.aim(8, true);
        assert_eq!(swim.charged, 1, "a new target restarts the charge");
        assert!((swim.pose(0.5).charge - 1.5 / 60.0).abs() < 1e-6);
        for _ in 0..100 {
            swim.aim(8, true);
        }
        assert_eq!(swim.charged, 60, "an elder charges for 60 ticks");
        swim.aim(0, true);
        assert_eq!(swim.charged, 0, "losing the target resets it");
    }

    #[test]
    fn a_guardian_looks_up_and_to_the_side_of_what_it_watches() {
        let mut swim = GuardianSwim::new(1);
        let eyes = DVec3::ZERO;
        // Facing +z (yaw 0); the target is ahead and to its left (+x), above.
        swim.look_at(eyes, 0.0, 0.0, DVec3::new(3.0, 1.0, 5.0));
        assert_eq!(swim.eye[1], 0.0, "above: the pupil goes up");
        let side = swim.eye[0];
        swim.look_at(eyes, 0.0, 0.0, DVec3::new(-3.0, -1.0, 5.0));
        assert_eq!(swim.eye[1], 1.0);
        assert!(
            side * swim.eye[0] < 0.0,
            "the other side: {side} {:?}",
            swim.eye
        );
    }

    #[test]
    fn other_statuses_and_kinds_are_left_alone() {
        let mut zombie = MobState::for_kind(EntityKind::Zombie, 0);
        assert!(!zombie.status(4));
        let mut rabbit = MobState::for_kind(EntityKind::Rabbit, 0);
        assert!(!rabbit.status(4));
    }
}
