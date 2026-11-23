// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: entity list (API 1.1).
//!
//! Shows the render-thread entity snapshot: a `Frame` intercept remembers where the
//! camera is, and the widget draws the nearest entities with `World::snapshot`,
//! which only answers inside a render-thread callback such as a widget draw. Each
//! row has the entity's name (or its kind), its distance from the camera and, for a
//! living one, a health bar. The list is a debugging view, as the owner decided in
//! ADR-0094; it shows nothing the player cannot see. See `mods/examples/README.md`
//! for how to build and install it.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umicraft_modapi::{
    umicraft_mod, Anchor, Entity, Frame, Hud, InitCx, Mod, ModError, Placement, World,
};

/// How many entities the widget lists.
const ROWS: usize = 10;
/// Height of one row, in GUI pixels: a line of text and the bar under it.
const ROW_HEIGHT: f32 = 14.0;
/// Where the bar sits under the row's text, and its size.
const BAR_TOP: f32 = 9.0;
const BAR_HEIGHT: f32 = 2.0;
/// Width of the widget and of its bars; a label is centred on it.
const WIDGET_WIDTH: f32 = 120.0;
/// Text colour.
const TEXT_COLOR: u32 = 0x00FF_FFFF;
/// The bar's empty part.
const BAR_BACK_COLOR: u32 = 0x0055_5555;

/// The camera eye of the last frame, which the distances are measured from.
#[derive(Debug, Default, Clone)]
struct Eye(Arc<Mutex<[f64; 3]>>);

impl Eye {
    fn lock(&self) -> MutexGuard<'_, [f64; 3]> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Everything lives in the closures.
struct EntityList;

impl Mod for EntityList {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let eye = Eye::default();
        let world = cx.world();

        // The `Frame` intercept runs on the render thread right before the
        // widgets are drawn, so the eye it stores is the one of this frame.
        let remembered = eye.clone();
        cx.intercept::<Frame>(0, move |frame, _control| {
            *remembered.lock() = frame.eye();
        })?;

        let placement = Placement::new(Anchor::TopRight).offset(4.0, 4.0);
        cx.hud_widget("list", "Entity list", placement, move |hud| {
            draw(hud, world, *eye.lock());
        })?;
        Ok(Self)
    }
}

/// Draws up to [`ROWS`] rows of the snapshot, nearest first.
fn draw(hud: &mut Hud<'_>, world: World, eye: [f64; 3]) {
    let mut row = 0;
    let read = world.snapshot(|entity| {
        draw_row(hud, row, entity, eye);
        row += 1;
        row < ROWS
    });
    // Outside a world (or while the mod is off) the list is empty, not an error.
    if read.is_ok() {
        hud.reserve(WIDGET_WIDTH, ROW_HEIGHT * row as f32);
    }
}

fn draw_row(hud: &mut Hud<'_>, row: usize, entity: &Entity<'_>, eye: [f64; 3]) {
    let y = ROW_HEIGHT * row as f32;
    let blocks = distance(eye, entity.render_position());
    let text = row_text(entity.name(), entity.kind(), blocks);
    // A label is centred on its x.
    hud.label(WIDGET_WIDTH / 2.0, y, &text, TEXT_COLOR, 1.0, 1.0);
    if entity.is_living() {
        let fill = health_fraction(entity.health(), entity.max_health());
        hud.tick(
            0.0,
            y + BAR_TOP,
            WIDGET_WIDTH,
            BAR_HEIGHT,
            BAR_BACK_COLOR,
            1.0,
        );
        hud.tick(
            0.0,
            y + BAR_TOP,
            WIDGET_WIDTH * fill,
            BAR_HEIGHT,
            bar_color(fill),
            1.0,
        );
    }
}

/// Euclidean distance between two points, in blocks.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// "Name 3.2m": the name, or the kind when there is none, and the distance.
fn row_text(name: &str, kind: &str, blocks: f64) -> String {
    let label = if name.is_empty() { kind } else { name };
    format!("{label} {blocks:.1}m")
}

/// The share of health left, `0.0..=1.0`; an entity with no maximum shows empty.
fn health_fraction(health: f32, max_health: f32) -> f32 {
    if max_health > 0.0 {
        (health / max_health).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Green when healthy, orange when hurt, red when nearly dead.
fn bar_color(fraction: f32) -> u32 {
    if fraction > 0.6 {
        0x0055_FF55
    } else if fraction > 0.3 {
        0x00FF_AA00
    } else {
        0x00FF_5555
    }
}

umicraft_mod!(EntityList);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_shows_the_name_or_else_the_kind() {
        assert_eq!(row_text("Ann", "player", 3.04), "Ann 3.0m");
        assert_eq!(row_text("", "zombie", 12.0), "zombie 12.0m");
    }

    #[test]
    fn the_fraction_is_clamped_and_empty_without_a_maximum() {
        assert_eq!(health_fraction(10.0, 20.0), 0.5);
        assert_eq!(health_fraction(30.0, 20.0), 1.0);
        assert_eq!(health_fraction(-1.0, 20.0), 0.0);
        assert_eq!(health_fraction(5.0, 0.0), 0.0);
    }

    #[test]
    fn the_bar_changes_colour_as_health_drops() {
        let colors = [1.0, 0.5, 0.1].map(bar_color);
        assert_eq!(colors, [0x0055_FF55, 0x00FF_AA00, 0x00FF_5555]);
    }

    #[test]
    fn distance_is_euclidean() {
        assert!((distance([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-12);
    }
}
