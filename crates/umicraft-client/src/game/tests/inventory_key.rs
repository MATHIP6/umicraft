// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packet of the inventory key.

use super::*;

/// The commands queued after the inventory key opened the inventory.
fn after_inventory_key(game: &mut Game) -> Vec<ClientCommand> {
    let (frames, _received) = mpsc::sync_channel(8);
    let mut queue = CommandQueue::default();
    handle_event(game, GameEvent::InventoryOpened, &mut queue, &frames);
    queue.ordered.into_iter().collect()
}

#[test]
fn opening_the_inventory_tells_the_server() {
    // Regression: the inventory opened without `Client Status`
    // (OPEN_INVENTORY_ACHIEVEMENT), which vanilla always sends.
    let mut game = placed_game();
    assert_eq!(
        after_inventory_key(&mut game),
        [ClientCommand::OpenInventory]
    );
}

#[test]
fn the_packet_is_the_open_inventory_client_status() {
    assert!(matches!(
        ClientCommand::OpenInventory.into_packet(),
        PlayServerbound::ClientStatus(umicraft_protocol::packets::play::serverbound::ClientStatus {
            action: umicraft_protocol::packets::play::serverbound::ClientStatusAction::OpenInventoryAchievement,
        })
    ));
}

#[test]
fn nothing_is_sent_on_a_horse() {
    let mut game = placed_game();
    game.entities.spawn_mob(
        EntityId(42),
        100,
        DVec3::new(0.0, 4.0, 0.0),
        0.0,
        0.0,
        0.0,
        Vec3::ZERO,
        EntityFlags::default(),
    );
    game.vehicle = Some(42);
    assert!(after_inventory_key(&mut game).is_empty());
}

#[test]
fn nothing_is_sent_outside_the_play_state() {
    let mut game = Game::new();
    assert!(after_inventory_key(&mut game).is_empty());
}
