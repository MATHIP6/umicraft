// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's input: the key state, the update that crosses into the game
//! thread, and the scripted input of the headless mode.
//!
//! The window and the headless script are two *input sources*: both produce
//! [`InputUpdate`] messages and the game thread cannot tell them apart. The
//! interactive client and the acceptance session therefore drive the exact
//! same input path (spec §11 wants the headless mode to run the real
//! integration suite, not a separate simulation).

use std::time::{Duration, Instant};

/// The movement key state, sampled by an input source.
///
/// The axes are the analogue values of the movement keys. The sign of the
/// strafe axis follows MCP `net/minecraft/util/MovementInput.java`, where a
/// positive number means "strafe left".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputState {
    /// The forward axis: `-1.0` is backwards, `1.0` forwards.
    pub forward: f32,
    /// The strafe axis: `-1.0` is to the right, `1.0` to the left.
    pub strafe: f32,
    /// Whether the jump key is held.
    pub jump: bool,
    /// Whether the sneak key is held.
    pub sneak: bool,
    /// Whether the sprint key is held.
    pub sprint: bool,
}

impl InputState {
    /// A state that only holds the sprint key.
    ///
    /// The sprint key is the "sprint while moving" modifier; the game logic
    /// still needs a forward input above the vanilla threshold before the
    /// sprint starts (MCP `EntityPlayerSP.java:784-785`).
    #[must_use]
    pub const fn sprinting() -> Self {
        Self {
            forward: 0.0,
            strafe: 0.0,
            jump: false,
            sneak: false,
            sprint: true,
        }
    }
}

/// One input update sent from an input source to the game thread.
///
/// The message is plain data: the game thread owns the player state and
/// applies the update to it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputUpdate {
    /// The new key state; it replaces the previous one.
    pub state: InputState,
    /// The yaw delta in radians; a positive delta turns the view to the right.
    pub look_yaw: f32,
    /// The pitch delta in radians; a positive delta looks down.
    pub look_pitch: f32,
    /// The hotbar slot the player selected, if any.
    pub select_slot: Option<u8>,
    /// Whether the player started breaking the targeted block.
    pub break_block: bool,
    /// Whether the player placed a block against the targeted face.
    pub place_block: bool,
    /// Whether the player pressed the perspective key (`F5`).
    pub toggle_perspective: bool,
}

/// The yaw the script turns per update, in radians.
///
/// The script is updated once per tick, so a quarter turn takes about eight
/// seconds.
const TURN_RATE: f32 = 0.08;

/// One step of a scripted session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptStep {
    /// Stand still.
    Idle,
    /// Walk forward.
    Walk,
    /// Walk forward while sprinting.
    Sprint,
    /// Walk forward while sneaking.
    Sneak,
    /// Walk forward and jump once a second.
    Jump,
    /// Steer the view down towards the ground in front of the player.
    LookDown,
    /// Steer the view back to the horizon.
    LookUp,
    /// Turn to the right, so the walk does not run into the block the script
    /// placed.
    Turn,
    /// Look around: turn right, then left, alternating every second, while
    /// the pitch is steered towards the phase's pitch.
    LookAround,
    /// Break the block under the crosshair.
    Break,
    /// Place a block against the targeted face.
    Place,
}

/// One phase of a scripted session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptPhase {
    /// What the player does during the phase.
    pub step: ScriptStep,
    /// How long the phase lasts.
    pub duration: Duration,
    /// The pitch the view is steered to, in radians; positive looks down.
    pub pitch: f32,
}

/// The scripted session of the headless mode.
///
/// The phases run in order and then repeat, so a long session keeps walking,
/// jumping, sprinting and interacting. The view is steered towards the pitch
/// of the current phase, which is what makes the break and the place phases
/// aim at the ground in front of the player.
#[derive(Debug, Clone)]
pub struct Script {
    phases: Vec<ScriptPhase>,
    index: usize,
    phase_started: Instant,
    pitch: f32,
    fired: bool,
    last_jump: Instant,
}

impl Script {
    /// Creates a script from its phases.
    ///
    /// An empty list is replaced by [`default_script`], so the script always
    /// has at least one phase.
    #[must_use]
    pub fn new(phases: Vec<ScriptPhase>) -> Self {
        let phases = if phases.is_empty() {
            default_script()
        } else {
            phases
        };
        Self {
            phases,
            index: 0,
            phase_started: Instant::now(),
            pitch: 0.0,
            fired: false,
            last_jump: Instant::now(),
        }
    }

    /// The phase the script is currently in.
    #[must_use]
    pub fn current(&self) -> ScriptPhase {
        self.phases[self.index]
    }

    /// Advances the script and returns the input update for `now`.
    ///
    /// The update carries the full key state, so the game thread never has to
    /// merge partial updates.
    #[must_use]
    pub fn update(&mut self, now: Instant) -> InputUpdate {
        let mut phase = self.current();
        while now.duration_since(self.phase_started) >= phase.duration {
            self.phase_started += phase.duration;
            self.index = (self.index + 1) % self.phases.len();
            self.fired = false;
            phase = self.current();
        }

        // Steer the view towards the pitch of the phase. The script tracks the
        // pitch it asked for, so it can compute the delta without asking the
        // game thread.
        let delta = (phase.pitch - self.pitch).clamp(-0.1, 0.1);
        self.pitch = (self.pitch + delta).clamp(-1.5, 1.5);

        let mut update = InputUpdate {
            look_pitch: delta,
            ..InputUpdate::default()
        };
        let mut break_block = false;
        let mut place_block = false;
        match phase.step {
            ScriptStep::Idle => {}
            ScriptStep::Walk => update.state.forward = 1.0,
            ScriptStep::Sprint => {
                update.state.forward = 1.0;
                update.state.sprint = true;
            }
            ScriptStep::Sneak => {
                update.state.forward = 1.0;
                update.state.sneak = true;
            }
            ScriptStep::Jump => {
                update.state.forward = 1.0;
                if now.duration_since(self.last_jump) >= Duration::from_millis(500) {
                    self.last_jump = now;
                    update.state.jump = true;
                }
            }
            ScriptStep::LookDown | ScriptStep::LookUp => {}
            ScriptStep::Turn => {
                // A slow turn to the right: the script walks in a zig-zag and
                // does not run into the block it placed.
                update.look_yaw = TURN_RATE;
            }
            ScriptStep::LookAround => {
                let second = now.duration_since(self.phase_started).as_secs();
                update.look_yaw = if second.is_multiple_of(2) {
                    TURN_RATE
                } else {
                    -TURN_RATE
                };
            }
            ScriptStep::Break => {
                if !self.fired {
                    self.fired = true;
                    break_block = true;
                }
            }
            ScriptStep::Place => {
                if !self.fired {
                    self.fired = true;
                    place_block = true;
                }
            }
        }
        update.break_block = break_block;
        update.place_block = place_block;
        update
    }
}

/// The scripted session of the acceptance run.
///
/// The phases leave the spawn area first — the server protects the blocks
/// around the world spawn, so the dig and the placement only work further
/// away — then aim at the ground, break a block, walk over the drop the
/// server spawned (a survival player has an empty hand, so the placement
/// needs the block the break produced), place it again, and finally sprint
/// and jump.
///
/// The break and the placement run twice: the first placement can still fail
/// because the player has not picked the drop up yet, the second one always
/// has the block of the first break in its hand.
#[must_use]
pub fn default_script() -> Vec<ScriptPhase> {
    let mut phases = vec![ScriptPhase {
        step: ScriptStep::Walk,
        duration: Duration::from_secs(10),
        pitch: 0.0,
    }];
    for _ in 0..2 {
        phases.extend([
            ScriptPhase {
                step: ScriptStep::LookDown,
                duration: Duration::from_millis(700),
                pitch: 0.6,
            },
            ScriptPhase {
                step: ScriptStep::Break,
                duration: Duration::from_millis(1500),
                pitch: 0.6,
            },
            ScriptPhase {
                step: ScriptStep::LookUp,
                duration: Duration::from_millis(700),
                pitch: 0.0,
            },
            ScriptPhase {
                step: ScriptStep::Walk,
                duration: Duration::from_millis(3000),
                pitch: 0.0,
            },
            ScriptPhase {
                step: ScriptStep::LookDown,
                duration: Duration::from_millis(700),
                pitch: 0.6,
            },
            ScriptPhase {
                step: ScriptStep::Place,
                duration: Duration::from_millis(1500),
                pitch: 0.6,
            },
            ScriptPhase {
                step: ScriptStep::LookUp,
                duration: Duration::from_millis(700),
                pitch: 0.0,
            },
            ScriptPhase {
                step: ScriptStep::Turn,
                duration: Duration::from_millis(1500),
                pitch: 0.0,
            },
        ]);
    }
    phases.extend([
        ScriptPhase {
            step: ScriptStep::Sprint,
            duration: Duration::from_secs(4),
            pitch: 0.0,
        },
        ScriptPhase {
            step: ScriptStep::Jump,
            duration: Duration::from_secs(3),
            pitch: 0.0,
        },
        ScriptPhase {
            step: ScriptStep::Walk,
            duration: Duration::from_secs(3),
            pitch: 0.0,
        },
    ]);
    phases
}

/// The 60-second movement script of the parity harness (ROADMAP.md P0):
/// stand, walk, sneak, sprint, jump, look around, walk, stand.
///
/// The owner plays the same timeline by hand with the vanilla client
/// (`docs/parity-harness.md`), so the two serverbound traces can be compared
/// with `umicraft-trace-diff`. Changing a phase here means changing that
/// document too.
#[must_use]
pub fn movement_script() -> Vec<ScriptPhase> {
    let phase = |step, seconds, pitch| ScriptPhase {
        step,
        duration: Duration::from_secs(seconds),
        pitch,
    };
    vec![
        phase(ScriptStep::Idle, 5, 0.0),
        phase(ScriptStep::Walk, 8, 0.0),
        phase(ScriptStep::Sneak, 8, 0.0),
        phase(ScriptStep::Sprint, 8, 0.0),
        phase(ScriptStep::Jump, 8, 0.0),
        phase(ScriptStep::LookAround, 5, 0.5),
        phase(ScriptStep::LookAround, 5, -0.5),
        phase(ScriptStep::Walk, 8, 0.0),
        phase(ScriptStep::Idle, 5, 0.0),
    ]
}

/// The scripted session that only stands still.
///
/// It is used to measure the idle traffic of a session (the keepalives and
/// the twenty-tick position reports) without moving.
#[must_use]
pub fn idle_script() -> Vec<ScriptPhase> {
    vec![ScriptPhase {
        step: ScriptStep::Idle,
        duration: Duration::from_secs(3600),
        pitch: 0.0,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_phase_moves_forward() {
        let mut script = Script::new(default_script());
        let update = script.update(Instant::now());
        assert_eq!(update.state.forward, 1.0);
        assert!(!update.state.sprint);
    }

    #[test]
    fn the_sprint_phase_sprints_forward() {
        let mut script = Script::new(vec![ScriptPhase {
            step: ScriptStep::Sprint,
            duration: Duration::from_secs(10),
            pitch: 0.0,
        }]);
        let update = script.update(Instant::now());
        assert_eq!(update.state.forward, 1.0);
        assert!(update.state.sprint);
    }

    #[test]
    fn the_break_and_place_phases_fire_once() {
        let mut script = Script::new(default_script());
        let start = Instant::now();
        // The walk phase lasts ten seconds, the break phase starts at 10.7 s
        // and the placement phase at 15.4 s.
        let mut saw_break = false;
        let mut saw_place = false;
        for step in 0..200 {
            let update = script.update(start + Duration::from_millis(step * 100));
            saw_break |= update.break_block;
            saw_place |= update.place_block;
        }
        assert!(saw_break, "the break phase must request a dig");
        assert!(saw_place, "the place phase must request a placement");
    }

    #[test]
    fn the_movement_script_lasts_sixty_seconds() {
        let total: Duration = movement_script().iter().map(|phase| phase.duration).sum();
        assert_eq!(total, Duration::from_secs(60));
    }

    #[test]
    fn the_sneak_phase_sneaks_forward() {
        let mut script = Script::new(vec![ScriptPhase {
            step: ScriptStep::Sneak,
            duration: Duration::from_secs(10),
            pitch: 0.0,
        }]);
        let update = script.update(Instant::now());
        assert_eq!(update.state.forward, 1.0);
        assert!(update.state.sneak);
    }

    #[test]
    fn the_look_around_phase_turns_both_ways() {
        let mut script = Script::new(vec![ScriptPhase {
            step: ScriptStep::LookAround,
            duration: Duration::from_secs(10),
            pitch: 0.0,
        }]);
        let start = Instant::now();
        let right = script.update(start).look_yaw;
        let left = script.update(start + Duration::from_millis(1500)).look_yaw;
        assert!(right > 0.0 && left < 0.0, "{right} then {left}");
    }

    #[test]
    fn an_empty_script_falls_back_to_the_default() {
        let script = Script::new(Vec::new());
        assert_eq!(script.phases.len(), default_script().len());
    }

    #[test]
    fn the_pitch_is_steered_towards_the_phase() {
        let mut script = Script::new(vec![ScriptPhase {
            step: ScriptStep::LookDown,
            duration: Duration::from_secs(10),
            pitch: 0.6,
        }]);
        let start = Instant::now();
        let mut total = 0.0_f32;
        for step in 0..20 {
            let update = script.update(start + Duration::from_millis(step * 100));
            total += update.look_pitch;
        }
        assert!(total > 0.0, "the view must look down");
        assert!(total <= 0.6 + f32::EPSILON, "the view must not overshoot");
    }
}
