// SPDX-License-Identifier: MIT OR Apache-2.0

//! Event identifiers and payload layouts.
//!
//! Strings in payloads are borrowed from the host for the duration of the
//! handler call only; a mod that keeps one must copy it.

use crate::{UmiBytes, UmiStr};

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
    /// One of the mod's own key bindings was pressed while no screen was
    /// open; payload [`UmiKeyPressed`]. Delivered only to the mod that
    /// registered the key: mods never see the player's other keystrokes.
    pub const KEY_PRESSED: Self = Self(9);
    /// A play packet arrived from the server, before the client handles it;
    /// payload [`UmiPacket`], network thread. Cancellable (the client never
    /// sees it), except `Set Compression` and `Disconnect`. Needs the
    /// `network_packets` permission. Added in API 0.4.
    pub const PACKET_RECEIVED: Self = Self(10);
    /// A play packet is about to be written to the server (the client's own,
    /// automatic replies and injected ones); payload [`UmiPacket`], network
    /// thread. Cancellable (nothing is written). Needs the `network_packets`
    /// permission. Added in API 0.4.
    pub const PACKET_SEND: Self = Self(11);
    /// Once per drawn frame, before the HUD is built; payload [`UmiFrame`],
    /// render thread. The same snapshot the built-in mods read. Not
    /// cancellable, not mutable. Added in API 1.0.
    pub const FRAME: Self = Self(12);
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

/// Payload of [`UmiEventId::KEY_PRESSED`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiKeyPressed {
    /// Common header.
    pub header: UmiEventHeader,
    /// The key id the mod gave to `register_key`.
    pub key: UmiStr,
}

/// Payload of [`UmiEventId::PACKET_RECEIVED`] and [`UmiEventId::PACKET_SEND`].
///
/// Neither event is mutable: to change a packet the client sends, cancel it
/// and inject the replacement with `UmiHost::packet_send`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiPacket {
    /// Common header.
    pub header: UmiEventHeader,
    /// The play-state packet id (protocol 47).
    pub id: u32,
    /// The packet body: no length prefix, never compressed.
    pub data: UmiBytes,
}

/// Payload of [`UmiEventId::FRAME`]: what the player sees this frame.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiFrame {
    /// Common header.
    pub header: UmiEventHeader,
    /// Monotonic time in milliseconds.
    pub now_ms: u64,
    /// Feet position, in blocks.
    pub position: [f64; 3],
    /// Frames drawn during the last second.
    pub fps: u32,
    /// Smoothed duration of a frame, in milliseconds.
    pub frame_ms: f32,
    /// View angle in degrees (Minecraft yaw).
    pub yaw: f32,
    /// View angle in degrees, positive looks down.
    pub pitch: f32,
    /// Left clicks since the previous frame.
    pub left_clicks: u32,
    /// Right clicks since the previous frame.
    pub right_clicks: u32,
    /// Round trip to the server in milliseconds, `-1` when not measured.
    pub ping_ms: i32,
    /// `1` while the player sprints, else `0`.
    pub sprinting: u8,
}
