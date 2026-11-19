// SPDX-License-Identifier: MIT OR Apache-2.0

//! Event identifiers and payload layouts.
//!
//! Strings in payloads are borrowed from the host for the duration of the
//! handler call only; a mod that keeps one must copy it.

use crate::UmiStr;

/// Stable event id; ids are never reused (ADR-0087).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiEventId(pub u32);

impl UmiEventId {
    /// Fired before the client runs a game tick; payload [`UmiPreTick`].
    pub const PRE_TICK: Self = Self(1);
    /// Fired after the client ran a game tick; payload [`UmiPostTick`].
    pub const POST_TICK: Self = Self(2);
    /// A chat line arrived from the server; payload [`UmiChatReceived`].
    /// Cancellable (the line is not shown) and its text replaceable.
    pub const CHAT_RECEIVED: Self = Self(3);
    /// The player sends a chat line or command; payload [`UmiChatSend`].
    /// Cancellable (nothing is sent) and its text replaceable.
    pub const CHAT_SEND: Self = Self(4);
    /// The client joined a server; payload [`UmiConnected`].
    pub const CONNECTED: Self = Self(5);
    /// The client left a server or was kicked; payload [`UmiDisconnected`].
    pub const DISCONNECTED: Self = Self(6);
    /// A screen opened; payload [`UmiScreen`].
    pub const SCREEN_OPENED: Self = Self(7);
    /// A screen closed; payload [`UmiScreen`].
    pub const SCREEN_CLOSED: Self = Self(8);
}

/// `UmiEventHeader::flags` bit: a handler may cancel the event.
pub const UMI_EVENT_CANCELLABLE: u32 = 1;
/// `UmiEventHeader::flags` bit: a handler cancelled the event.
pub const UMI_EVENT_CANCELLED: u32 = 2;
/// `UmiEventHeader::flags` bit: the payload may be changed through host
/// functions such as `event_set_text`. Set only for intercept handlers; an
/// observer gets a copy, so changing it would have no effect.
pub const UMI_EVENT_MUTABLE: u32 = 4;

/// First field of every event payload.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiEventHeader {
    /// Size in bytes of the whole payload, so later minors can append fields.
    pub size: u32,
    /// Which event this payload belongs to.
    pub id: UmiEventId,
    /// Bit set of `UMI_EVENT_*` flags.
    pub flags: u32,
}

/// Payload of [`UmiEventId::PRE_TICK`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiPreTick {
    /// Common header.
    pub header: UmiEventHeader,
    /// Index of the tick about to run.
    pub tick: u64,
}

/// Payload of [`UmiEventId::POST_TICK`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiPostTick {
    /// Common header.
    pub header: UmiEventHeader,
    /// Index of the tick that just ran.
    pub tick: u64,
}

/// Payload of [`UmiEventId::CHAT_RECEIVED`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiChatReceived {
    /// Common header.
    pub header: UmiEventHeader,
    /// The chat component as JSON, exactly as the server sent it.
    pub json: UmiStr,
    /// Where it shows: [`UMI_CHAT_POSITION_CHAT`], [`UMI_CHAT_POSITION_SYSTEM`]
    /// or [`UMI_CHAT_POSITION_ACTION_BAR`] (the protocol's position byte).
    pub position: u8,
}

/// [`UmiChatReceived::position`]: a player chat message.
pub const UMI_CHAT_POSITION_CHAT: u8 = 0;
/// [`UmiChatReceived::position`]: a system message.
pub const UMI_CHAT_POSITION_SYSTEM: u8 = 1;
/// [`UmiChatReceived::position`]: the action bar above the hotbar.
pub const UMI_CHAT_POSITION_ACTION_BAR: u8 = 2;

/// Payload of [`UmiEventId::CHAT_SEND`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiChatSend {
    /// Common header.
    pub header: UmiEventHeader,
    /// The line as typed, commands included (`/…`).
    pub text: UmiStr,
}

/// Payload of [`UmiEventId::CONNECTED`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiConnected {
    /// Common header.
    pub header: UmiEventHeader,
    /// The server address as the player entered it (`host[:port]`).
    pub address: UmiStr,
}

/// Payload of [`UmiEventId::DISCONNECTED`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiDisconnected {
    /// Common header.
    pub header: UmiEventHeader,
    /// Why the connection ended, as plain text; empty when the player left.
    pub reason: UmiStr,
}

/// Payload of [`UmiEventId::SCREEN_OPENED`] and [`UmiEventId::SCREEN_CLOSED`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiScreen {
    /// Common header.
    pub header: UmiEventHeader,
    /// Stable screen name (`chat`, `inventory`, `game_menu`, …); the list is
    /// in `docs/modding/events.md` and names are never reused.
    pub name: UmiStr,
}
