// SPDX-License-Identifier: MIT OR Apache-2.0

//! The I/O-free play session: one clientbound packet in, one [`Reaction`] out
//! (ADR-0010).
//!
//! The session never touches a socket, so it can be raced in a `select!`
//! without risk and tested packet by packet. [`super::drive`] is the loop that
//! feeds it and writes what it answers.

use umicraft_protocol::packets::play::clientbound::{
    self, Animation, AttachEntity, BlockChange, ChunkData, DestroyEntities, EntityEquipment,
    EntityHeadLook, EntityLook, EntityLookMove, EntityMetadata, EntityRelativeMove, EntityStatus,
    EntityTeleport, EntityVelocity, MapChunkBulk, MultiBlockChange, PositionLook, Respawn,
    SetExperience, SpawnMob, SpawnObject, SpawnPlayer, SpawnPosition, TimeUpdate, UpdateHealth,
};
use umicraft_protocol::packets::play::serverbound::{Brand, ClientSettings, KeepAlive};
use umicraft_protocol::varint::write_var_u32;
use umicraft_protocol::{PlayServerbound, ProtocolError};

use crate::error::NetError;
use crate::status::disconnect_reason;

/// The most serverbound packets one clientbound packet answers with: the two
/// entry packets of the first `Join Game`.
const MAX_REPLIES: usize = 2;

/// Every skin layer shown, as a vanilla client announces by default.
const ALL_SKIN_PARTS: u8 = 0b0111_1111;

/// Facts captured from `Join Game`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JoinGameInfo {
    /// Player's entity id in this world.
    pub entity_id: i32,
    /// Raw gamemode byte (1.8 folds hardcore into it).
    pub gamemode: u8,
    /// Dimension byte.
    pub dimension: i8,
    /// Difficulty byte.
    pub difficulty: u8,
    /// Player cap advertised in the packet.
    pub max_players: u8,
    /// World type string, e.g. `flat`.
    pub level_type: String,
    /// 1.8 trailing reduced-debug flag from `Join Game`.
    pub reduced_debug_info: bool,
}

/// The absolute placement imposed by `Player Position and Look` (0x08), after
/// the relative flags were resolved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerPlacement {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Z.
    pub z: f64,
    /// Yaw.
    pub yaw: f32,
    /// Pitch.
    pub pitch: f32,
}

impl PlayerPlacement {
    /// The `Player Position and Look` (0x06) packet that acknowledges this
    /// placement.
    ///
    /// The 1.8 server ignores the player's movement until it receives it
    /// (ADR-0005 §A.5). The game thread sends its own acknowledgement after
    /// applying the correction; this helper serves callers without a
    /// simulation (the headless binary, the tests).
    #[must_use]
    pub fn acknowledgement(&self, on_ground: bool) -> PlayServerbound {
        PlayServerbound::PositionLook(
            umicraft_protocol::packets::play::serverbound::PositionLook {
                x: self.x,
                y: self.y,
                z: self.z,
                yaw: self.yaw,
                pitch: self.pitch,
                on_ground,
            },
        )
    }
}

/// What the client announces in its entry packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayOptions {
    /// Locale string, e.g. `en_us`.
    pub locale: String,
    /// Render distance in chunks announced in `Client Settings`.
    pub view_distance: u8,
    /// Whether colors are rendered in chat.
    pub chat_colors: bool,
    /// Client brand sent on the `MC|Brand` channel (spec NET-9).
    pub brand: String,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            locale: "en_us".to_string(),
            view_distance: 8,
            chat_colors: true,
            brand: "umicraft".to_string(),
        }
    }
}

/// A clientbound play packet the game thread consumes.
///
/// Keepalives never become events: the session answers them in its
/// [`Reaction`]. The chunk payloads are boxed so the bounded inbox of the game
/// thread stays small.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayEvent {
    /// The server entered play state and described the world.
    JoinGame(JoinGameInfo),
    /// Absolute player placement after resolving the 1.8 relative flags.
    Placement(PlayerPlacement),
    /// Chat/system JSON.
    Chat(String),
    /// The world clock changed (`Time Update`, 0x03).
    ///
    /// The signed time of day is preserved: a negative value means the server
    /// froze the day/night cycle.
    TimeUpdate(TimeUpdate),
    /// The player's health, food and saturation.
    UpdateHealth(UpdateHealth),
    /// The selected hotbar slot.
    HeldItemChange(i8),
    /// The player changed dimension.
    Respawn(Respawn),
    /// An entity was teleported.
    EntityTeleport(EntityTeleport),
    /// The equipment of an entity.
    EntityEquipment(EntityEquipment),
    /// Another player was spawned.
    SpawnPlayer(SpawnPlayer),
    /// A mob was spawned.
    SpawnMob(SpawnMob),
    /// An object or item was spawned.
    SpawnObject(SpawnObject),
    /// Entities were destroyed / removed.
    DestroyEntities(DestroyEntities),
    /// An entity moved relative to its previous position.
    EntityRelativeMove(EntityRelativeMove),
    /// An entity rotated.
    EntityLook(EntityLook),
    /// An entity moved and rotated.
    EntityLookMove(EntityLookMove),
    /// An entity's head turned.
    EntityHeadLook(EntityHeadLook),
    /// An entity's velocity changed.
    EntityVelocity(EntityVelocity),
    /// An entity's metadata changed.
    EntityMetadata(EntityMetadata),
    /// An entity was attached (riding / leashed).
    AttachEntity(AttachEntity),
    /// An entity status event occurred.
    EntityStatus(EntityStatus),
    /// An entity animation occurred.
    Animation(Animation),
    /// The world spawn point.
    SpawnPosition(SpawnPosition),
    /// The experience bar changed.
    SetExperience(SetExperience),
    /// One chunk column, complete or a section update.
    ChunkData(Box<ChunkData>),
    /// Initial bulk chunk batch.
    MapChunkBulk(Box<MapChunkBulk>),
    /// One block update.
    BlockChange(BlockChange),
    /// Batched block updates in one chunk.
    MultiBlockChange(MultiBlockChange),
}

/// What one clientbound packet produced.
///
/// A reaction is a value: nothing was sent when it exists. The caller writes
/// the [`replies`](Self::replies), stops on a [`kick_reason`](Self::kick_reason)
/// and forwards the [`event`](Self::into_event).
#[derive(Debug, Default, PartialEq)]
pub struct Reaction {
    event: Option<PlayEvent>,
    replies: [Option<PlayServerbound>; MAX_REPLIES],
    kick_reason: Option<String>,
}

impl Reaction {
    /// A reaction that only forwards an event.
    fn event(event: PlayEvent) -> Self {
        Self {
            event: Some(event),
            ..Self::default()
        }
    }

    /// The serverbound packets to write, in order.
    pub fn replies(&self) -> impl Iterator<Item = &PlayServerbound> {
        self.replies.iter().flatten()
    }

    /// The reason of a play `Disconnect`; the session is over when it is set.
    #[must_use]
    pub fn kick_reason(&self) -> Option<&str> {
        self.kick_reason.as_deref()
    }

    /// The event for the game thread, consuming the reaction.
    #[must_use]
    pub fn into_event(self) -> Option<PlayEvent> {
        self.event
    }
}

/// The play-state half of a connection, without I/O (ADR-0010).
///
/// ```
/// use umicraft_net::play::{PlayOptions, PlaySession};
/// use umicraft_protocol::packets::play::clientbound::KeepAlive;
/// use umicraft_protocol::{EncodablePacket, PlayServerbound};
///
/// let mut session = PlaySession::new(PlayOptions::default());
/// let mut body = Vec::new();
/// KeepAlive { id: 7 }.encode_body(&mut body).expect("encodes");
/// let reaction = session.handle(KeepAlive::ID, &body).expect("valid packet");
/// assert!(matches!(
///     reaction.replies().next(),
///     Some(PlayServerbound::KeepAlive(answer)) if answer.id == 7
/// ));
/// ```
#[derive(Debug)]
pub struct PlaySession {
    options: PlayOptions,
    entry_sent: bool,
    placement: Option<PlayerPlacement>,
}

impl PlaySession {
    /// Creates the session of a connection that just finished its login.
    #[must_use]
    pub fn new(options: PlayOptions) -> Self {
        Self {
            options,
            entry_sent: false,
            placement: None,
        }
    }

    /// Handles one clientbound play packet.
    ///
    /// Ids the client does not consume yet produce an empty reaction.
    ///
    /// # Errors
    /// [`NetError::Protocol`] when the body of a handled id is malformed; the
    /// caller ends the session (spec NET-13).
    pub fn handle(&mut self, id: u8, body: &[u8]) -> Result<Reaction, NetError> {
        self.react(id, &mut &body[..]).map_err(|err| {
            tracing::warn!(packet_id = id, body_len = body.len(), %err, "play packet decode failed");
            NetError::from(err)
        })
    }

    fn react(&mut self, id: u8, body: &mut &[u8]) -> Result<Reaction, ProtocolError> {
        Ok(match id {
            clientbound::KeepAlive::ID => {
                let packet = clientbound::KeepAlive::decode(body)?;
                let mut reaction = Reaction::default();
                reaction.replies[0] = Some(PlayServerbound::KeepAlive(KeepAlive { id: packet.id }));
                reaction
            }
            clientbound::JoinGame::ID => self.join_game(clientbound::JoinGame::decode(body)?)?,
            clientbound::Chat::ID => {
                Reaction::event(PlayEvent::Chat(clientbound::Chat::decode(body)?.json))
            }
            clientbound::TimeUpdate::ID => {
                Reaction::event(PlayEvent::TimeUpdate(TimeUpdate::decode(body)?))
            }
            clientbound::PositionLook::ID => {
                let placement = self.resolve_placement(&PositionLook::decode(body)?);
                self.placement = Some(placement.clone());
                Reaction::event(PlayEvent::Placement(placement))
            }
            clientbound::UpdateHealth::ID => {
                Reaction::event(PlayEvent::UpdateHealth(UpdateHealth::decode(body)?))
            }
            clientbound::HeldItemChange::ID => Reaction::event(PlayEvent::HeldItemChange(
                clientbound::HeldItemChange::decode(body)?.slot,
            )),
            clientbound::Respawn::ID => Reaction::event(PlayEvent::Respawn(Respawn::decode(body)?)),
            clientbound::EntityTeleport::ID => {
                Reaction::event(PlayEvent::EntityTeleport(EntityTeleport::decode(body)?))
            }
            clientbound::EntityEquipment::ID => {
                Reaction::event(PlayEvent::EntityEquipment(EntityEquipment::decode(body)?))
            }
            clientbound::SpawnPlayer::ID => {
                Reaction::event(PlayEvent::SpawnPlayer(SpawnPlayer::decode(body)?))
            }
            clientbound::SpawnMob::ID => {
                Reaction::event(PlayEvent::SpawnMob(SpawnMob::decode(body)?))
            }
            clientbound::SpawnObject::ID => {
                Reaction::event(PlayEvent::SpawnObject(SpawnObject::decode(body)?))
            }
            clientbound::DestroyEntities::ID => {
                Reaction::event(PlayEvent::DestroyEntities(DestroyEntities::decode(body)?))
            }
            clientbound::Entity::ID => Reaction::default(),
            clientbound::EntityRelativeMove::ID => Reaction::event(PlayEvent::EntityRelativeMove(
                EntityRelativeMove::decode(body)?,
            )),
            clientbound::EntityLook::ID => {
                Reaction::event(PlayEvent::EntityLook(EntityLook::decode(body)?))
            }
            clientbound::EntityLookMove::ID => {
                Reaction::event(PlayEvent::EntityLookMove(EntityLookMove::decode(body)?))
            }
            clientbound::EntityHeadLook::ID => {
                Reaction::event(PlayEvent::EntityHeadLook(EntityHeadLook::decode(body)?))
            }
            clientbound::EntityVelocity::ID => {
                Reaction::event(PlayEvent::EntityVelocity(EntityVelocity::decode(body)?))
            }
            clientbound::EntityMetadata::ID => {
                Reaction::event(PlayEvent::EntityMetadata(EntityMetadata::decode(body)?))
            }
            clientbound::AttachEntity::ID => {
                Reaction::event(PlayEvent::AttachEntity(AttachEntity::decode(body)?))
            }
            clientbound::EntityStatus::ID => {
                Reaction::event(PlayEvent::EntityStatus(EntityStatus::decode(body)?))
            }
            clientbound::Animation::ID => {
                Reaction::event(PlayEvent::Animation(Animation::decode(body)?))
            }
            clientbound::SpawnPosition::ID => {
                Reaction::event(PlayEvent::SpawnPosition(SpawnPosition::decode(body)?))
            }
            clientbound::SetExperience::ID => {
                Reaction::event(PlayEvent::SetExperience(SetExperience::decode(body)?))
            }
            clientbound::ChunkData::ID => {
                Reaction::event(PlayEvent::ChunkData(Box::new(ChunkData::decode(body)?)))
            }
            clientbound::MapChunkBulk::ID => Reaction::event(PlayEvent::MapChunkBulk(Box::new(
                MapChunkBulk::decode(body)?,
            ))),
            clientbound::BlockChange::ID => {
                Reaction::event(PlayEvent::BlockChange(BlockChange::decode(body)?))
            }
            clientbound::MultiBlockChange::ID => {
                Reaction::event(PlayEvent::MultiBlockChange(MultiBlockChange::decode(body)?))
            }
            clientbound::Disconnect::ID => {
                let packet = clientbound::Disconnect::decode(body)?;
                Reaction {
                    kick_reason: Some(disconnect_reason(&packet.reason)),
                    ..Reaction::default()
                }
            }
            other => {
                tracing::trace!(id = other, bytes = body.len(), "skipped play packet");
                Reaction::default()
            }
        })
    }

    /// Forwards `Join Game` and, the first time only, answers with the entry
    /// packets.
    ///
    /// The order matters: a 1.8 server that receives them before it switched
    /// to the play state logs `Bad packet id 21`.
    fn join_game(&mut self, packet: clientbound::JoinGame) -> Result<Reaction, ProtocolError> {
        let mut reaction = Reaction::event(PlayEvent::JoinGame(JoinGameInfo {
            entity_id: packet.entity_id,
            gamemode: packet.gamemode,
            dimension: packet.dimension,
            difficulty: packet.difficulty,
            max_players: packet.max_players,
            level_type: packet.level_type,
            reduced_debug_info: packet.reduced_debug_info,
        }));
        if !self.entry_sent {
            reaction.replies = self.entry_packets()?;
            self.entry_sent = true;
        }
        Ok(reaction)
    }

    /// The `Client Settings` and `MC|Brand` packets built from the options.
    fn entry_packets(&self) -> Result<[Option<PlayServerbound>; MAX_REPLIES], ProtocolError> {
        let settings = ClientSettings {
            locale: self.options.locale.clone(),
            view_distance: self.options.view_distance,
            chat_mode: 0,
            chat_colors: self.options.chat_colors,
            skin_parts: ALL_SKIN_PARTS,
        };
        // The brand channel carries a length-prefixed string.
        let brand = self.options.brand.as_bytes();
        let mut data = Vec::with_capacity(brand.len() + 5);
        write_var_u32(&mut data, brand.len() as u32)?;
        data.extend_from_slice(brand);
        let brand = Brand {
            channel: "MC|Brand".to_string(),
            data,
        };
        Ok([
            Some(PlayServerbound::ClientSettings(settings)),
            Some(PlayServerbound::Brand(brand)),
        ])
    }

    /// Applies the 1.8 relative-position flag bits to the previous placement.
    ///
    /// Flags: X, Y, Z, yaw, pitch (bits 0–4); vanilla accumulates relative
    /// components on the last accepted position.
    fn resolve_placement(&self, packet: &PositionLook) -> PlayerPlacement {
        let previous = self.placement.clone().unwrap_or_default();
        let pick = |bit: u8, previous: f64, value: f64| {
            if packet.flags & bit != 0 {
                previous + value
            } else {
                value
            }
        };
        let pick_angle = |bit: u8, previous: f32, value: f32| {
            if packet.flags & bit != 0 {
                previous + value
            } else {
                value
            }
        };
        PlayerPlacement {
            x: pick(0x01, previous.x, packet.x),
            y: pick(0x02, previous.y, packet.y),
            z: pick(0x04, previous.z, packet.z),
            yaw: pick_angle(0x08, previous.yaw, packet.yaw),
            pitch: pick_angle(0x10, previous.pitch, packet.pitch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::ItemStack;
    use umicraft_protocol::{EncodablePacket, PlayClientbound};

    fn body(packet: &impl EncodablePacket) -> Vec<u8> {
        let mut body = Vec::new();
        packet.encode_body(&mut body).expect("test packet encodes");
        body
    }

    /// Feeds one packet through the generic clientbound enum, so every test
    /// uses the wire id the protocol crate assigns.
    fn react(session: &mut PlaySession, packet: PlayClientbound) -> Reaction {
        let mut body = Vec::new();
        packet.encode_body(&mut body).expect("test packet encodes");
        session
            .handle(packet.packet_id(), &body)
            .expect("valid packet")
    }

    fn event(session: &mut PlaySession, packet: PlayClientbound) -> PlayEvent {
        let reaction = react(session, packet);
        assert_eq!(reaction.replies().count(), 0, "no reply expected");
        assert_eq!(reaction.kick_reason(), None);
        reaction.into_event().expect("the packet becomes an event")
    }

    fn join_game() -> PlayClientbound {
        PlayClientbound::JoinGame(clientbound::JoinGame {
            entity_id: 42,
            gamemode: 1,
            dimension: -1,
            difficulty: 2,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        })
    }

    fn position_look(flags: u8) -> PlayClientbound {
        PlayClientbound::PositionLook(PositionLook {
            x: 1.5,
            y: 70.0,
            z: -2.5,
            yaw: 15.0,
            pitch: -20.0,
            flags,
        })
    }

    #[test]
    fn a_keepalive_is_answered_with_the_same_id_and_no_event() {
        let mut session = PlaySession::new(PlayOptions::default());
        let reaction = react(
            &mut session,
            PlayClientbound::KeepAlive(clientbound::KeepAlive { id: 4242 }),
        );
        let replies: Vec<_> = reaction.replies().cloned().collect();
        assert_eq!(
            replies,
            vec![PlayServerbound::KeepAlive(KeepAlive { id: 4242 })]
        );
        assert_eq!(reaction.into_event(), None);
    }

    #[test]
    fn the_first_join_game_sends_the_entry_packets_from_the_options() {
        let options = PlayOptions {
            locale: "fr_fr".to_string(),
            view_distance: 12,
            chat_colors: false,
            brand: "test".to_string(),
        };
        let mut session = PlaySession::new(options);
        let reaction = react(&mut session, join_game());

        let replies: Vec<_> = reaction.replies().cloned().collect();
        assert_eq!(
            replies,
            vec![
                PlayServerbound::ClientSettings(ClientSettings {
                    locale: "fr_fr".to_string(),
                    view_distance: 12,
                    chat_mode: 0,
                    chat_colors: false,
                    skin_parts: ALL_SKIN_PARTS,
                }),
                PlayServerbound::Brand(Brand {
                    channel: "MC|Brand".to_string(),
                    data: vec![4, b't', b'e', b's', b't'],
                }),
            ]
        );
        let Some(PlayEvent::JoinGame(info)) = reaction.into_event() else {
            panic!("join game must be forwarded");
        };
        assert_eq!(info.entity_id, 42);
        assert_eq!(info.dimension, -1);
        assert_eq!(info.level_type, "flat");
    }

    #[test]
    fn a_second_join_game_does_not_resend_the_entry_packets() {
        let mut session = PlaySession::new(PlayOptions::default());
        let _ = react(&mut session, join_game());
        let event = event(&mut session, join_game());
        assert!(matches!(event, PlayEvent::JoinGame(_)));
    }

    #[test]
    fn a_placement_is_forwarded_without_an_answer() {
        let mut session = PlaySession::new(PlayOptions::default());
        let event = event(&mut session, position_look(0));
        assert_eq!(
            event,
            PlayEvent::Placement(PlayerPlacement {
                x: 1.5,
                y: 70.0,
                z: -2.5,
                yaw: 15.0,
                pitch: -20.0,
            })
        );
    }

    #[test]
    fn relative_flags_accumulate_on_the_last_placement() {
        let mut session = PlaySession::new(PlayOptions::default());
        session.placement = Some(PlayerPlacement {
            x: 10.0,
            y: 64.0,
            z: -5.0,
            yaw: 90.0,
            pitch: 10.0,
        });
        // X, Z and yaw are deltas; Y and pitch stay absolute.
        let PlayEvent::Placement(placement) =
            event(&mut session, position_look(0x01 | 0x04 | 0x08))
        else {
            panic!("placement expected");
        };
        assert_eq!(placement.x, 11.5);
        assert_eq!(placement.y, 70.0);
        assert_eq!(placement.z, -7.5);
        assert_eq!(placement.yaw, 105.0);
        assert_eq!(placement.pitch, -20.0);

        // The resolved placement becomes the base of the next relative one.
        let PlayEvent::Placement(next) = event(&mut session, position_look(0x02 | 0x10)) else {
            panic!("placement expected");
        };
        assert_eq!(next.x, 1.5);
        assert_eq!(next.y, 140.0);
        assert_eq!(next.pitch, -40.0);
    }

    #[test]
    fn every_consumed_packet_becomes_its_event() {
        let mut session = PlaySession::new(PlayOptions::default());
        let time = TimeUpdate {
            world_age: 72_770,
            time_of_day: -6_000,
        };
        let health = UpdateHealth {
            health: 15.0,
            food: 18,
            saturation: 2.5,
        };
        let respawn = Respawn {
            dimension: -1,
            difficulty: 0,
            game_mode: 0,
            world_type: "default".to_string(),
        };
        let teleport = EntityTeleport {
            entity_id: 9,
            x: 32,
            y: 64,
            z: -32,
            yaw: 0,
            pitch: 0,
            on_ground: true,
        };
        let equipment = EntityEquipment {
            entity_id: 9,
            slot: 4,
            item: ItemStack::EMPTY,
        };
        let spawn = SpawnPosition { x: 1, y: 64, z: -1 };
        let experience = SetExperience {
            experience: 0.5,
            level: 3,
            total: 40,
        };
        let block = BlockChange {
            packed_position: 0x40_0000_0123,
            block_state: 16,
        };
        let cases = vec![
            (
                PlayClientbound::Chat(clientbound::Chat {
                    json: r#"{"text":"hi"}"#.to_string(),
                    position: 0,
                }),
                PlayEvent::Chat(r#"{"text":"hi"}"#.to_string()),
            ),
            (
                PlayClientbound::TimeUpdate(time),
                PlayEvent::TimeUpdate(time),
            ),
            (
                PlayClientbound::UpdateHealth(health),
                PlayEvent::UpdateHealth(health),
            ),
            (
                PlayClientbound::HeldItemChange(clientbound::HeldItemChange { slot: 3 }),
                PlayEvent::HeldItemChange(3),
            ),
            (
                PlayClientbound::Respawn(respawn.clone()),
                PlayEvent::Respawn(respawn),
            ),
            (
                PlayClientbound::EntityTeleport(teleport),
                PlayEvent::EntityTeleport(teleport),
            ),
            (
                PlayClientbound::EntityEquipment(equipment.clone()),
                PlayEvent::EntityEquipment(equipment),
            ),
            (
                PlayClientbound::SpawnPosition(spawn),
                PlayEvent::SpawnPosition(spawn),
            ),
            (
                PlayClientbound::SetExperience(experience),
                PlayEvent::SetExperience(experience),
            ),
            (
                PlayClientbound::BlockChange(block),
                PlayEvent::BlockChange(block),
            ),
        ];
        for (packet, expected) in cases {
            assert_eq!(event(&mut session, packet), expected);
        }
    }

    #[test]
    fn world_packets_are_boxed_into_their_events() {
        let mut session = PlaySession::new(PlayOptions::default());
        let chunk = ChunkData {
            chunk_x: 3,
            chunk_z: -4,
            ground_up: true,
            primary_bit_mask: 0,
            data: vec![0; 256],
        };
        assert_eq!(
            event(&mut session, PlayClientbound::ChunkData(chunk.clone())),
            PlayEvent::ChunkData(Box::new(chunk))
        );
        let changes = MultiBlockChange {
            chunk_x: 0,
            chunk_z: 0,
            records: Vec::new(),
        };
        assert_eq!(
            event(
                &mut session,
                PlayClientbound::MultiBlockChange(changes.clone())
            ),
            PlayEvent::MultiBlockChange(changes)
        );
    }

    #[test]
    fn a_disconnect_carries_the_readable_reason() {
        let mut session = PlaySession::new(PlayOptions::default());
        let reaction = react(
            &mut session,
            PlayClientbound::Disconnect(clientbound::Disconnect {
                reason: r#"{"text":"session over"}"#.to_string(),
            }),
        );
        assert_eq!(reaction.kick_reason(), Some("session over"));
        assert_eq!(reaction.replies().count(), 0);
        assert_eq!(reaction.into_event(), None);
    }

    #[test]
    fn an_unconsumed_id_produces_an_empty_reaction() {
        let mut session = PlaySession::new(PlayOptions::default());
        // `Block Action` (0x24) is decoded by the protocol crate but not
        // consumed by the client session yet; its body is not even read.
        let reaction = session.handle(0x24, &[0xFF]).expect("skipped, not failed");
        assert_eq!(reaction, Reaction::default());
    }

    #[test]
    fn a_malformed_body_of_a_handled_id_is_an_error() {
        let mut session = PlaySession::new(PlayOptions::default());
        let time = body(&TimeUpdate {
            world_age: 1,
            time_of_day: 2,
        });
        let result = session.handle(TimeUpdate::ID, &time[..4]);
        assert!(matches!(result, Err(NetError::Protocol(_))), "{result:?}");
    }

    #[test]
    fn the_acknowledgement_repeats_the_placement() {
        let placement = PlayerPlacement {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
        };
        let PlayServerbound::PositionLook(answer) = placement.acknowledgement(true) else {
            panic!("the acknowledgement is a position-and-look packet");
        };
        assert_eq!((answer.x, answer.y, answer.z), (0.5, 64.0, -0.5));
        assert_eq!(
            (answer.yaw, answer.pitch, answer.on_ground),
            (90.0, 0.0, true)
        );
    }
}
