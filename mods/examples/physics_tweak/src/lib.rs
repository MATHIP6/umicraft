// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: physics tweak.
//!
//! Shows the two services of API 0.5: wrapping the player's movement step and
//! asking the world questions. The wrapper only measures (horizontal speed and
//! the height of the last jump) and never writes to the state, so the movement
//! stays bit for bit vanilla: an official example must not give an advantage
//! (spec §10). Wrapping still needs `movement = true` and
//! `wraps = ["player_physics"]` in `mod.toml`; without them the host refuses
//! the layer, and the client turns the mod off in server-compatible mode. See
//! `mods/examples/README.md` for how to build and install it.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umicraft_modapi::{
    umicraft_mod, Anchor, Block, InitCx, Mod, ModError, Placement, PostTick, World,
};

/// Priority of the `PostTick` handler; lower values run first.
const POST_TICK_PRIORITY: i32 = 0;

/// The feet sit exactly on the block's top face when standing, so look a
/// little below them to find that block.
const BELOW_FEET: f64 = 0.01;

/// HUD text colour.
const WIDGET_COLOR: u32 = 0x00FF_FFFF;

/// Game ticks per second, to turn a distance per tick into blocks per second.
const TICKS_PER_SECOND: f64 = 20.0;

/// Speed and jump height, measured from the position after each step.
#[derive(Debug, Default)]
struct Motion {
    /// Feet position after the previous step; `None` before the first.
    position: Option<[f64; 3]>,
    /// Whether the player stood on the ground after the previous step.
    on_ground: bool,
    /// Horizontal speed over the last step, in blocks per second.
    speed: f64,
    /// Height of the jump in progress: take-off y and the peak so far.
    airborne: Option<Airborne>,
    /// Height of the last finished jump, in blocks; `None` before the first.
    last_jump: Option<f64>,
}

/// A jump from the ground, from take-off to landing.
#[derive(Debug, Clone, Copy)]
struct Airborne {
    takeoff_y: f64,
    peak_y: f64,
}

impl Motion {
    /// Takes the state after one step: its feet position and `on_ground`.
    fn record(&mut self, position: [f64; 3], on_ground: bool) {
        if let Some(previous) = self.position {
            let (dx, dz) = (position[0] - previous[0], position[2] - previous[2]);
            self.speed = dx.hypot(dz) * TICKS_PER_SECOND;
            if self.on_ground && !on_ground {
                // The feet left the ground: the take-off height is where they
                // stood after the previous step.
                self.airborne = Some(Airborne {
                    takeoff_y: previous[1],
                    peak_y: previous[1],
                });
            }
        }
        if let Some(jump) = &mut self.airborne {
            jump.peak_y = jump.peak_y.max(position[1]);
            if on_ground {
                // Walking off a ledge never rises above the take-off: not a jump.
                let height = jump.peak_y - jump.takeoff_y;
                self.last_jump = (height > 0.0).then_some(height).or(self.last_jump);
                self.airborne = None;
            }
        }
        self.position = Some(position);
        self.on_ground = on_ground;
    }
}

/// What the mod last learned, shared by the physics layer, the tick handler
/// and the HUD widget.
#[derive(Debug, Default)]
struct Seen {
    /// What the physics layer measured.
    motion: Motion,
    /// The block under the feet; `None` when unknown (unloaded chunk).
    block: Option<Block>,
    /// Name and distance of the nearest other player.
    nearest: Option<(String, f64)>,
}

/// The shared state, tolerant of a panic in another handler: the values are
/// plain data, and a HUD must not stop drawing because of one.
#[derive(Debug, Default, Clone)]
struct Shared(Arc<Mutex<Seen>>);

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Seen> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The mod has no state of its own: everything lives in the closures.
struct PhysicsTweak;

impl Mod for PhysicsTweak {
    /// Runs once when the client loads the library: register the layer, the
    /// tick handler and the widget.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let shared = Shared::default();

        // A wrapper runs around the vanilla step. It is called on the game
        // thread, within a time budget, and again for past ticks when the
        // server corrects the position, so it must be a function of the state
        // and input only.
        let seen = shared.clone();
        cx.wrap_physics(move |physics| {
            // Run the vanilla step first; the state now holds this tick's
            // result. It is only read: nothing is written back.
            physics.next()?;
            let state = physics.state();
            seen.lock()
                .motion
                .record(state.position(), state.on_ground());
            Ok(())
        })?;

        // World queries only answer on the game thread inside a callback. The
        // physics layer is one; an intercept handler on a game event is
        // another (an observer is not, and neither is the HUD), so the
        // queries run here and the widget only reads what they stored.
        let world = cx.world();
        let seen = shared.clone();
        cx.intercept::<PostTick>(POST_TICK_PRIORITY, move |_event, _control| {
            let Some(position) = seen.lock().motion.position else {
                return;
            };
            let block = block_under(world, position);
            let nearest = nearest_player(world, position);
            let mut seen = seen.lock();
            seen.block = block;
            seen.nearest = nearest;
        })?;

        let placement = Placement::new(Anchor::TopLeft).offset(4.0, 80.0);
        cx.hud_widget("physics", "Physics tweak", placement, move |hud| {
            let seen = shared.lock();
            hud.line(&speed_line(seen.motion.speed), WIDGET_COLOR);
            hud.line(&jump_line(seen.motion.last_jump), WIDGET_COLOR);
            hud.line(&block_line(seen.block), WIDGET_COLOR);
            hud.line(&nearest_line(seen.nearest.as_ref()), WIDGET_COLOR);
        })?;
        Ok(Self)
    }
}

/// The block just below `position`, or `None` when its chunk is not loaded
/// (or the host refuses, which only happens off the game thread).
fn block_under(world: World, position: [f64; 3]) -> Option<Block> {
    let [x, z] = [position[0], position[2]].map(|axis| axis.floor() as i32);
    let y = (position[1] - BELOW_FEET).floor() as i32;
    world.block(x, y, z).ok()
}

/// The closest entity of kind `player` and its distance, in blocks.
fn nearest_player(world: World, position: [f64; 3]) -> Option<(String, f64)> {
    let mut nearest: Option<(String, f64)> = None;
    let visited = world.entities(|entity| {
        if entity.kind() == "player" {
            let distance = distance(position, entity.position());
            if nearest.as_ref().is_none_or(|(_, best)| distance < *best) {
                nearest = Some((entity.name().to_owned(), distance));
            }
        }
        true
    });
    // A refusal means "no answer": show no player rather than a stale one.
    visited.ok().and(nearest)
}

/// Euclidean distance between two positions.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn speed_line(speed: f64) -> String {
    format!("speed: {speed:.1} blocks/s")
}

fn jump_line(height: Option<f64>) -> String {
    match height {
        Some(height) => format!("last jump: {height:.2} blocks"),
        None => "last jump: none".to_owned(),
    }
}

fn block_line(block: Option<Block>) -> String {
    match block {
        Some(Block { id, meta }) => format!("under you: {id}:{meta}"),
        None => "under you: unknown".to_owned(),
    }
}

fn nearest_line(nearest: Option<&(String, f64)>) -> String {
    match nearest {
        Some((name, distance)) => format!("nearest player: {name} ({distance:.1} m)"),
        None => "nearest player: none".to_owned(),
    }
}

umicraft_mod!(PhysicsTweak);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_euclidean() {
        assert!((distance([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-12);
    }

    /// Feeds `(x, y, z, on_ground)` steps into a fresh `Motion`.
    fn run(steps: &[([f64; 3], bool)]) -> Motion {
        let mut motion = Motion::default();
        for &(position, on_ground) in steps {
            motion.record(position, on_ground);
        }
        motion
    }

    #[test]
    fn speed_is_the_horizontal_distance_per_tick_times_twenty() {
        let motion = run(&[([0.0, 64.0, 0.0], true), ([0.2, 64.0, 0.0], true)]);
        assert!((motion.speed - 4.0).abs() < 1e-9);
        // The vertical part does not count.
        let motion = run(&[([0.0, 64.0, 0.0], false), ([0.0, 63.0, 0.0], false)]);
        assert!(motion.speed.abs() < 1e-12);
    }

    #[test]
    fn a_jump_is_the_peak_above_the_take_off() {
        let motion = run(&[
            ([0.0, 64.0, 0.0], true),
            ([0.0, 64.42, 0.0], false),
            ([0.0, 65.0, 0.0], false),
            ([0.0, 64.8, 0.0], false),
            ([0.0, 64.0, 0.0], true),
        ]);
        assert!((motion.last_jump.expect("a jump") - 1.0).abs() < 1e-9);
        assert!(motion.airborne.is_none());
    }

    #[test]
    fn a_fall_from_a_ledge_is_not_a_jump() {
        let motion = run(&[
            ([0.0, 64.0, 0.0], true),
            ([0.1, 63.9, 0.0], false),
            ([0.2, 60.0, 0.0], true),
        ]);
        assert_eq!(motion.last_jump, None);
    }

    #[test]
    fn a_later_fall_keeps_the_previous_jump_height() {
        let motion = run(&[
            ([0.0, 64.0, 0.0], true),
            ([0.0, 65.0, 0.0], false),
            ([0.0, 64.0, 0.0], true),
            ([0.1, 63.9, 0.0], false),
            ([0.2, 60.0, 0.0], true),
        ]);
        assert!((motion.last_jump.expect("a jump") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn hud_lines_cover_known_and_unknown() {
        assert_eq!(block_line(Some(Block { id: 2, meta: 1 })), "under you: 2:1");
        assert_eq!(block_line(None), "under you: unknown");
        assert_eq!(
            nearest_line(Some(&("Ann".to_owned(), 3.04))),
            "nearest player: Ann (3.0 m)"
        );
        assert_eq!(nearest_line(None), "nearest player: none");
        assert_eq!(speed_line(4.317), "speed: 4.3 blocks/s");
        assert_eq!(jump_line(Some(1.2519)), "last jump: 1.25 blocks");
        assert_eq!(jump_line(None), "last jump: none");
    }
}
