// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tick-driven parity scenario `e1` (phase E.2) and its state log.
//!
//! The scenario is the one of `tools/vanilla-agent/scenarios/e1.txt`, which
//! drives the vanilla 1.8.9 client through an agent: walk, sprint, jump,
//! sprint and jump, strafe left and right, sneak. It is counted in **client
//! ticks**, not in seconds, and it starts when the player has stood on the
//! ground for `SETTLE_TICKS` consecutive ticks, the same rule as the agent,
//! so the two clients start from the same state and a few ticks of delay at
//! the start cost nothing.
//!
//! The script only chooses which keys are down on each tick; it writes
//! nothing but the input state, never a position, a motion or a rotation.
//! Every tick it also appends one line of the player's internal state to the
//! state log (`umicraft_testkit::state_log` documents the format), read after
//! the physics step like the agent reads the end of `EntityPlayerSP.onUpdate`.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

use umicraft_game::Player;

/// Consecutive on-ground ticks before the scenario starts.
pub const SETTLE_TICKS: u32 = 40;

/// The keys held during a phase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Keys {
    /// Forward.
    pub w: bool,
    /// Strafe left.
    pub a: bool,
    /// Back.
    pub s: bool,
    /// Strafe right.
    pub d: bool,
    /// Jump.
    pub jump: bool,
    /// Sneak.
    pub sneak: bool,
    /// Sprint key.
    pub sprint: bool,
}

impl Keys {
    /// `W+SPRINT`, or `-` when nothing is held (the `keys_applied` column).
    #[must_use]
    pub fn applied(&self) -> String {
        let named = [
            (self.w, "W"),
            (self.a, "A"),
            (self.s, "S"),
            (self.d, "D"),
            (self.jump, "JUMP"),
            (self.sneak, "SNEAK"),
            (self.sprint, "SPRINT"),
        ];
        let held: Vec<&str> = named
            .iter()
            .filter_map(|(down, name)| down.then_some(*name))
            .collect();
        if held.is_empty() {
            "-".to_string()
        } else {
            held.join("+")
        }
    }

    /// The seven bindings as `0`/`1` in the order W A S D JUMP SNEAK SPRINT
    /// (the `keys_pressed_WASDJSC` column).
    #[must_use]
    pub fn pressed(&self) -> String {
        [self.w, self.a, self.s, self.d, self.jump, self.sneak, self.sprint]
            .iter()
            .map(|down| if *down { '1' } else { '0' })
            .collect()
    }
}

/// One phase of the scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickPhase {
    /// Phase name, as in the log and in `e1.txt`.
    pub name: &'static str,
    /// Length in client ticks.
    pub ticks: u32,
    /// Keys held for the whole phase.
    pub keys: Keys,
}

/// The scenario `e1`.
#[must_use]
pub fn e1_script() -> Vec<TickPhase> {
    let still = Keys::default();
    let w = Keys { w: true, ..still };
    let phase = |name, ticks, keys| TickPhase { name, ticks, keys };
    vec![
        phase("walk", 40, w),
        phase("stop1", 40, still),
        phase("sprint", 40, Keys { sprint: true, ..w }),
        phase("stop2", 40, still),
        phase("jump1", 1, Keys { jump: true, ..still }),
        phase("jump1_air", 19, still),
        phase("jump2", 1, Keys { jump: true, ..still }),
        phase("jump2_air", 19, still),
        phase("jump3", 1, Keys { jump: true, ..still }),
        phase("jump3_air", 19, still),
        phase("stop3", 20, still),
        phase(
            "sprint_jump",
            40,
            Keys {
                sprint: true,
                jump: true,
                ..w
            },
        ),
        phase("stop4", 40, still),
        phase("strafe_left", 40, Keys { a: true, ..still }),
        phase("strafe_right", 40, Keys { d: true, ..still }),
        phase("stop5", 20, still),
        phase("sneak_still", 20, Keys { sneak: true, ..still }),
        phase("sneak_walk", 40, Keys { sneak: true, ..w }),
        phase("stop6", 40, still),
    ]
}

/// The scenario `e2` (phase E.5): a sprint ended by sneaking with W still held.
/// Sneaking scales the forward input to 0.3, below the 0.8 sprint threshold
/// (`EntityPlayerSP.onLivingUpdate`), so the sprint ends on the first sneak tick.
#[must_use]
pub fn e2_script() -> Vec<TickPhase> {
    let still = Keys::default();
    let w = Keys { w: true, ..still };
    let phase = |name, ticks, keys| TickPhase { name, ticks, keys };
    vec![
        phase("sprint", 40, Keys { sprint: true, ..w }),
        phase("sneak_w", 20, Keys { sneak: true, ..w }),
        phase("stop", 20, still),
    ]
}

/// The scenario `e3` (phase E.5): walk and sprint through water in Depth
/// Strider III boots (the server gives the boots and builds the pool).
#[must_use]
pub fn e3_script() -> Vec<TickPhase> {
    let still = Keys::default();
    let w = Keys { w: true, ..still };
    let phase = |name, ticks, keys| TickPhase { name, ticks, keys };
    vec![
        phase("approach", 15, w),
        phase("swim_walk", 40, w),
        phase("swim_sprint", 40, Keys { sprint: true, ..w }),
        phase("swim_stop", 20, still),
    ]
}

/// The scenario `e4` (phase E.7): the same walk and sprint as `e3`, in a deeper
/// pool, with the jump key held so the swimmer floats (`onGround` false).
#[must_use]
pub fn e4_script() -> Vec<TickPhase> {
    let still = Keys::default();
    let wj = Keys {
        w: true,
        jump: true,
        ..still
    };
    let phase = |name, ticks, keys| TickPhase { name, ticks, keys };
    vec![
        phase("approach", 15, Keys { w: true, ..still }),
        phase("surface_walk", 40, wj),
        phase("surface_sprint", 40, Keys { sprint: true, ..wj }),
        phase("surface_stop", 20, Keys { jump: true, ..still }),
    ]
}

/// The scenario called `name` (`e1` to `e4`).
#[must_use]
pub fn scenario(name: &str) -> Option<Vec<TickPhase>> {
    match name {
        "e1" => Some(e1_script()),
        "e2" => Some(e2_script()),
        "e3" => Some(e3_script()),
        "e4" => Some(e4_script()),
        _ => None,
    }
}

/// The state log header (see `umicraft_testkit::state_log::HEADER`).
const HEADER: &str = "tick,script_tick,phase,x,y,z,mx,my,mz,yaw,pitch,on_ground,sprinting,sneaking,keys_applied,keys_pressed_WASDJSC";

/// A running scenario with its log.
pub struct E1Run {
    phases: Vec<TickPhase>,
    log: BufWriter<File>,
    /// Client ticks seen so far, minus one.
    tick: i64,
    ground_run: u32,
    start: Option<i64>,
    /// Keys applied by the current tick.
    keys: Keys,
}

impl E1Run {
    /// Opens the state log at `path`.
    ///
    /// # Errors
    /// The file could not be created.
    pub fn open(path: &Path, name: &str) -> std::io::Result<Self> {
        let phases = scenario(name).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("unknown scenario {name}"))
        })?;
        let mut log = BufWriter::new(File::create(path)?);
        let total: u32 = phases.iter().map(|phase| phase.ticks).sum();
        writeln!(
            log,
            "# umicraft e1 state log; scenario ticks={total} settle={SETTLE_TICKS}"
        )?;
        writeln!(log, "{HEADER}")?;
        Ok(Self {
            phases,
            log,
            tick: -1,
            ground_run: 0,
            start: None,
            keys: Keys::default(),
        })
    }

    fn phase_at(&self, script_tick: i64) -> Option<&TickPhase> {
        let mut from = 0_i64;
        self.phases.iter().find(|phase| {
            let end = from + i64::from(phase.ticks);
            let inside = script_tick < end;
            from = end;
            inside
        })
    }

    /// Starts a tick: returns the keys to hold, given whether the player
    /// stands on the ground (its state at the end of the previous tick).
    pub fn before(&mut self, on_ground: bool) -> Keys {
        self.tick += 1;
        self.ground_run = if on_ground { self.ground_run + 1 } else { 0 };
        if self.start.is_none() && self.ground_run >= SETTLE_TICKS {
            self.start = Some(self.tick);
        }
        self.keys = self
            .start
            .and_then(|start| self.phase_at(self.tick - start))
            .map_or_else(Keys::default, |phase| phase.keys);
        self.keys
    }

    /// Ends a tick: appends the line of the player's state after the physics.
    pub fn after(&mut self, player: &Player) {
        let script_tick = self.start.map_or(-1, |start| self.tick - start);
        let name = match (script_tick, self.phase_at(script_tick)) {
            (tick, _) if tick < 0 => "pre",
            (_, Some(phase)) => phase.name,
            (_, None) => "end",
        };
        let line = format!(
            "{},{script_tick},{name},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.tick,
            player.position.x,
            player.position.y,
            player.position.z,
            player.velocity.x,
            player.velocity.y,
            player.velocity.z,
            player.yaw.to_degrees(),
            player.pitch.to_degrees(),
            player.on_ground,
            player.sprinting,
            player.sneaking,
            self.keys.applied(),
            self.keys.pressed(),
        );
        // A failed write only loses log lines, never the session.
        let _ = writeln!(self.log, "{line}");
        if self.tick % 20 == 0 {
            let _ = self.log.flush();
        }
    }

    /// Flushes the log.
    pub fn finish(&mut self) {
        let _ = self.log.flush();
    }
}

/// The run waiting for the game thread to pick it up.
///
/// The game thread is started by `Session::start`, whose configuration is
/// shared with the windowed client; handing the run over through this slot
/// leaves that configuration alone.
static PENDING: Mutex<Option<E1Run>> = Mutex::new(None);

/// Hands a run to the next game thread.
pub fn install(run: E1Run) {
    if let Ok(mut slot) = PENDING.lock() {
        *slot = Some(run);
    }
}

/// Takes the installed run, if any.
#[must_use]
pub fn take() -> Option<E1Run> {
    PENDING.lock().ok().and_then(|mut slot| slot.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent file and the Rust table must describe the same scenario.
    #[test]
    fn the_rust_tables_equal_the_agent_scenario_files() {
        for name in ["e1", "e2", "e3", "e4"] {
            check_against_file(name);
        }
    }

    fn check_against_file(name: &str) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tools/vanilla-agent/scenarios/{name}.txt"));
        let text = std::fs::read_to_string(&path).expect("scenario file");
        let mut parsed = Vec::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words[0] {
                "settle" => assert_eq!(words[1].parse::<u32>().unwrap(), SETTLE_TICKS),
                "exit_after" => {}
                "phase" => {
                    let mut keys = Keys::default();
                    for key in &words[3..] {
                        match *key {
                            "W" => keys.w = true,
                            "A" => keys.a = true,
                            "S" => keys.s = true,
                            "D" => keys.d = true,
                            "JUMP" => keys.jump = true,
                            "SNEAK" => keys.sneak = true,
                            "SPRINT" => keys.sprint = true,
                            "-" => {}
                            other => panic!("unknown key {other}"),
                        }
                    }
                    parsed.push((words[1].to_string(), words[2].parse::<u32>().unwrap(), keys));
                }
                other => panic!("unknown directive {other}"),
            }
        }
        let rust: Vec<(String, u32, Keys)> = scenario(name)
            .expect("known scenario")
            .into_iter()
            .map(|p| (p.name.to_string(), p.ticks, p.keys))
            .collect();
        assert_eq!(parsed, rust);
    }

    fn run() -> E1Run {
        let path = std::env::temp_dir().join(format!("umicraft-e1-test-{}.csv", std::process::id()));
        E1Run::open(&path, "e1").expect("log file")
    }

    #[test]
    fn the_script_waits_for_the_player_to_settle_then_follows_the_phases() {
        let mut run = run();
        // Airborne ticks never start the script.
        for _ in 0..10 {
            assert_eq!(run.before(false), Keys::default());
        }
        // Forty ticks on the ground: the script starts on the fortieth.
        for _ in 0..39 {
            assert_eq!(run.before(true), Keys::default());
        }
        let first = run.before(true);
        assert!(first.w && !first.sprint, "the first phase is a walk: {first:?}");
        for _ in 0..39 {
            assert!(run.before(true).w);
        }
        assert_eq!(run.before(true), Keys::default(), "stop1 releases everything");
    }

    #[test]
    fn a_tick_in_the_air_before_the_start_resets_the_settle_count() {
        let mut run = run();
        for _ in 0..39 {
            run.before(true);
        }
        run.before(false);
        for _ in 0..39 {
            assert_eq!(run.before(true), Keys::default());
        }
        assert!(run.before(true).w);
    }

    #[test]
    fn the_script_is_over_after_its_last_phase() {
        let total: u32 = e1_script().iter().map(|p| p.ticks).sum();
        let mut run = run();
        for _ in 0..SETTLE_TICKS {
            run.before(true);
        }
        for _ in 1..total {
            run.before(true);
        }
        assert_eq!(run.before(true), Keys::default());
        assert!(run.phase_at(i64::from(total)).is_none());
        assert_eq!(total, 520);
    }

    #[test]
    fn the_key_columns_follow_the_agent_order() {
        let keys = Keys {
            w: true,
            sprint: true,
            jump: true,
            ..Keys::default()
        };
        assert_eq!(keys.applied(), "W+JUMP+SPRINT");
        assert_eq!(keys.pressed(), "1000101");
        assert_eq!(Keys::default().applied(), "-");
    }
}
