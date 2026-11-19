// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chat events, emitted on the game thread. Both are cancellable and their text
//! may be replaced by an intercepting handler (`event_set_text`).

use umicraft_modapi_sys::{UmiChatReceived, UmiChatSend, UmiEventHeader, UmiEventId};

use super::{header_for, host_event, umi_str, EventThread, HostEvent};

/// A chat line arrived from the server. Cancelling hides it; changing the JSON
/// changes what is shown.
#[repr(C)]
#[derive(Debug)]
pub struct ChatReceived {
    pub(crate) raw: UmiChatReceived,
    json: String,
}

impl ChatReceived {
    /// Builds the event from the server's chat component and its position byte.
    pub fn new(json: &str, position: u8) -> Self {
        let json = json.to_owned();
        Self {
            raw: UmiChatReceived {
                header: header_for::<UmiChatReceived>(UmiEventId::CHAT_RECEIVED),
                json: umi_str(&json),
                position,
            },
            json,
        }
    }

    /// The chat component as JSON, possibly replaced by a handler.
    pub fn json(&self) -> &str {
        &self.json
    }

    /// The protocol's position byte (chat, system message or action bar).
    pub fn position(&self) -> u8 {
        self.raw.position
    }

    /// Replaces the JSON and repoints the payload at it.
    pub fn set_json(&mut self, json: &str) {
        self.json = json.to_owned();
        self.raw.json = umi_str(&self.json);
    }
}

// Not derived: a derived clone would keep pointing at the original's buffer.
impl Clone for ChatReceived {
    fn clone(&self) -> Self {
        Self::new(&self.json, self.raw.position)
    }
}

host_event!(ChatReceived, CHAT_RECEIVED, Game, cancellable: true, mutable: true);

/// The player sends a chat line or command. Cancelling sends nothing; changing the
/// text changes what is sent.
#[repr(C)]
#[derive(Debug)]
pub struct ChatSend {
    pub(crate) raw: UmiChatSend,
    text: String,
}

impl ChatSend {
    /// Builds the event from the line as typed.
    pub fn new(text: &str) -> Self {
        let text = text.to_owned();
        Self {
            raw: UmiChatSend {
                header: header_for::<UmiChatSend>(UmiEventId::CHAT_SEND),
                text: umi_str(&text),
            },
            text,
        }
    }

    /// The line, possibly replaced by a handler.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the line and repoints the payload at it.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_owned();
        self.raw.text = umi_str(&self.text);
    }
}

// Not derived: see `ChatReceived`.
impl Clone for ChatSend {
    fn clone(&self) -> Self {
        Self::new(&self.text)
    }
}

host_event!(ChatSend, CHAT_SEND, Game, cancellable: true, mutable: true);
