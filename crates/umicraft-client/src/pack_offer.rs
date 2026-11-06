// SPDX-License-Identifier: MIT OR Apache-2.0

//! The server resource pack offer on its way from the game thread to the
//! window (NET-11).
//!
//! The network layer decodes `Resource Pack Send` and the game thread
//! forwards it as a [`ServerPackRequest`] in a render event. The window
//! decides what to do with it (`window::server_pack`) and answers with
//! [`GameEvent::ServerPackStatus`](crate::game::GameEvent), which the game
//! thread turns into the serverbound `Resource Pack Status`.

use umicraft_assets::server_pack::ServerPackOffer;
use umicraft_game::server::ResourcePackOffer;

/// A resource pack the server offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPackRequest {
    /// Where to get the pack (`http`, `https` or `level://`).
    pub url: String,
    /// The SHA-1 the server sent; may be empty or malformed. It is also the
    /// key of every status reply.
    pub hash: String,
}

impl From<&ResourcePackOffer> for ServerPackRequest {
    fn from(offer: &ResourcePackOffer) -> Self {
        Self {
            url: offer.url.clone(),
            hash: offer.hash.clone(),
        }
    }
}

impl From<ServerPackRequest> for ServerPackOffer {
    fn from(request: ServerPackRequest) -> Self {
        Self {
            url: request.url,
            hash: request.hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_converts_to_an_asset_offer_unchanged() {
        let request = ServerPackRequest {
            url: "https://example.net/p.zip".to_string(),
            hash: "abc".to_string(),
        };
        let offer = ServerPackOffer::from(request.clone());
        assert_eq!((offer.url, offer.hash), (request.url, request.hash));
    }
}
