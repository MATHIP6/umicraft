// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packets about windows and slots.

use std::sync::mpsc::SyncSender;

use umicraft_game::inventory::WindowKind;
use umicraft_protocol::packets::play::clientbound::{
    ConfirmTransaction, OpenWindow, SetSlot, WindowItems, WindowProperty,
};

use crate::game::commands::CommandQueue;
use crate::game::messages::RenderEvent;
use crate::game::publish::publish_inventory;
use crate::game::state::Game;
use crate::network::ClientCommand;

/// Recomputes the armour bar from the armour slots of the inventory.
pub(in crate::game) fn sync_armor(game: &mut Game) {
    game.armor = f32::from(game.windows.armor_value());
    game.hud_dirty = true;
}

/// Answers a `Confirm Transaction` the server did not accept.
///
/// Vanilla answers on the game thread, in the order of the packets
/// (`NetHandlerPlayClient.handleConfirmTransaction`). GrimAC sends a
/// transaction before and after a teleport and needs the replies in that
/// order: the position reply must leave between the two. The session thread
/// used to answer at once, so the second transaction overtook the position
/// reply and Grim flagged `BadPacketsN` (observed 2026-11-02).
pub(in crate::game) fn answer_transaction(
    game: &Game,
    packet: &umicraft_protocol::packets::play::clientbound::ConfirmTransaction,
    queue: &mut CommandQueue,
) {
    if !packet.accepted && game.windows.answers_transaction(packet.window_id) {
        queue.push(ClientCommand::ConfirmTransaction {
            window_id: packet.window_id as i8,
            action_number: packet.action_number,
        });
    }
}

/// `Open Window`: a container screen.
pub(super) fn open_window(game: &mut Game, packet: OpenWindow, frames: &SyncSender<RenderEvent>) {
    if let Some(kind) = WindowKind::from_type_string(&packet.window_type) {
        game.windows
            .open_window(packet.window_id, kind, packet.title, packet.slot_count);
        publish_inventory(game, frames);
    } else {
        tracing::warn!(
            window_type = %packet.window_type,
            "unknown container type, ignoring"
        );
    }
}

/// `Close Window`.
pub(super) fn close_window(game: &mut Game, window_id: u8, frames: &SyncSender<RenderEvent>) {
    game.windows.close_window(window_id);
    publish_inventory(game, frames);
}

/// `Set Slot`.
pub(super) fn set_slot(game: &mut Game, packet: SetSlot, frames: &SyncSender<RenderEvent>) {
    game.windows
        .apply_set_slot(packet.window_id, packet.slot, packet.item);
    sync_armor(game);
    publish_inventory(game, frames);
}

/// `Window Items`: every slot of a window.
pub(super) fn fill_window(game: &mut Game, packet: &WindowItems, frames: &SyncSender<RenderEvent>) {
    game.windows
        .apply_window_items(packet.window_id, &packet.items);
    sync_armor(game);
    publish_inventory(game, frames);
}

/// `Window Property`: furnace progress, enchantment costs and the like.
pub(super) fn set_window_property(
    game: &mut Game,
    packet: &WindowProperty,
    frames: &SyncSender<RenderEvent>,
) {
    game.windows
        .apply_property(packet.window_id, packet.property, packet.value);
    publish_inventory(game, frames);
}

/// `Confirm Transaction`: the server accepts or rolls back a click.
pub(super) fn confirm_transaction(
    game: &mut Game,
    packet: &ConfirmTransaction,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    answer_transaction(game, packet, queue);
    // The prediction is confirmed or rolled back.
    game.windows
        .confirm_transaction(packet.window_id, packet.action_number, packet.accepted);
    sync_armor(game);
    publish_inventory(game, frames);
}
