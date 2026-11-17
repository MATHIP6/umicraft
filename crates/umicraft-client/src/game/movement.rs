// SPDX-License-Identifier: MIT OR Apache-2.0

//! The movement packets of each tick (`umicraft_game::traffic`).

use umicraft_game::traffic::{SteerInput, TickPacket, TickSnapshot};
use umicraft_game::InputIntent;

use super::commands::CommandQueue;
use super::state::Game;
use crate::network::ClientCommand;

/// Sends the movement packets of the tick (`umicraft_game::traffic`).
///
/// Nothing is sent before the server is in the play state and the player is
/// placed (a play packet that arrives during the login makes the server log
/// `Bad packet id`). A missing chunk under the player does not stop the
/// reports: MCP `EntityPlayerSP.onUpdate` tests `World.isBlockLoaded`, which
/// is always true on the client (`ChunkProviderClient.chunkExists`), so
/// vanilla reports every tick while it waits for the chunk. The steer packet
/// carries the axes damped by 0.98, as the vanilla client sends them
/// ([`SteerInput::damped`]).
pub(super) fn report_movement(game: &mut Game, intent: &InputIntent, queue: &mut CommandQueue) {
    let Some(state) = game
        .state
        .as_ref()
        .filter(|_| game.playing && !game.spectating())
    else {
        return;
    };
    let position = state.position();
    let player = state.player();
    let snapshot = TickSnapshot {
        position,
        yaw: game.yaw,
        pitch: game.pitch,
        on_ground: player.on_ground,
        sprinting: player.sprinting,
        sneaking: player.sneaking,
        riding: game
            .vehicle
            .map(|_| SteerInput::damped(intent.strafe, intent.forward, intent.jump, intent.sneak)),
    };
    let packets = game.server_view.tick(&snapshot);
    if packets.reports_position() {
        game.status_data.position_reports += 1;
    }
    for packet in packets.iter() {
        queue.push(movement_command(*packet, game.entity_id));
    }
}

/// The command of one movement packet of the tick.
fn movement_command(packet: TickPacket, entity_id: i32) -> ClientCommand {
    match packet {
        TickPacket::EntityAction(action) => ClientCommand::EntityAction { entity_id, action },
        TickPacket::PositionLook {
            position,
            yaw,
            pitch,
            on_ground,
        } => ClientCommand::PositionLook {
            x: position.x,
            y: position.y,
            z: position.z,
            yaw,
            pitch,
            on_ground,
        },
        TickPacket::Position {
            position,
            on_ground,
        } => ClientCommand::PlayerPosition {
            x: position.x,
            y: position.y,
            z: position.z,
            on_ground,
        },
        TickPacket::Look {
            yaw,
            pitch,
            on_ground,
        } => ClientCommand::PlayerLook {
            yaw,
            pitch,
            on_ground,
        },
        TickPacket::Ground { on_ground } => ClientCommand::Player { on_ground },
        TickPacket::SteerVehicle(steer) => ClientCommand::Input {
            strafe: steer.strafe,
            forward: steer.forward,
            jumping: steer.jump,
            sneaking: steer.sneak,
        },
    }
}
