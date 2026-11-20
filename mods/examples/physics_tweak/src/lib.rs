// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: physics tweak.
//!
//! Shows the two services of API 0.5: wrapping the player's movement step and
//! asking the world questions. It needs `movement = true` and
//! `wraps = ["player_physics"]` in `mod.toml`; without them the host refuses
//! the layer. Because it alters movement, the client turns it off in
//! server-compatible mode. See `mods/examples/README.md` for how to build and
//! install it.

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

/// What the mod last learned, shared by the physics layer, the tick handler
/// and the HUD widget.
#[derive(Debug, Default)]
struct Seen {
    /// Feet position the physics layer last saw; `None` before the first tick.
    position: Option<[f64; 3]>,
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
        let fall_speed = cx.slider("fall_speed", "Fall speed", 0.5, 1.0, 0.05, 1.0)?;

        // A wrapper runs around the vanilla step. It is called on the game
        // thread, within a time budget, and again for past ticks when the
        // server corrects the position, so it must be a function of the state
        // and input only (reading a setting is fine).
        let seen = shared.clone();
        cx.wrap_physics(move |physics| {
            // Run the vanilla step first; the state now holds this tick's
            // result, which is what we adjust.
            physics.next()?;

            let scale = f64::from(fall_speed.get());
            let [x, y, z] = physics.state().velocity();
            // At the default (1.0) nothing is written, so the movement stays
            // bit for bit vanilla.
            if scale < 1.0 && y < 0.0 && !physics.state().on_ground() {
                physics.state_mut().set_velocity([x, y * scale, z]);
            }
            seen.lock().position = Some(physics.state().position());
            Ok(())
        })?;

        // World queries only answer on the game thread inside a callback. The
        // physics layer is one; an intercept handler on a game event is
        // another (an observer is not, and neither is the HUD), so the
        // queries run here and the widget only reads what they stored.
        let world = cx.world();
        let seen = shared.clone();
        cx.intercept::<PostTick>(POST_TICK_PRIORITY, move |_event, _control| {
            let Some(position) = seen.lock().position else {
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

    #[test]
    fn hud_lines_cover_known_and_unknown() {
        assert_eq!(block_line(Some(Block { id: 2, meta: 1 })), "under you: 2:1");
        assert_eq!(block_line(None), "under you: unknown");
        assert_eq!(
            nearest_line(Some(&("Ann".to_owned(), 3.04))),
            "nearest player: Ann (3.0 m)"
        );
        assert_eq!(nearest_line(None), "nearest player: none");
    }
}
