// SPDX-License-Identifier: MIT OR Apache-2.0

//! The serverbound commands: the ordered queue towards the network thread
//! and the conversion of the interaction rules' packets.

use std::collections::VecDeque;

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender as CommandSender;
use umicraft_game::interaction::Outgoing;
use umicraft_protocol::packets::play::serverbound::UseEntityAction;
use umicraft_protocol::PlayServerbound;

use crate::network::ClientCommand;

/// The commands that could not be handed to the network thread yet.
///
/// Every command keeps its place: the movement packet of each tick, the
/// clicks and the actions leave in the order they were queued, like the
/// netty queue of the vanilla client (MCP `NetworkManager.sendPacket`). The
/// vanilla client sends one movement packet per tick, so nothing is
/// coalesced or replaced.
///
/// The queue cannot grow without bound: while it holds [`PENDING_LIMIT`]
/// commands (the socket does not take them), [`CommandQueue::congested`]
/// is true and the game loop does not start a new tick. No packet is
/// dropped; the simulation waits for the connection instead.
#[derive(Default)]
pub(super) struct CommandQueue {
    /// The commands in the order they were queued.
    pub(super) ordered: VecDeque<ClientCommand>,
    pub(super) sender: Option<CommandSender<PlayServerbound>>,
}

/// The commands the queue holds before the game loop stops ticking. The
/// channel to the socket takes 64, so the queue only fills when the socket
/// does not drain; at 20 ticks a second this is several seconds of ticks.
pub(super) const PENDING_LIMIT: usize = 128;

/// Whether the command is the movement report of a tick.
pub(super) fn is_movement(command: &ClientCommand) -> bool {
    matches!(
        command,
        ClientCommand::Input { .. }
            | ClientCommand::Player { .. }
            | ClientCommand::PlayerPosition { .. }
            | ClientCommand::PlayerLook { .. }
            | ClientCommand::PositionLook { .. }
    )
}

impl CommandQueue {
    /// Queues a command; the socket thread picks it up.
    pub(super) fn push(&mut self, command: ClientCommand) {
        self.ordered.push_back(command);
    }

    /// Queues a command ahead of the movement report that ends the tick,
    /// looking back no further than `since` (the length of the queue when
    /// the tick began its report).
    pub(super) fn push_before_movement(&mut self, command: ClientCommand, since: usize) {
        let mut at = self.ordered.len();
        while at > since && is_movement(&self.ordered[at - 1]) {
            at -= 1;
        }
        self.ordered.insert(at, command);
    }

    /// Whether the socket is so far behind that no new tick may start.
    pub(super) fn congested(&self) -> bool {
        self.ordered.len() >= PENDING_LIMIT
    }

    /// Retries the queued commands, oldest first.
    ///
    /// A command is converted into its wire packet only once the channel
    /// reserved a slot for it, so a full channel keeps the command and
    /// every one behind it for the next flush.
    pub(super) fn flush(&mut self) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        while let Some(command) = self.ordered.pop_front() {
            if let Err(command) = try_send(sender, command) {
                self.ordered.push_front(command);
                break;
            }
        }
    }
}

/// Sends a command to the network thread without blocking.
///
/// Gives the command back when the channel is full; drops it when the network
/// thread is gone, since nothing will ever read it.
fn try_send(
    sender: &CommandSender<PlayServerbound>,
    command: ClientCommand,
) -> Result<(), ClientCommand> {
    match sender.try_reserve() {
        Ok(permit) => {
            permit.send(command.into_packet());
            Ok(())
        }
        Err(TrySendError::Full(())) => Err(command),
        Err(TrySendError::Closed(())) => Ok(()),
    }
}

/// The network command of a packet the interaction rules produced.
pub(super) fn command_of(outgoing: Outgoing) -> ClientCommand {
    match outgoing {
        Outgoing::Dig {
            status,
            block,
            face,
        } => ClientCommand::PlayerDigging {
            status,
            x: block.x,
            y: block.y,
            z: block.z,
            face,
        },
        Outgoing::Place {
            block,
            face,
            item,
            cursor: [cursor_x, cursor_y, cursor_z],
        } => ClientCommand::PlayerBlockPlacement {
            x: block.x,
            y: block.y,
            z: block.z,
            face,
            item,
            cursor_x,
            cursor_y,
            cursor_z,
        },
        Outgoing::UseItem { item } => ClientCommand::UseItem { item },
        Outgoing::Swing => ClientCommand::Swing,
        Outgoing::Attack { entity } => use_entity(entity, UseEntityAction::Attack),
        Outgoing::Interact { entity } => use_entity(entity, UseEntityAction::Interact),
        Outgoing::InteractAt {
            entity,
            offset: [x, y, z],
        } => use_entity(entity, UseEntityAction::InteractAt { x, y, z }),
        Outgoing::AckPosition(ack) => ClientCommand::AckPosition {
            x: ack.x,
            y: ack.y,
            z: ack.z,
            yaw: ack.yaw,
            pitch: ack.pitch,
            on_ground: ack.on_ground,
        },
    }
}

/// The `Use Entity` command of an entity click.
fn use_entity(target: i32, action: UseEntityAction) -> ClientCommand {
    ClientCommand::UseEntity { target, action }
}
