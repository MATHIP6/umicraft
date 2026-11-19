// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sign text follows its block: a replaced block or an unloaded column
//! forgets it, a turned sign keeps it.

use umicraft_protocol::packets::play::clientbound::{BlockChange, UpdateSign};

use super::*;

const STANDING_SIGN: u16 = 63;
const SIGN_AT: (i32, i32, i32) = (3, 64, 3);

/// A game with a standing sign reading "hi" at [`SIGN_AT`].
fn game_with_sign() -> (Game, CommandQueue, SyncSender<RenderEvent>) {
    let mut game = placed_game();
    let (x, y, z) = SIGN_AT;
    if let Some(world) = game.world.as_mut() {
        world.set_block(BlockPos::new(x, y, z), BlockState::new(STANDING_SIGN, 0));
    }
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let packet = UpdateSign {
        x,
        y,
        z,
        lines: ["\"hi\"".into(), String::new(), String::new(), String::new()],
    };
    handle_net(
        &mut game,
        PlayEvent::UpdateSign(Box::new(packet)),
        &mut queue,
        &frames,
    );
    assert!(game.server.block_entities().sign(SIGN_AT).is_some());
    (game, queue, frames)
}

fn block_change(state: BlockState) -> PlayEvent {
    let (x, y, z) = SIGN_AT;
    PlayEvent::BlockChange(BlockChange::at(x, y, z, state.wire()))
}

#[test]
fn a_broken_sign_forgets_its_text() {
    let (mut game, mut queue, frames) = game_with_sign();
    handle_net(
        &mut game,
        block_change(BlockState::AIR),
        &mut queue,
        &frames,
    );
    assert!(game.server.block_entities().sign(SIGN_AT).is_none());
}

#[test]
fn a_sign_turned_by_the_server_keeps_its_text() {
    let (mut game, mut queue, frames) = game_with_sign();
    handle_net(
        &mut game,
        block_change(BlockState::new(STANDING_SIGN, 8)),
        &mut queue,
        &frames,
    );
    assert!(game.server.block_entities().sign(SIGN_AT).is_some());
}

#[test]
fn an_unloaded_column_forgets_its_signs() {
    let (mut game, mut queue, frames) = game_with_sign();
    let unload = ChunkData {
        chunk_x: 0,
        chunk_z: 0,
        ground_up: true,
        primary_bit_mask: 0,
        data: Vec::new(),
    };
    handle_net(
        &mut game,
        PlayEvent::ChunkData(Box::new(unload)),
        &mut queue,
        &frames,
    );
    assert!(game.server.block_entities().sign(SIGN_AT).is_none());
}
