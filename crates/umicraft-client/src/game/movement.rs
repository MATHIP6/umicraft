// SPDX-License-Identifier: MIT OR Apache-2.0

//! The movement packets of each tick (`umicraft_game::traffic`).

use umicraft_game::traffic::{SteerInput, TickPacket, TickSnapshot};
use umicraft_game::InputIntent;
use umicraft_world::ChunkPos;

use super::commands::CommandQueue;
use super::state::Game;
use crate::network::ClientCommand;

/// Sends the movement packets of the tick (`umicraft_game::traffic`).
///
/// Nothing is sent before the server is in the play state and the player is
/// placed (a play packet that arrives during the login makes the server log
/// `Bad packet id`), nor while the chunk under the player is missing: MCP
/// `EntityPlayerSP.onUpdate` skips the whole update until
/// `World.isBlockLoaded` holds for the player's column. The steer packet carries the raw wire values; the 0.98
/// damping of `EntityLivingBase.java:2020` is applied by the game logic and
/// by the server, so it must not be applied twice.
pub(super) fn report_movement(game: &mut Game, intent: &InputIntent, queue: &mut CommandQueue) {
    let Some(state) = game
        .state
        .as_ref()
        .filter(|_| game.playing && !game.spectating())
    else {
        return;
    };
    let position = state.position();
    let column = ChunkPos::new(
        (position.x.floor() as i32).div_euclid(16),
        (position.z.floor() as i32).div_euclid(16),
    );
    if game
        .world
        .as_ref()
        .and_then(|world| world.chunk(column))
        .is_none()
    {
        return;
    }
    let player = state.player();
    let snapshot = TickSnapshot {
        position,
        yaw: game.yaw.to_degrees(),
        pitch: game.pitch.to_degrees(),
        on_ground: player.on_ground,
        sprinting: player.sprinting,
        sneaking: player.sneaking,
        riding: game.vehicle.map(|_| SteerInput {
            strafe: intent.strafe,
            forward: intent.forward,
            jump: intent.jump,
            sneak: intent.sneak,
        }),
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
