// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hand and crosshair facts of the `INPUT` mod event (API 1.5, ADR-0098), taken
//! by the game thread at each tick and published with the frame.

use umicraft_game::combat::Pointed;
use umicraft_game::interaction::wire_face;
use umicraft_game::item_use::{is_block_item, use_action, UseAction};
use umicraft_modhost::{HandFacts, HeldItem, PointedFacts, UseKind};

use super::combat;
use super::state::Game;

/// The hand and the crosshair of the local player at this tick.
pub(super) fn hand_facts(game: &Game) -> HandFacts {
    let held = game
        .windows
        .selected_item()
        .filter(|stack| stack.item_id >= 0);
    HandFacts {
        tick: game.status_data.ticks,
        swing_progress: game.hand.swing_progress(1.0),
        using_item: game.item_use.is_using(),
        held: held.map(|stack| HeldItem {
            id: i32::from(stack.item_id),
            damage: i32::from(stack.damage),
        }),
        held_use: held.and_then(use_action).map_or(UseKind::None, use_kind),
        held_is_block: held.is_some_and(is_block_item),
        pointed: pointed_facts(game, combat::pointed(game)),
    }
}

/// `pointed` in the terms of the event: the block's wire state, the face as 1.8
/// numbers it and the distance from the eye.
fn pointed_facts(game: &Game, pointed: Pointed) -> PointedFacts {
    let eye = game.eye().unwrap_or_default();
    match pointed {
        Pointed::Miss => PointedFacts::Miss,
        Pointed::Block(hit) => {
            let state = game.world.as_ref().map_or(0, |world| {
                let block = world.block(hit.block);
                u32::from(block.id()) << 4 | u32::from(block.metadata())
            });
            PointedFacts::Block {
                position: [hit.block.x, hit.block.y, hit.block.z],
                face: wire_face(hit.face).code(),
                state,
                point: hit.position.to_array(),
                distance: hit.distance as f32,
            }
        }
        Pointed::Entity(hit) => PointedFacts::Entity {
            id: hit.id.0,
            point: hit.point.to_array(),
            distance: hit.point.distance(eye) as f32,
        },
    }
}

fn use_kind(action: UseAction) -> UseKind {
    match action {
        UseAction::Eat => UseKind::Eat,
        UseAction::Drink => UseKind::Drink,
        UseAction::Block => UseKind::Block,
        UseAction::Bow => UseKind::Bow,
    }
}
