// SPDX-License-Identifier: MIT OR Apache-2.0

//! The horse window: which horse it belongs to and whether it wears armour.

use umicraft_protocol::packets::play::clientbound::OpenWindow;
use umicraft_protocol::packets::play::metadata::WatchedObject;

use super::*;

/// A game tracking a horse of `horse_type` (entry 19) as entity 42.
fn game_with_horse(horse_type: u8) -> Game {
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
    let mut metadata = Metadata::default();
    metadata.objects.push(WatchedObject {
        type_id: 0,
        index: 19,
        value: vec![horse_type],
    });
    game.entities.apply_metadata(EntityId(42), &metadata);
    game
}

/// The horse of the window after `Open Window` for entity 42.
fn opened_horse(game: &mut Game) -> Option<umicraft_game::inventory::HorseWindow> {
    let (frames, _received) = mpsc::sync_channel(8);
    let packet = OpenWindow {
        window_id: 3,
        window_type: "EntityHorse".into(),
        title: "\"Horse\"".into(),
        slot_count: 2,
        entity_id: Some(42),
    };
    let mut queue = CommandQueue::default();
    handle_net(game, PlayEvent::OpenWindow(packet), &mut queue, &frames);
    game.windows.view().container.and_then(|c| c.horse)
}

#[test]
fn a_horse_window_knows_its_horse_wears_armour() {
    let horse = opened_horse(&mut game_with_horse(0));
    assert_eq!(
        horse.map(|h| (h.entity_id, h.wears_armor)),
        Some((42, true))
    );
}

#[test]
fn a_donkey_window_has_no_armour() {
    let horse = opened_horse(&mut game_with_horse(1));
    assert_eq!(horse.map(|h| h.wears_armor), Some(false));
}
