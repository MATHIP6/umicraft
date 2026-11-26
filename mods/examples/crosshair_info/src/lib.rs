// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: crosshair info (API 1.5).
//!
//! Shows how to read the `INPUT` event: a [`PlayerInput`] intercept (render
//! thread, right after `Frame` and before the widgets are drawn) keeps the lines
//! to show, and a `TopLeft` widget draws them in the same frame. It needs
//! `input = true` in `mod.toml` to receive the event, yet it never presses a
//! binding nor turns the look: it tells the player what the crosshair already
//! shows them (spec §10).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umicraft_modapi::{
    umicraft_mod, Anchor, InitCx, Mod, ModError, Placement, PlayerInput, Pointed, UseAction,
};

/// Colour of the lines.
const COLOR: u32 = 0x00E0_E0E0;
/// The names of the faces, in the order of their wire numbers.
const FACES: [&str; 6] = ["bottom", "top", "north", "south", "west", "east"];

/// The line naming what the crosshair points at.
fn pointed_line(pointed: Pointed) -> String {
    match pointed {
        Pointed::Miss => "Nothing in reach".to_owned(),
        Pointed::Block {
            position: [x, y, z],
            face,
            state,
            distance,
            ..
        } => {
            let face = FACES.get(usize::from(face)).copied().unwrap_or("?");
            format!(
                "Block {}:{} at {x} {y} {z}, {face} face, {distance:.1} m",
                state >> 4,
                state & 0xF
            )
        }
        Pointed::Entity { id, distance, .. } => format!("Entity #{id}, {distance:.1} m"),
    }
}

/// The line about the held item, `None` for an empty hand.
fn held_line(input: &PlayerInput) -> Option<String> {
    let (id, damage) = input.held_item()?;
    let using = match input.held_use_action() {
        UseAction::None if input.held_is_block() => "places a block",
        UseAction::None => "no lasting use",
        UseAction::Eat => "eaten",
        UseAction::Drink => "drunk",
        UseAction::Block => "blocks",
        UseAction::Bow => "drawn",
    };
    let state = if input.using_item() { ", in use" } else { "" };
    Some(format!("Holding {id}:{damage} ({using}{state})"))
}

/// The lines of the last frame; a panic elsewhere leaves plain text behind.
#[derive(Debug, Default, Clone)]
struct Lines(Arc<Mutex<Vec<String>>>);

impl Lines {
    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Everything lives in the closures.
struct CrosshairInfo;

impl Mod for CrosshairInfo {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let lines = Lines::default();

        let shown = lines.clone();
        cx.intercept::<PlayerInput>(0, move |input, _control| {
            let mut shown = shown.lock();
            shown.clear();
            shown.push(pointed_line(input.pointed()));
            shown.extend(held_line(input));
        })?;

        cx.hud_widget(
            "crosshair",
            "Crosshair info",
            Placement::new(Anchor::TopLeft).offset(2.0, 2.0),
            move |hud| {
                for line in lines.lock().iter() {
                    hud.line(line, COLOR);
                }
            },
        )?;
        Ok(Self)
    }
}

umicraft_mod!(CrosshairInfo);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_line_names_the_state_position_face_and_distance() {
        let block = Pointed::Block {
            position: [1, 63, -2],
            face: 1,
            state: 2 << 4,
            point: [1.5, 64.0, -1.5],
            distance: 2.04,
        };
        assert_eq!(pointed_line(block), "Block 2:0 at 1 63 -2, top face, 2.0 m");
    }

    #[test]
    fn an_entity_and_a_miss_have_their_lines() {
        let entity = Pointed::Entity {
            id: 7,
            point: [0.0; 3],
            distance: 3.0,
        };
        assert_eq!(pointed_line(entity), "Entity #7, 3.0 m");
        assert_eq!(pointed_line(Pointed::Miss), "Nothing in reach");
    }
}
