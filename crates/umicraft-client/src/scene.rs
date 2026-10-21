// SPDX-License-Identifier: MIT OR Apache-2.0

//! Main-thread world state fed by [`crate::network::NetEvent`]s.
//!
//! Keeping world mutation on the render thread means chunk snapshots can be
//! taken without locking and the mesher only ever sees immutable `Arc` column
//! versions.

use std::collections::BTreeSet;

use umicraft_net::client::{JoinGameInfo, PlayerPlacement};
use umicraft_world::{ChunkPos, World};

use crate::network::NetEvent;

/// World plus connection/status fields shown in the debug overlay.
#[derive(Debug, Default)]
pub struct SessionWorld {
    /// Current dimension's world, or `None` before `Join Game`.
    pub world: Option<World>,
    /// Latest `Join Game` facts.
    pub join: Option<JoinGameInfo>,
    /// Latest spawn/teleport placement.
    pub placement: Option<PlayerPlacement>,
    /// Human-readable connection/world status.
    pub status: String,
    /// Chat/system messages observed.
    pub chat_messages: u64,
    /// Chunk packets applied (0x21/0x26).
    pub chunk_packets: u64,
    /// Individual block updates applied (0x22/0x23 records).
    pub block_updates: u64,
    /// Chunks whose geometry may have changed since the last mesher dispatch.
    pub dirty: BTreeSet<ChunkPos>,
}

impl SessionWorld {
    /// Creates an empty scene waiting for `Join Game`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: "CONNECTING".to_string(),
            ..Self::default()
        }
    }

    /// Applies one network event.
    ///
    /// Returns `true` when a `Join Game` replaced the world, so the caller must
    /// drop all GPU meshes and in-flight mesh jobs for the previous dimension.
    pub fn apply(&mut self, event: NetEvent) -> bool {
        match event {
            NetEvent::JoinGame(info) => {
                self.world = Some(World::new(info.dimension));
                self.join = Some(info);
                self.placement = None;
                self.dirty.clear();
                self.status = "PLAYING".to_string();
                true
            }
            NetEvent::Placement(placement) => {
                self.placement = Some(placement);
                false
            }
            NetEvent::Chat(_) => {
                self.chat_messages += 1;
                false
            }
            NetEvent::ChunkData(packet) => {
                self.chunk_packets += 1;
                let Some(world) = self.world.as_mut() else {
                    return false;
                };
                match world.apply_chunk_data(&packet) {
                    Ok(dirty) => self.dirty.extend(dirty),
                    Err(error) => {
                        tracing::warn!(%error, "chunk data rejected");
                        self.status = format!("WORLD ERROR {error}");
                    }
                }
                false
            }
            NetEvent::MapChunkBulk(packet) => {
                self.chunk_packets += 1;
                let Some(world) = self.world.as_mut() else {
                    return false;
                };
                match world.apply_map_chunk_bulk(&packet) {
                    Ok(dirty) => self.dirty.extend(dirty),
                    Err(error) => {
                        tracing::warn!(%error, "bulk chunk rejected");
                        self.status = format!("WORLD ERROR {error}");
                    }
                }
                false
            }
            NetEvent::BlockChange(packet) => {
                self.block_updates += 1;
                if let Some(world) = self.world.as_mut() {
                    self.dirty.extend(world.apply_block_change(&packet));
                }
                false
            }
            NetEvent::MultiBlockChange(packet) => {
                self.block_updates += packet.records.len() as u64;
                if let Some(world) = self.world.as_mut() {
                    self.dirty.extend(world.apply_multi_block_change(&packet));
                }
                false
            }
            NetEvent::Error(message) => {
                tracing::error!(%message, "network session failed");
                self.status = format!("ERROR {message}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join() -> NetEvent {
        NetEvent::JoinGame(JoinGameInfo {
            entity_id: 7,
            gamemode: 1,
            dimension: 0,
            difficulty: 0,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        })
    }

    #[test]
    fn join_creates_world_and_resets_scene() {
        let mut scene = SessionWorld::new();
        assert_eq!(scene.status, "CONNECTING");
        assert!(scene.apply(join()));
        let world = scene.world.as_ref().expect("world");
        assert_eq!(world.dimension(), 0);
        assert_eq!(scene.join.as_ref().expect("join").level_type, "flat");
        assert_eq!(scene.status, "PLAYING");
    }

    #[test]
    fn placement_and_chat_are_recorded() {
        let mut scene = SessionWorld::new();
        scene.apply(join());
        assert!(!scene.apply(NetEvent::Placement(PlayerPlacement {
            x: 1.5,
            y: 4.0,
            z: -2.5,
            yaw: 90.0,
            pitch: 10.0,
        })));
        assert_eq!(scene.placement.as_ref().expect("placement").x, 1.5);
        scene.apply(NetEvent::Chat("{\"text\":\"hi\"}".to_string()));
        assert_eq!(scene.chat_messages, 1);
    }

    #[test]
    fn errors_are_visible() {
        let mut scene = SessionWorld::new();
        scene.apply(NetEvent::Error("kicked".to_string()));
        assert!(scene.status.contains("kicked"));
    }
}
