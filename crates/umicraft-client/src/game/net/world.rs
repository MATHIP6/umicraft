// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packets that change blocks: chunks, block changes, explosions and
//! block actions.

use glam::DVec3;
use umicraft_game::interaction::BlockAck;
use umicraft_game::piston::motion;
use umicraft_protocol::packets::play::clientbound::{
    BlockAction, BlockChange, ChunkData, Explosion, MapChunkBulk, MultiBlockChange,
};
use umicraft_world::{BlockPos, BlockState, ChunkPayload, ChunkPos};

use crate::game::particles::particle_filter;
use crate::game::state::Game;

/// Removes the blocks of an explosion from the world.
///
/// MCP `NetHandlerPlayClient.handleExplosion` runs `Explosion.doExplosionB`
/// on the client, which sets every affected block to air before the server's
/// block changes arrive. The player's knockback from the packet is P2.
pub(in crate::game) fn apply_explosion(game: &mut Game, packet: &Explosion) {
    // MCP `handleExplosion`: the push is added to the motion of the player.
    if let Some(state) = game.state.as_mut() {
        state.add_velocity(DVec3::new(
            f64::from(packet.player_motion_x),
            f64::from(packet.player_motion_y),
            f64::from(packet.player_motion_z),
        ));
    }
    let Some(world) = game.world.as_mut() else {
        return;
    };
    let affected: Vec<(i32, i32, i32)> = packet.affected_blocks().collect();
    for &(x, y, z) in &affected {
        game.dirty
            .extend(world.apply_block_change(BlockPos::new(x, y, z), BlockState::AIR));
    }
    if let Some(filter) = particle_filter(game) {
        game.particles.explode(
            DVec3::new(packet.x.into(), packet.y.into(), packet.z.into()),
            packet.strength,
            &affected,
            &filter,
        );
    }
}

/// The column and payload of a `Chunk Data` packet.
pub(in crate::game) fn chunk_payload(packet: &ChunkData) -> (ChunkPos, ChunkPayload<'_>) {
    (
        ChunkPos::new(packet.chunk_x, packet.chunk_z),
        ChunkPayload {
            primary_bit_mask: packet.primary_bit_mask,
            ground_up: packet.ground_up,
            data: &packet.data,
        },
    )
}

/// The columns and payloads of a `Map Chunk Bulk` packet; every bulk entry
/// is a ground-up column.
pub(in crate::game) fn bulk_payloads(packet: &MapChunkBulk) -> Vec<(ChunkPos, ChunkPayload<'_>)> {
    packet
        .chunks
        .iter()
        .map(|chunk| {
            (
                ChunkPos::new(chunk.chunk_x, chunk.chunk_z),
                ChunkPayload {
                    primary_bit_mask: chunk.primary_bit_mask,
                    ground_up: true,
                    data: &chunk.data,
                },
            )
        })
        .collect()
}

/// Counts a block change the server confirmed.
pub(in crate::game) fn acknowledge_block(game: &mut Game, position: BlockPos, block_state: u16) {
    tracing::debug!(?position, block_state, "block change applied");
    match game.interaction.acknowledge_block(position) {
        Some(BlockAck::Break) => {
            game.status_data.breaks_acknowledged += 1;
            tracing::info!(
                ?position,
                block_state,
                "the server confirmed the block break"
            );
        }
        Some(BlockAck::Placement) => {
            game.status_data.placements_acknowledged += 1;
            tracing::info!(
                ?position,
                block_state,
                "the server confirmed the block placement"
            );
        }
        None => {}
    }
}

/// `Chunk Data`: one column, or its unload.
pub(super) fn load_chunk(game: &mut Game, packet: &ChunkData) {
    game.status_data.chunk_packets += 1;
    if let Some(world) = game.world.as_mut() {
        let (pos, payload) = chunk_payload(packet);
        match world.apply_chunk(pos, payload) {
            Ok(dirty) => game.dirty.extend(dirty),
            Err(error) => {
                tracing::warn!(%error, "chunk data rejected");
                game.status = format!("WORLD ERROR {error}");
            }
        }
    }
}

/// `Map Chunk Bulk`: several ground-up columns.
pub(super) fn load_chunk_bulk(game: &mut Game, packet: &MapChunkBulk) {
    game.status_data.chunk_packets += 1;
    if let Some(world) = game.world.as_mut() {
        match world.apply_chunk_bulk(packet.sky_light, &bulk_payloads(packet)) {
            Ok(dirty) => game.dirty.extend(dirty),
            Err(error) => {
                tracing::warn!(%error, "bulk chunk rejected");
                game.status = format!("WORLD ERROR {error}");
            }
        }
    }
}

/// `Block Change`: one block, which may confirm a dig or a placement.
pub(super) fn change_block(game: &mut Game, packet: &BlockChange) {
    game.status_data.block_updates += 1;
    let (x, y, z) = packet.position();
    let position = BlockPos::new(x, y, z);
    if let Some(world) = game.world.as_mut() {
        let state = BlockState::from_wire(packet.block_state);
        game.dirty.extend(world.apply_block_change(position, state));
    }
    acknowledge_block(game, position, packet.block_state);
}

/// `Multi Block Change`: several blocks of one chunk.
pub(super) fn change_blocks(game: &mut Game, packet: &MultiBlockChange) {
    game.status_data.block_updates += packet.records.len() as u64;
    if let Some(world) = game.world.as_mut() {
        let chunk = ChunkPos::new(packet.chunk_x, packet.chunk_z);
        let changes = packet.records.iter().map(|record| {
            (
                record.local_position(),
                BlockState::from_wire(record.block_state),
            )
        });
        game.dirty.extend(world.apply_block_changes(chunk, changes));
    }
    // The server confirms a dig or a placement with a single block
    // change, but a batch carries the same information.
    for record in &packet.records {
        let (x, y, z) = record.local_position();
        let position = BlockPos::new(
            packet.chunk_x * 16 + i32::from(x),
            i32::from(y),
            packet.chunk_z * 16 + i32::from(z),
        );
        acknowledge_block(game, position, record.block_state);
    }
}

/// `Block Action`: pistons start moving blocks; the rest is state.
pub(super) fn start_block_action(game: &mut Game, packet: &BlockAction) {
    if let Some(world) = game.world.as_mut() {
        let dirty = motion::start(world, game.server.moving_blocks_mut(), packet);
        game.dirty.extend(dirty);
    }
    game.server.apply_block_action(packet);
}
