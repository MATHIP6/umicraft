// SPDX-License-Identifier: MIT OR Apache-2.0

//! Play state (`State::Play`): the packet set needed to join a 1.8 server,
//! stay connected, receive world data (M2), and move and interact in the
//! world (M3).
//!
//! Direction convention: `clientbound` flows server → client (inbound on a
//! client), `serverbound` flows client → server (outbound on a client).
//!
//! The wire id of a play packet is its **registration index** in
//! `mcp:net/minecraft/network/EnumConnectionState.java` (the `PLAY`
//! constant), not the MCP class-name prefix — `S19PacketEntityStatus` is
//! registered at 0x1A, while 0x19 belongs to `S19PacketEntityHeadLook`
//! (ADR-0005 §A.4).
//!
//! Clientbound: `Keep Alive` (0x00), `Join Game` (0x01), `Chat Message`
//! (0x02), `Time Update` (0x03), `Entity Equipment` (0x04), `Spawn
//! Position` (0x05), `Update Health` (0x06), `Respawn` (0x07), `Player
//! Position and Look` (0x08), `Held Item Change` (0x09), `Animation`
//! (0x0B), `Spawn Player` (0x0C), `Destroy Entities` (0x13), `Entity`
//! (0x14), `Entity Relative Move` (0x15), `Entity Look` (0x16), `Entity
//! Look and Move` (0x17), `Entity Teleport` (0x18), `Entity Status`
//! (0x1A), `Set Experience` (0x1F), `Chunk Data` (0x21), `Multi Block
//! Change` (0x22), `Block Change` (0x23), `Block Action` (0x24), `Block
//! Break Animation` (0x25), `Map Chunk Bulk` (0x26), `Change Game State`
//! (0x2B), `Open Window` (0x2D), `Close Window` (0x2E), `Set Slot`
//! (0x2F), `Window Items` (0x30), `Window Property` (0x31), `Confirm
//! Transaction` (0x32), `Disconnect` (0x40).
//!
//! Serverbound: `Keep Alive` (0x00), `Chat Message` (0x01), `Use Entity`
//! (0x02), `Player Position` (0x04), `Player Look` (0x05), `Player
//! Position and Look` (0x06), `Player Digging` (0x07), `Player Block
//! Placement` (0x08), `Held Item Change` (0x09), `Animation` (0x0A),
//! `Entity Action` (0x0B), `Input` (0x0C), `Close Window` (0x0D),
//! `Click Window` (0x0E), `Confirm Transaction` (0x0F), `Creative
//! Inventory Action` (0x10), `Client Settings` (0x15), `Plugin Message`
//! (0x17).
//!
//! Ids outside this subset are rejected as [`ProtocolError::UnknownPacket`]
//! and skipped by the caller (see `umicraft-net`'s play loop). `Entity
//! Metadata` (0x1C) and `Entity Properties` (0x1D) are out of scope for M3
//! (ADR-0005 §A.4).

pub mod clientbound;
pub mod facing;
pub mod item_stack;
pub mod metadata;
mod nbt;
mod position;
pub mod serverbound;
mod wire;

use std::io::{Read, Write};

use crate::error::ProtocolError;

use super::EncodablePacket;

pub use facing::Facing;
pub use item_stack::ItemStack;
pub use metadata::{Metadata, TypedMetadata};
pub use position::{pack_block_position, unpack_block_position};

/// Clientbound play-state packets (inbound for a client).
#[derive(Debug, Clone, PartialEq)]
pub enum PlayClientbound {
    /// `Keep Alive` (0x00): must be echoed with the same id.
    KeepAlive(clientbound::KeepAlive),
    /// `Join Game` (0x01): entry into the world.
    JoinGame(clientbound::JoinGame),
    /// `Chat Message` (0x02): chat/system JSON.
    Chat(clientbound::Chat),
    /// `Time Update` (0x03): world age and time of day.
    TimeUpdate(clientbound::TimeUpdate),
    /// `Entity Equipment` (0x04): one equipped item of one entity.
    EntityEquipment(clientbound::EntityEquipment),
    /// `Spawn Position` (0x05): the world spawn point.
    SpawnPosition(clientbound::SpawnPosition),
    /// `Update Health` (0x06): health, food and saturation.
    UpdateHealth(clientbound::UpdateHealth),
    /// `Respawn` (0x07): the player moves to another world.
    Respawn(clientbound::Respawn),
    /// `Player Position and Look` (0x08): teleport / initial placement.
    PositionLook(clientbound::PositionLook),
    /// `Held Item Change` (0x09): the server changed the selected slot.
    HeldItemChange(clientbound::HeldItemChange),
    /// `Animation` (0x0B): an entity swung its arm or turned.
    Animation(clientbound::Animation),
    /// `Spawn Player` (0x0C): another player entered the world.
    SpawnPlayer(clientbound::SpawnPlayer),
    /// `Spawn Object` (0x0E): an object/item was spawned.
    SpawnObject(clientbound::SpawnObject),
    /// `Spawn Mob` (0x0F): a living non-player entity was spawned.
    SpawnMob(clientbound::SpawnMob),
    /// `Entity Velocity` (0x12): an entity's velocity changed.
    EntityVelocity(clientbound::EntityVelocity),
    /// `Destroy Entities` (0x13): entities left the world.
    DestroyEntities(clientbound::DestroyEntities),
    /// `Entity` (0x14): the legacy entity id packet.
    Entity(clientbound::Entity),
    /// `Entity Relative Move` (0x15): an entity moved by a delta.
    EntityRelativeMove(clientbound::EntityRelativeMove),
    /// `Entity Look` (0x16): an entity turned.
    EntityLook(clientbound::EntityLook),
    /// `Entity Look and Move` (0x17): an entity moved and turned.
    EntityLookMove(clientbound::EntityLookMove),
    /// `Entity Teleport` (0x18): an entity was placed absolutely.
    EntityTeleport(clientbound::EntityTeleport),
    /// `Entity Head Look` (0x19): an entity's head turned.
    EntityHeadLook(clientbound::EntityHeadLook),
    /// `Entity Status` (0x1A): an entity triggered a status event.
    EntityStatus(clientbound::EntityStatus),
    /// `Attach Entity` (0x1B): an entity was mounted or leashed.
    AttachEntity(clientbound::AttachEntity),
    /// `Entity Metadata` (0x1C): an entity's data watchers changed.
    EntityMetadata(clientbound::EntityMetadata),
    /// `Set Experience` (0x1F): the experience bar changed.
    SetExperience(clientbound::SetExperience),
    /// `Chunk Data` (0x21): one chunk column, full or partial.
    ChunkData(clientbound::ChunkData),
    /// `Multi Block Change` (0x22): batched block updates in one chunk.
    MultiBlockChange(clientbound::MultiBlockChange),
    /// `Block Change` (0x23): one block update.
    BlockChange(clientbound::BlockChange),
    /// `Block Action` (0x24): a block was used.
    BlockAction(clientbound::BlockAction),
    /// `Block Break Animation` (0x25): someone is breaking a block.
    BlockBreakAnim(clientbound::BlockBreakAnim),
    /// `Map Chunk Bulk` (0x26): initial chunk batch.
    MapChunkBulk(clientbound::MapChunkBulk),
    /// `Change Game State` (0x2B): a global game-state value changed.
    ChangeGameState(clientbound::ChangeGameState),
    /// `Open Window` (0x2D): the server opens a container GUI.
    OpenWindow(clientbound::OpenWindow),
    /// `Close Window` (0x2E): the server closes a container GUI.
    CloseWindow(clientbound::CloseWindow),
    /// `Set Slot` (0x2F): the server updates a single slot.
    SetSlot(clientbound::SetSlot),
    /// `Window Items` (0x30): the server sends all slots of a window.
    WindowItems(clientbound::WindowItems),
    /// `Window Property` (0x31): a container property changed.
    WindowProperty(clientbound::WindowProperty),
    /// `Confirm Transaction` (0x32): the server confirms or rejects a click.
    ConfirmTransaction(clientbound::ConfirmTransaction),
    /// `Disconnect` (0x40): reason as chat JSON; the socket closes after.
    Disconnect(clientbound::Disconnect),
}

impl PlayClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::KeepAlive(_) => clientbound::KeepAlive::ID,
            Self::JoinGame(_) => clientbound::JoinGame::ID,
            Self::Chat(_) => clientbound::Chat::ID,
            Self::TimeUpdate(_) => clientbound::TimeUpdate::ID,
            Self::EntityEquipment(_) => clientbound::EntityEquipment::ID,
            Self::SpawnPosition(_) => clientbound::SpawnPosition::ID,
            Self::UpdateHealth(_) => clientbound::UpdateHealth::ID,
            Self::Respawn(_) => clientbound::Respawn::ID,
            Self::PositionLook(_) => clientbound::PositionLook::ID,
            Self::HeldItemChange(_) => clientbound::HeldItemChange::ID,
            Self::Animation(_) => clientbound::Animation::ID,
            Self::SpawnPlayer(_) => clientbound::SpawnPlayer::ID,
            Self::SpawnObject(_) => clientbound::SpawnObject::ID,
            Self::SpawnMob(_) => clientbound::SpawnMob::ID,
            Self::EntityVelocity(_) => clientbound::EntityVelocity::ID,
            Self::DestroyEntities(_) => clientbound::DestroyEntities::ID,
            Self::Entity(_) => clientbound::Entity::ID,
            Self::EntityRelativeMove(_) => clientbound::EntityRelativeMove::ID,
            Self::EntityLook(_) => clientbound::EntityLook::ID,
            Self::EntityLookMove(_) => clientbound::EntityLookMove::ID,
            Self::EntityTeleport(_) => clientbound::EntityTeleport::ID,
            Self::EntityHeadLook(_) => clientbound::EntityHeadLook::ID,
            Self::EntityStatus(_) => clientbound::EntityStatus::ID,
            Self::AttachEntity(_) => clientbound::AttachEntity::ID,
            Self::EntityMetadata(_) => clientbound::EntityMetadata::ID,
            Self::SetExperience(_) => clientbound::SetExperience::ID,
            Self::ChunkData(_) => clientbound::ChunkData::ID,
            Self::MultiBlockChange(_) => clientbound::MultiBlockChange::ID,
            Self::BlockChange(_) => clientbound::BlockChange::ID,
            Self::BlockAction(_) => clientbound::BlockAction::ID,
            Self::BlockBreakAnim(_) => clientbound::BlockBreakAnim::ID,
            Self::MapChunkBulk(_) => clientbound::MapChunkBulk::ID,
            Self::ChangeGameState(_) => clientbound::ChangeGameState::ID,
            Self::OpenWindow(_) => clientbound::OpenWindow::ID,
            Self::CloseWindow(_) => clientbound::CloseWindow::ID,
            Self::SetSlot(_) => clientbound::SetSlot::ID,
            Self::WindowItems(_) => clientbound::WindowItems::ID,
            Self::WindowProperty(_) => clientbound::WindowProperty::ID,
            Self::ConfirmTransaction(_) => clientbound::ConfirmTransaction::ID,
            Self::Disconnect(_) => clientbound::Disconnect::ID,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::KeepAlive(packet) => packet.encode_body(writer),
            Self::JoinGame(packet) => packet.encode_body(writer),
            Self::Chat(packet) => packet.encode_body(writer),
            Self::TimeUpdate(packet) => packet.encode_body(writer),
            Self::EntityEquipment(packet) => packet.encode_body(writer),
            Self::SpawnPosition(packet) => packet.encode_body(writer),
            Self::UpdateHealth(packet) => packet.encode_body(writer),
            Self::Respawn(packet) => packet.encode_body(writer),
            Self::PositionLook(packet) => packet.encode_body(writer),
            Self::HeldItemChange(packet) => packet.encode_body(writer),
            Self::Animation(packet) => packet.encode_body(writer),
            Self::SpawnPlayer(packet) => packet.encode_body(writer),
            Self::SpawnObject(packet) => packet.encode_body(writer),
            Self::SpawnMob(packet) => packet.encode_body(writer),
            Self::EntityVelocity(packet) => packet.encode_body(writer),
            Self::DestroyEntities(packet) => packet.encode_body(writer),
            Self::Entity(packet) => packet.encode_body(writer),
            Self::EntityRelativeMove(packet) => packet.encode_body(writer),
            Self::EntityLook(packet) => packet.encode_body(writer),
            Self::EntityLookMove(packet) => packet.encode_body(writer),
            Self::EntityTeleport(packet) => packet.encode_body(writer),
            Self::EntityHeadLook(packet) => packet.encode_body(writer),
            Self::EntityStatus(packet) => packet.encode_body(writer),
            Self::AttachEntity(packet) => packet.encode_body(writer),
            Self::EntityMetadata(packet) => packet.encode_body(writer),
            Self::SetExperience(packet) => packet.encode_body(writer),
            Self::ChunkData(packet) => packet.encode_body(writer),
            Self::MultiBlockChange(packet) => packet.encode_body(writer),
            Self::BlockChange(packet) => packet.encode_body(writer),
            Self::BlockAction(packet) => packet.encode_body(writer),
            Self::BlockBreakAnim(packet) => packet.encode_body(writer),
            Self::MapChunkBulk(packet) => packet.encode_body(writer),
            Self::ChangeGameState(packet) => packet.encode_body(writer),
            Self::OpenWindow(packet) => packet.encode_body(writer),
            Self::CloseWindow(packet) => packet.encode_body(writer),
            Self::SetSlot(packet) => packet.encode_body(writer),
            Self::WindowItems(packet) => packet.encode_body(writer),
            Self::WindowProperty(packet) => packet.encode_body(writer),
            Self::ConfirmTransaction(packet) => packet.encode_body(writer),
            Self::Disconnect(packet) => packet.encode_body(writer),
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for ids outside the supported subset.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            clientbound::KeepAlive::ID => Self::KeepAlive(clientbound::KeepAlive::decode(reader)?),
            clientbound::JoinGame::ID => Self::JoinGame(clientbound::JoinGame::decode(reader)?),
            clientbound::Chat::ID => Self::Chat(clientbound::Chat::decode(reader)?),
            clientbound::TimeUpdate::ID => {
                Self::TimeUpdate(clientbound::TimeUpdate::decode(reader)?)
            }
            clientbound::EntityEquipment::ID => {
                Self::EntityEquipment(clientbound::EntityEquipment::decode(reader)?)
            }
            clientbound::SpawnPosition::ID => {
                Self::SpawnPosition(clientbound::SpawnPosition::decode(reader)?)
            }
            clientbound::UpdateHealth::ID => {
                Self::UpdateHealth(clientbound::UpdateHealth::decode(reader)?)
            }
            clientbound::Respawn::ID => Self::Respawn(clientbound::Respawn::decode(reader)?),
            clientbound::PositionLook::ID => {
                Self::PositionLook(clientbound::PositionLook::decode(reader)?)
            }
            clientbound::HeldItemChange::ID => {
                Self::HeldItemChange(clientbound::HeldItemChange::decode(reader)?)
            }
            clientbound::Animation::ID => Self::Animation(clientbound::Animation::decode(reader)?),
            clientbound::SpawnPlayer::ID => {
                Self::SpawnPlayer(clientbound::SpawnPlayer::decode(reader)?)
            }
            clientbound::SpawnObject::ID => {
                Self::SpawnObject(clientbound::SpawnObject::decode(reader)?)
            }
            clientbound::SpawnMob::ID => Self::SpawnMob(clientbound::SpawnMob::decode(reader)?),
            clientbound::EntityVelocity::ID => {
                Self::EntityVelocity(clientbound::EntityVelocity::decode(reader)?)
            }
            clientbound::DestroyEntities::ID => {
                Self::DestroyEntities(clientbound::DestroyEntities::decode(reader)?)
            }
            clientbound::Entity::ID => Self::Entity(clientbound::Entity::decode(reader)?),
            clientbound::EntityRelativeMove::ID => {
                Self::EntityRelativeMove(clientbound::EntityRelativeMove::decode(reader)?)
            }
            clientbound::EntityLook::ID => {
                Self::EntityLook(clientbound::EntityLook::decode(reader)?)
            }
            clientbound::EntityLookMove::ID => {
                Self::EntityLookMove(clientbound::EntityLookMove::decode(reader)?)
            }
            clientbound::EntityTeleport::ID => {
                Self::EntityTeleport(clientbound::EntityTeleport::decode(reader)?)
            }
            clientbound::EntityHeadLook::ID => {
                Self::EntityHeadLook(clientbound::EntityHeadLook::decode(reader)?)
            }
            clientbound::EntityStatus::ID => {
                Self::EntityStatus(clientbound::EntityStatus::decode(reader)?)
            }
            clientbound::AttachEntity::ID => {
                Self::AttachEntity(clientbound::AttachEntity::decode(reader)?)
            }
            clientbound::EntityMetadata::ID => {
                Self::EntityMetadata(clientbound::EntityMetadata::decode(reader)?)
            }
            clientbound::SetExperience::ID => {
                Self::SetExperience(clientbound::SetExperience::decode(reader)?)
            }
            clientbound::ChunkData::ID => Self::ChunkData(clientbound::ChunkData::decode(reader)?),
            clientbound::MultiBlockChange::ID => {
                Self::MultiBlockChange(clientbound::MultiBlockChange::decode(reader)?)
            }
            clientbound::BlockChange::ID => {
                Self::BlockChange(clientbound::BlockChange::decode(reader)?)
            }
            clientbound::BlockAction::ID => {
                Self::BlockAction(clientbound::BlockAction::decode(reader)?)
            }
            clientbound::BlockBreakAnim::ID => {
                Self::BlockBreakAnim(clientbound::BlockBreakAnim::decode(reader)?)
            }
            clientbound::MapChunkBulk::ID => {
                Self::MapChunkBulk(clientbound::MapChunkBulk::decode(reader)?)
            }
            clientbound::ChangeGameState::ID => {
                Self::ChangeGameState(clientbound::ChangeGameState::decode(reader)?)
            }
            clientbound::OpenWindow::ID => {
                Self::OpenWindow(clientbound::OpenWindow::decode(reader)?)
            }
            clientbound::CloseWindow::ID => {
                Self::CloseWindow(clientbound::CloseWindow::decode(reader)?)
            }
            clientbound::SetSlot::ID => Self::SetSlot(clientbound::SetSlot::decode(reader)?),
            clientbound::WindowItems::ID => {
                Self::WindowItems(clientbound::WindowItems::decode(reader)?)
            }
            clientbound::WindowProperty::ID => {
                Self::WindowProperty(clientbound::WindowProperty::decode(reader)?)
            }
            clientbound::ConfirmTransaction::ID => {
                Self::ConfirmTransaction(clientbound::ConfirmTransaction::decode(reader)?)
            }
            clientbound::Disconnect::ID => {
                Self::Disconnect(clientbound::Disconnect::decode(reader)?)
            }
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Play,
                })
            }
        })
    }
}

/// Serverbound play-state packets (outbound for a client).
#[derive(Debug, Clone, PartialEq)]
pub enum PlayServerbound {
    /// `Keep Alive` (0x00): echo of the server's id.
    KeepAlive(serverbound::KeepAlive),
    /// `Chat Message` (0x01): user-typed text (includes leading slash commands).
    Chat(serverbound::Chat),
    /// `Use Entity` (0x02): interact with or attack an entity.
    UseEntity(serverbound::UseEntity),
    /// `Player Position` (0x04): the player moved.
    PlayerPosition(serverbound::PlayerPosition),
    /// `Player Look` (0x05): the player turned.
    PlayerLook(serverbound::PlayerLook),
    /// `Player Position and Look` (0x06): vanilla's standard teleport answer.
    PositionLook(serverbound::PositionLook),
    /// `Player Digging` (0x07): the player breaks a block.
    PlayerDigging(serverbound::PlayerDigging),
    /// `Player Block Placement` (0x08): the player places a block.
    PlayerBlockPlacement(serverbound::PlayerBlockPlacement),
    /// `Held Item Change` (0x09): the player selected another hotbar slot.
    HeldItemChange(serverbound::HeldItemChange),
    /// `Animation` (0x0A): the player swung their arm.
    Animation(serverbound::Animation),
    /// `Entity Action` (0x0B): sneak, sprint or sleep state changed.
    EntityAction(serverbound::EntityAction),
    /// `Input` (0x0C): the player's movement keys.
    Input(serverbound::Input),
    /// `Close Window` (0x0D): the client closes a container GUI.
    CloseWindow(serverbound::CloseWindow),
    /// `Click Window` (0x0E): the client clicked in a window.
    ClickWindow(serverbound::ClickWindow),
    /// `Confirm Transaction` (0x0F): the client echoes a rejected transaction.
    ConfirmTransaction(serverbound::ConfirmTransaction),
    /// `Creative Inventory Action` (0x10): the client sets a slot in creative mode.
    CreativeInventoryAction(serverbound::CreativeInventoryAction),
    /// `Client Settings` (0x15): required before some servers release content.
    ClientSettings(serverbound::ClientSettings),
    /// `Plugin Message` (0x17): client brand channel payload.
    Brand(serverbound::Brand),
}

impl PlayServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::KeepAlive(_) => serverbound::KeepAlive::ID,
            Self::Chat(_) => serverbound::Chat::ID,
            Self::UseEntity(_) => serverbound::UseEntity::ID,
            Self::PlayerPosition(_) => serverbound::PlayerPosition::ID,
            Self::PlayerLook(_) => serverbound::PlayerLook::ID,
            Self::PositionLook(_) => serverbound::PositionLook::ID,
            Self::PlayerDigging(_) => serverbound::PlayerDigging::ID,
            Self::PlayerBlockPlacement(_) => serverbound::PlayerBlockPlacement::ID,
            Self::HeldItemChange(_) => serverbound::HeldItemChange::ID,
            Self::Animation(_) => serverbound::Animation::ID,
            Self::EntityAction(_) => serverbound::EntityAction::ID,
            Self::Input(_) => serverbound::Input::ID,
            Self::CloseWindow(_) => serverbound::CloseWindow::ID,
            Self::ClickWindow(_) => serverbound::ClickWindow::ID,
            Self::ConfirmTransaction(_) => serverbound::ConfirmTransaction::ID,
            Self::CreativeInventoryAction(_) => serverbound::CreativeInventoryAction::ID,
            Self::ClientSettings(_) => serverbound::ClientSettings::ID,
            Self::Brand(_) => serverbound::Brand::ID,
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for ids outside the supported subset.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            serverbound::KeepAlive::ID => Self::KeepAlive(serverbound::KeepAlive::decode(reader)?),
            serverbound::Chat::ID => Self::Chat(serverbound::Chat::decode(reader)?),
            serverbound::UseEntity::ID => Self::UseEntity(serverbound::UseEntity::decode(reader)?),
            serverbound::PlayerPosition::ID => {
                Self::PlayerPosition(serverbound::PlayerPosition::decode(reader)?)
            }
            serverbound::PlayerLook::ID => {
                Self::PlayerLook(serverbound::PlayerLook::decode(reader)?)
            }
            serverbound::PositionLook::ID => {
                Self::PositionLook(serverbound::PositionLook::decode(reader)?)
            }
            serverbound::PlayerDigging::ID => {
                Self::PlayerDigging(serverbound::PlayerDigging::decode(reader)?)
            }
            serverbound::PlayerBlockPlacement::ID => {
                Self::PlayerBlockPlacement(serverbound::PlayerBlockPlacement::decode(reader)?)
            }
            serverbound::HeldItemChange::ID => {
                Self::HeldItemChange(serverbound::HeldItemChange::decode(reader)?)
            }
            serverbound::Animation::ID => Self::Animation(serverbound::Animation::decode(reader)?),
            serverbound::EntityAction::ID => {
                Self::EntityAction(serverbound::EntityAction::decode(reader)?)
            }
            serverbound::Input::ID => Self::Input(serverbound::Input::decode(reader)?),
            serverbound::CloseWindow::ID => {
                Self::CloseWindow(serverbound::CloseWindow::decode(reader)?)
            }
            serverbound::ClickWindow::ID => {
                Self::ClickWindow(serverbound::ClickWindow::decode(reader)?)
            }
            serverbound::ConfirmTransaction::ID => {
                Self::ConfirmTransaction(serverbound::ConfirmTransaction::decode(reader)?)
            }
            serverbound::CreativeInventoryAction::ID => {
                Self::CreativeInventoryAction(serverbound::CreativeInventoryAction::decode(reader)?)
            }
            serverbound::ClientSettings::ID => {
                Self::ClientSettings(serverbound::ClientSettings::decode(reader)?)
            }
            serverbound::Brand::ID => Self::Brand(serverbound::Brand::decode(reader)?),
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Play,
                })
            }
        })
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::KeepAlive(packet) => packet.encode_body(writer),
            Self::Chat(packet) => packet.encode_body(writer),
            Self::UseEntity(packet) => packet.encode_body(writer),
            Self::PlayerPosition(packet) => packet.encode_body(writer),
            Self::PlayerLook(packet) => packet.encode_body(writer),
            Self::PositionLook(packet) => packet.encode_body(writer),
            Self::PlayerDigging(packet) => packet.encode_body(writer),
            Self::PlayerBlockPlacement(packet) => packet.encode_body(writer),
            Self::HeldItemChange(packet) => packet.encode_body(writer),
            Self::Animation(packet) => packet.encode_body(writer),
            Self::EntityAction(packet) => packet.encode_body(writer),
            Self::Input(packet) => packet.encode_body(writer),
            Self::CloseWindow(packet) => packet.encode_body(writer),
            Self::ClickWindow(packet) => packet.encode_body(writer),
            Self::ConfirmTransaction(packet) => packet.encode_body(writer),
            Self::CreativeInventoryAction(packet) => packet.encode_body(writer),
            Self::ClientSettings(packet) => packet.encode_body(writer),
            Self::Brand(packet) => packet.encode_body(writer),
        }
    }
}

impl EncodablePacket for PlayClientbound {
    fn packet_id(&self) -> u8 {
        PlayClientbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        PlayClientbound::encode_body(self, writer)
    }
}

impl EncodablePacket for PlayServerbound {
    fn packet_id(&self) -> u8 {
        PlayServerbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        PlayServerbound::encode_body(self, writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keepalive_round_trips_across_directions() {
        let sent = PlayServerbound::KeepAlive(serverbound::KeepAlive { id: 424_242 });
        let mut buffer = Vec::new();
        sent.encode_body(&mut buffer).unwrap();
        assert_eq!(
            PlayClientbound::decode(clientbound::KeepAlive::ID, &mut &buffer[..]).unwrap(),
            PlayClientbound::KeepAlive(clientbound::KeepAlive { id: 424_242 })
        );
    }

    #[test]
    fn position_look_roundtrip_through_the_answer_packet() {
        let inbound = PlayClientbound::PositionLook(clientbound::PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            flags: 0,
        });
        let answer = PlayServerbound::PositionLook(serverbound::PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            on_ground: true,
        });
        let mut buffer = Vec::new();
        answer.encode_body(&mut buffer).unwrap();
        assert_eq!(answer.packet_id(), 0x06);

        // The clientbound variant echoes what a server would send for the
        // same place, with a trailing relative-flags byte instead of
        // `on_ground`.
        assert_eq!(inbound.packet_id(), 0x08);
        let mut raw = buffer[..buffer.len() - 1].to_vec();
        raw.push(0);
        let decoded = PlayClientbound::decode(0x08, &mut &raw[..]).unwrap();
        assert_eq!(decoded, inbound);
    }

    #[test]
    fn world_packets_round_trip_through_the_enum() {
        let chunk_data = clientbound::ChunkData {
            chunk_x: -107,
            chunk_z: 6,
            ground_up: true,
            primary_bit_mask: 1,
            data: vec![0; clientbound::chunk_data::chunk_payload_len(1, true, true)],
        };
        let bulk = clientbound::MapChunkBulk {
            sky_light: true,
            chunks: vec![clientbound::BulkChunk {
                chunk_x: -107,
                chunk_z: 6,
                primary_bit_mask: 1,
                data: vec![0; clientbound::chunk_data::chunk_payload_len(1, true, true)],
            }],
        };
        let play = vec![
            PlayClientbound::ChunkData(chunk_data),
            PlayClientbound::MultiBlockChange(clientbound::MultiBlockChange {
                chunk_x: -107,
                chunk_z: 6,
                records: vec![clientbound::BlockUpdate::at(1, 4, 2, 0x20)],
            }),
            PlayClientbound::BlockChange(clientbound::BlockChange::at(-1698, 4, 111, 0x20)),
            PlayClientbound::MapChunkBulk(bulk),
        ];
        for packet in play {
            let id = packet.packet_id();
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                PlayClientbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }

    #[test]
    fn dispatch_rejects_unknown_play_ids() {
        let mut empty: &[u8] = &[];
        let err = PlayClientbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));

        let mut empty: &[u8] = &[];
        let err = PlayServerbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));
    }

    /// Serverbound packets must decode back from their own encodings
    /// (symmetry contract the fake server depends on).
    #[test]
    fn serverbound_round_trip_symmetry() {
        let play = vec![
            (
                serverbound::KeepAlive::ID,
                PlayServerbound::KeepAlive(serverbound::KeepAlive { id: -7 }),
            ),
            (
                serverbound::Chat::ID,
                PlayServerbound::Chat(serverbound::Chat {
                    message: "msg".to_string(),
                }),
            ),
            (
                serverbound::PositionLook::ID,
                PlayServerbound::PositionLook(serverbound::PositionLook {
                    x: 1.5,
                    y: -2.5,
                    z: 300.0,
                    yaw: -90.0,
                    pitch: 12.0,
                    on_ground: false,
                }),
            ),
            (
                serverbound::ClientSettings::ID,
                PlayServerbound::ClientSettings(serverbound::ClientSettings {
                    locale: "de_de".to_string(),
                    view_distance: 12,
                    chat_mode: 2,
                    chat_colors: false,
                    skin_parts: 0b0101,
                }),
            ),
            (
                serverbound::Brand::ID,
                PlayServerbound::Brand(serverbound::Brand {
                    channel: "MC|Brand".to_string(),
                    data: vec![8, b'u', b'm', b'i'],
                }),
            ),
        ];
        for (id, packet) in play {
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                PlayServerbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }

    /// The M3 movement and interaction packets round-trip through the
    /// serverbound dispatch table.
    #[test]
    fn m3_serverbound_packets_round_trip() {
        let play = vec![
            (
                serverbound::UseEntity::ID,
                PlayServerbound::UseEntity(serverbound::UseEntity {
                    target: 42,
                    action: serverbound::UseEntityAction::Attack,
                }),
            ),
            (
                serverbound::PlayerPosition::ID,
                PlayServerbound::PlayerPosition(serverbound::PlayerPosition {
                    x: -1707.5,
                    y: 4.0,
                    z: 103.25,
                    on_ground: true,
                }),
            ),
            (
                serverbound::PlayerLook::ID,
                PlayServerbound::PlayerLook(serverbound::PlayerLook {
                    yaw: 90.0,
                    pitch: -12.5,
                    on_ground: false,
                }),
            ),
            (
                serverbound::PlayerDigging::ID,
                PlayServerbound::PlayerDigging(serverbound::PlayerDigging {
                    status: serverbound::DiggingStatus::StartDestroyBlock,
                    x: 1,
                    y: 64,
                    z: -2,
                    face: Facing::Up,
                }),
            ),
            (
                serverbound::PlayerBlockPlacement::ID,
                PlayServerbound::PlayerBlockPlacement(serverbound::PlayerBlockPlacement {
                    x: 1,
                    y: 64,
                    z: -2,
                    face: Facing::North,
                    item: ItemStack {
                        item_id: 1,
                        stack_size: 64,
                        damage: 0,
                        nbt: None,
                    },
                    cursor_x: 8,
                    cursor_y: 8,
                    cursor_z: 0,
                }),
            ),
            (
                serverbound::HeldItemChange::ID,
                PlayServerbound::HeldItemChange(serverbound::HeldItemChange { slot: 3 }),
            ),
            (
                serverbound::Animation::ID,
                PlayServerbound::Animation(serverbound::Animation),
            ),
            (
                serverbound::EntityAction::ID,
                PlayServerbound::EntityAction(serverbound::EntityAction {
                    entity_id: 42,
                    action: serverbound::EntityActionKind::StartSprinting,
                    aux_data: 0,
                }),
            ),
            (
                serverbound::Input::ID,
                PlayServerbound::Input(serverbound::Input {
                    strafe: 0.0,
                    forward: 0.98,
                    jumping: true,
                    sneaking: false,
                }),
            ),
        ];
        for (id, packet) in play {
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                PlayServerbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }

    /// The M3 world packets round-trip through the clientbound dispatch
    /// table.
    #[test]
    fn m3_clientbound_packets_round_trip() {
        let play = vec![
            PlayClientbound::TimeUpdate(clientbound::TimeUpdate {
                world_age: 1,
                time_of_day: -2,
            }),
            PlayClientbound::EntityEquipment(clientbound::EntityEquipment {
                entity_id: 42,
                slot: 0,
                item: ItemStack {
                    item_id: 276,
                    stack_size: 1,
                    damage: 5,
                    nbt: None,
                },
            }),
            PlayClientbound::SpawnPosition(clientbound::SpawnPosition {
                x: -1707,
                y: 4,
                z: 103,
            }),
            PlayClientbound::UpdateHealth(clientbound::UpdateHealth {
                health: 20.0,
                food: 20,
                saturation: 5.0,
            }),
            PlayClientbound::Respawn(clientbound::Respawn {
                dimension: 0,
                difficulty: 1,
                game_mode: 0,
                world_type: "flat".to_string(),
            }),
            PlayClientbound::HeldItemChange(clientbound::HeldItemChange { slot: 2 }),
            PlayClientbound::Animation(clientbound::Animation {
                entity_id: 42,
                animation: 0,
            }),
            PlayClientbound::SpawnPlayer(clientbound::SpawnPlayer {
                entity_id: 42,
                uuid: [7u8; 16],
                x: -100,
                y: 64,
                z: 200,
                yaw: 12,
                pitch: -3,
                held_item: -1,
                metadata: Metadata::default(),
            }),
            PlayClientbound::DestroyEntities(clientbound::DestroyEntities {
                entity_ids: vec![1, 2, 3],
            }),
            PlayClientbound::Entity(clientbound::Entity { entity_id: 42 }),
            PlayClientbound::EntityRelativeMove(clientbound::EntityRelativeMove {
                entity_id: 42,
                dx: -6,
                dy: 11,
                dz: -2,
                on_ground: true,
            }),
            PlayClientbound::EntityLook(clientbound::EntityLook {
                entity_id: 42,
                yaw: -40,
                pitch: 0,
                on_ground: false,
            }),
            PlayClientbound::EntityLookMove(clientbound::EntityLookMove {
                entity_id: 42,
                dx: 6,
                dy: 5,
                dz: -1,
                yaw: -59,
                pitch: 0,
                on_ground: false,
            }),
            PlayClientbound::EntityTeleport(clientbound::EntityTeleport {
                entity_id: 42,
                x: -54_953,
                y: 128,
                z: 3715,
                yaw: 81,
                pitch: 0,
                on_ground: true,
            }),
            PlayClientbound::EntityStatus(clientbound::EntityStatus {
                entity_id: 42,
                status: 2,
            }),
            PlayClientbound::SetExperience(clientbound::SetExperience {
                experience: 0.25,
                level: 3,
                total: 44,
            }),
            PlayClientbound::BlockAction(clientbound::BlockAction {
                x: 10,
                y: 64,
                z: -3,
                instrument: 1,
                pitch: 12,
                block_id: 54,
            }),
            PlayClientbound::BlockBreakAnim(clientbound::BlockBreakAnim {
                entity_id: 42,
                x: -5,
                y: 70,
                z: 12,
                stage: 4,
            }),
            PlayClientbound::ChangeGameState(clientbound::ChangeGameState {
                reason: 3,
                value: 1.0,
            }),
        ];
        for packet in play {
            let id = packet.packet_id();
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                PlayClientbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }

    /// The M3 ids are the registration indices of `EnumConnectionState`, not
    /// the MCP class-name prefixes (ADR-0005 §A.4).
    #[test]
    fn the_m3_wire_ids_match_the_registration_index() {
        assert_eq!(
            PlayClientbound::TimeUpdate(clientbound::TimeUpdate {
                world_age: 0,
                time_of_day: 0,
            })
            .packet_id(),
            0x03
        );
        assert_eq!(
            PlayClientbound::EntityStatus(clientbound::EntityStatus {
                entity_id: 0,
                status: 0,
            })
            .packet_id(),
            0x1A
        );
        assert_eq!(
            PlayClientbound::SetExperience(clientbound::SetExperience {
                experience: 0.0,
                level: 0,
                total: 0,
            })
            .packet_id(),
            0x1F
        );
        assert_eq!(
            PlayClientbound::ChangeGameState(clientbound::ChangeGameState {
                reason: 0,
                value: 0.0,
            })
            .packet_id(),
            0x2B
        );
        assert_eq!(
            PlayServerbound::UseEntity(serverbound::UseEntity {
                target: 0,
                action: serverbound::UseEntityAction::Interact,
            })
            .packet_id(),
            0x02
        );
        assert_eq!(
            PlayServerbound::Input(serverbound::Input {
                strafe: 0.0,
                forward: 0.0,
                jumping: false,
                sneaking: false,
            })
            .packet_id(),
            0x0C
        );
    }
}
