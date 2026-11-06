// SPDX-License-Identifier: MIT OR Apache-2.0

//! The decisions about a server resource pack (NET-11), as a state machine
//! without a window, a renderer or a socket.
//!
//! [`ServerPackFlow`] takes the events (an offer, the player's answer, the
//! end of a download, the end of the session) and returns [`Effect`]s for the
//! window glue (`server_pack_control`) to carry out. The statuses it asks to
//! send follow `NetHandlerPlayClient.handleResourcePack` and
//! `ResourcePackRepository.downloadResourcePack` of the reference client
//! (`docs/sources.md`): `ACCEPTED` when the download starts, then
//! `SUCCESSFULLY_LOADED` or `FAILED_DOWNLOAD`; `DECLINED` when the player or
//! the saved server refuses.
//!
//! Deviations: a `level://` offer names a file in `saves/`, which a
//! multiplayer-only client has none of, so it fails; an offer that arrives
//! while another is being asked or downloaded is declined instead of queued.

use std::path::{Path, PathBuf};

use umicraft_assets::server_pack::{validate_offer, ValidatedOffer};
use umicraft_protocol::packets::play::serverbound::ResourcePackResult;

use crate::pack_offer::ServerPackRequest;
use crate::servers::ResourcePackMode;

/// What the window has to do after an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Effect {
    /// Send `Resource Pack Status` to the server.
    Reply {
        /// The hash of the offer the status is about.
        hash: String,
        /// The status.
        result: ResourcePackResult,
    },
    /// Download the pack; report back with the same `generation`.
    Download {
        /// Identifies this download, so a result that comes after the
        /// session ended is ignored.
        generation: u64,
        /// The pack to get.
        offer: ValidatedOffer,
    },
    /// Reload the resource packs: [`ServerPackFlow::active_path`] changed.
    Reload,
}

/// What the flow is waiting for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Idle,
    /// The player is being asked.
    Prompting { hash: String, offer: ValidatedOffer },
    /// A worker thread is downloading.
    Downloading { hash: String, generation: u64 },
}

/// The life of the server resource pack of one session.
#[derive(Debug, Default)]
pub(super) struct ServerPackFlow {
    mode: ResourcePackMode,
    phase: Phase,
    /// The loaded server pack, the top-priority pack while it is there.
    active: Option<PathBuf>,
    generation: u64,
}

impl ServerPackFlow {
    /// Sets the pack mode of the server the next session connects to.
    pub(super) fn set_mode(&mut self, mode: ResourcePackMode) {
        self.mode = mode;
    }

    /// Whether the player has to answer the question now.
    pub(super) fn is_prompting(&self) -> bool {
        matches!(self.phase, Phase::Prompting { .. })
    }

    /// The loaded server pack, if any.
    pub(super) fn active_path(&self) -> Option<&Path> {
        self.active.as_deref()
    }

    /// The server offers a pack.
    pub(super) fn offer(&mut self, request: ServerPackRequest) -> Vec<Effect> {
        let hash = request.hash.clone();
        let reply = |result| Effect::Reply {
            hash: hash.clone(),
            result,
        };
        if self.phase != Phase::Idle || self.mode == ResourcePackMode::Disabled {
            return vec![reply(ResourcePackResult::Declined)];
        }
        let offer = match validate_offer(&request.into()) {
            Ok(offer @ ValidatedOffer::Remote { .. }) => offer,
            Ok(ValidatedOffer::Level(_)) | Err(_) => {
                return vec![reply(ResourcePackResult::FailedDownload)];
            }
        };
        if self.mode == ResourcePackMode::Prompt {
            self.phase = Phase::Prompting {
                hash: hash.clone(),
                offer,
            };
            return Vec::new();
        }
        let mut effects = vec![reply(ResourcePackResult::Accepted)];
        effects.push(self.start_download(hash.clone(), offer));
        effects
    }

    /// The player answered the question.
    pub(super) fn answer(&mut self, accepted: bool) -> Vec<Effect> {
        let Phase::Prompting { hash, offer } = std::mem::take(&mut self.phase) else {
            return Vec::new();
        };
        if !accepted {
            let result = ResourcePackResult::Declined;
            return vec![Effect::Reply { hash, result }];
        }
        let accepted = Effect::Reply {
            hash: hash.clone(),
            result: ResourcePackResult::Accepted,
        };
        vec![accepted, self.start_download(hash, offer)]
    }

    /// The download `generation` ended with the cached pack at `path`, or
    /// failed (the worker logs why).
    pub(super) fn downloaded(&mut self, generation: u64, path: Option<PathBuf>) -> Vec<Effect> {
        let Phase::Downloading {
            hash,
            generation: current,
        } = &self.phase
        else {
            return Vec::new();
        };
        if *current != generation {
            return Vec::new();
        }
        let hash = hash.clone();
        self.phase = Phase::Idle;
        match path {
            Some(path) => {
                self.active = Some(path);
                vec![
                    Effect::Reload,
                    Effect::Reply {
                        hash,
                        result: ResourcePackResult::SuccessfullyLoaded,
                    },
                ]
            }
            None => vec![Effect::Reply {
                hash,
                result: ResourcePackResult::FailedDownload,
            }],
        }
    }

    /// The session ended: forget the offer and drop the server pack, as the
    /// reference client does on disconnect.
    pub(super) fn end_session(&mut self) -> Vec<Effect> {
        self.phase = Phase::Idle;
        self.generation += 1;
        self.active
            .take()
            .map(|_| Effect::Reload)
            .into_iter()
            .collect()
    }

    fn start_download(&mut self, hash: String, offer: ValidatedOffer) -> Effect {
        self.generation += 1;
        let generation = self.generation;
        self.phase = Phase::Downloading { hash, generation };
        Effect::Download { generation, offer }
    }
}

#[cfg(test)]
mod tests;
