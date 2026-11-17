// SPDX-License-Identifier: MIT OR Apache-2.0

//! The use button: placing, using the held item and opening doors.

use umicraft_game::combat::ClickViewer;
use umicraft_game::editors::book::{WRITABLE_BOOK, WRITTEN_BOOK};
use umicraft_game::entity::EntityId;
use umicraft_game::interaction::Outgoing;
use umicraft_game::item_use::{DoorToggle, UseTick};

use super::swing_effects;
use crate::game::combat;
use crate::game::commands::{command_of, CommandQueue};
use crate::game::state::Game;
use crate::network::ClientCommand;

/// The arrow item id (MCP `Items.arrow`): a bow needs one to be drawn.
const ARROW: i16 = 262;

/// Runs one tick of the use button (`umicraft_game::item_use`): the click on
/// a block or in the air, the held button and the release.
pub(in crate::game) fn run_use(game: &mut Game, queue: &mut CommandQueue) {
    let pressed = std::mem::take(&mut game.controls.use_pressed);
    if game.dead() {
        return;
    }
    let pointed = combat::pointed(game);
    let target = pointed.block();
    let entity = pointed
        .entity()
        .and_then(|hit| Some((game.entities.entities().get(&hit.id)?, hit)));
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let item = game.windows.selected_item().cloned();
    let has_arrow = game
        .windows
        .main_inventory()
        .iter()
        .flatten()
        .any(|stack| stack.item_id == ARROW);
    let player = state.player();
    let output = game.item_use.tick(
        &mut game.interaction,
        &UseTick {
            mode: game.gamemode,
            pressed,
            held: game.controls.input.use_item,
            target,
            entity,
            world,
            player: Some(player.bounding_box()),
            sneaking: player.sneaking,
            slot: game.hotbar.selected,
            item: item.as_ref(),
            food: game.vitals.food as i32,
            has_arrow,
            viewer: ClickViewer {
                id: EntityId(game.entity_id),
                uuid: game.body.uuid.map(u128::from_be_bytes),
            },
        },
    );
    for outgoing in output.packets {
        if let Outgoing::UseItem { item } = &outgoing {
            // MCP `ItemWritableBook` / `ItemEditableBook.onItemRightClick`
            // open the book screen when the item is used.
            if matches!(item.item_id, WRITABLE_BOOK | WRITTEN_BOOK) {
                game.pending_book = Some(item.clone());
            }
        }
        queue.push(command_of(outgoing));
    }
    if let Some(door) = output.door {
        swing_door(game, door);
    }
    if output.swing {
        // MCP `Minecraft.rightClickMouse`: a used item swings the arm.
        queue.push(ClientCommand::Swing);
        let effects = swing_effects(game);
        game.hand.swing(effects);
        game.body.animation.swing_item();
    }
}

/// MCP `BlockDoor.onBlockActivated` runs on the client too: the door swings at
/// once and the player hears it. The server sends that sound to everyone but
/// the player who clicked (`playAuxSFXAtEntity`), so the clicking client has
/// to make it: it takes the same path as the `Effect` packet the others get.
fn swing_door(game: &mut Game, door: DoorToggle) {
    if let Some(world) = game.world.as_mut() {
        game.dirty
            .extend(world.apply_block_change(door.lower, door.state));
    }
    let Some(state) = game.state.as_mut() else {
        return;
    };
    // 1003 opens, 1006 closes (`RenderGlobal.playAuxSFX`).
    state.on_effect(&umicraft_protocol::packets::play::clientbound::Effect {
        effect_id: if door.opens() { 1003 } else { 1006 },
        x: door.clicked.x,
        y: door.clicked.y,
        z: door.clicked.z,
        data: 0,
        disable_relative_volume: false,
    });
}
