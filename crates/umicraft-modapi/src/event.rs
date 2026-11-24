// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed views over the event payloads of the C ABI.

use umicraft_modapi_sys::{
    UmiButtonPressed, UmiChatReceived, UmiChatSend, UmiConnected, UmiDisconnected, UmiEventHeader,
    UmiEventId, UmiFrame, UmiKeyPressed, UmiModuleToggled, UmiPacket, UmiPostTick, UmiPreRender,
    UmiPreTick, UmiScreen, UMI_CHAT_POSITION_ACTION_BAR, UMI_CHAT_POSITION_CHAT,
    UMI_CHAT_POSITION_SYSTEM,
};

use crate::ffi::{bytes_of, str_of};
use crate::{host, ModError};

mod sealed {
    /// Implementors must be `#[repr(transparent)]` over a `#[repr(C)]` payload
    /// that starts with a `UmiEventHeader` carrying the event's id, because the
    /// trampoline casts the host's payload pointer to the view. Sealed so only
    /// this crate can uphold that.
    pub trait Sealed {}
}

/// An event a mod can subscribe to. Sealed: the set of events is the host's.
pub trait Event: sealed::Sealed + 'static {
    /// The ABI id the host dispatches this event under.
    const ID: UmiEventId;
}

/// Lets an intercepting handler cancel the event it is looking at.
#[derive(Debug)]
pub struct Control {
    cancellable: bool,
    cancelled: bool,
}

impl Control {
    pub(crate) fn new(cancellable: bool) -> Self {
        Self {
            cancellable,
            cancelled: false,
        }
    }

    /// Cancels the event. A no-op on events that are not cancellable, so
    /// handlers need not know which events allow it.
    pub fn cancel(&mut self) {
        self.cancelled = self.cancellable;
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// Declares a view type: transparent over the ABI payload, so the trampoline
/// can cast the host's pointer to it, and tied to its event id.
macro_rules! view {
    ($(#[$doc:meta])* $view:ident, $raw:ty, $id:expr) => {
        $(#[$doc])*
        #[repr(transparent)]
        #[derive(Debug)]
        pub struct $view($raw);

        impl sealed::Sealed for $view {}

        impl Event for $view {
            const ID: UmiEventId = $id;
        }
    };
}

view!(
    /// Fired before the client runs a game tick.
    PreTick,
    UmiPreTick,
    UmiEventId::PRE_TICK
);
view!(
    /// Fired after the client ran a game tick.
    PostTick,
    UmiPostTick,
    UmiEventId::POST_TICK
);
view!(
    /// A chat line arrived from the server. Cancellable (the line is hidden);
    /// an intercepting handler may replace its JSON.
    ChatReceived,
    UmiChatReceived,
    UmiEventId::CHAT_RECEIVED
);
view!(
    /// The player sends a chat line or command. Cancellable (nothing is sent);
    /// an intercepting handler may replace the line.
    ChatSend,
    UmiChatSend,
    UmiEventId::CHAT_SEND
);
view!(
    /// The player pressed a key binding this mod registered with
    /// [`InitCx::key`](crate::InitCx::key), with no screen open. Delivered to
    /// the registering mod only: the API has no raw key stream, so a mod
    /// cannot read what the player types (ADR-0086).
    KeyPressed,
    UmiKeyPressed,
    UmiEventId::KEY_PRESSED
);
view!(
    /// The player switched one of this mod's modules on or off in the mods menu
    /// (API 1.2). Render thread; delivered to the declaring mod only.
    ModuleToggled,
    UmiModuleToggled,
    UmiEventId::MODULE_TOGGLED
);
view!(
    /// The player clicked one of this mod's buttons in the mods menu (API 1.2).
    /// Render thread; delivered to the declaring mod only.
    ButtonPressed,
    UmiButtonPressed,
    UmiEventId::BUTTON_PRESSED
);
view!(
    /// The client joined a server.
    Connected,
    UmiConnected,
    UmiEventId::CONNECTED
);
view!(
    /// The client left a server or was kicked.
    Disconnected,
    UmiDisconnected,
    UmiEventId::DISCONNECTED
);
view!(
    /// A screen opened.
    ScreenOpened,
    UmiScreen,
    UmiEventId::SCREEN_OPENED
);
view!(
    /// A screen closed.
    ScreenClosed,
    UmiScreen,
    UmiEventId::SCREEN_CLOSED
);
view!(
    /// A play packet arrived from the server, before the client handles it.
    /// Cancellable (the client never sees it), except `Set Compression` and
    /// `Disconnect`, which the host always keeps. Network thread; needs
    /// `network_packets = true` in `mod.toml`.
    PacketReceived,
    UmiPacket,
    UmiEventId::PACKET_RECEIVED
);
view!(
    /// A play packet is about to be written to the server: the client's own,
    /// its automatic replies and packets injected with
    /// [`Packets::send`](crate::Packets::send). Cancellable (nothing is
    /// written). Network thread; needs `network_packets = true` in `mod.toml`.
    PacketSend,
    UmiPacket,
    UmiEventId::PACKET_SEND
);
view!(
    /// One rendered frame: what the player sees right now. Render thread;
    /// read-only and not cancellable. It carries no key state, so a mod
    /// cannot read what the player types (ADR-0086).
    Frame,
    UmiFrame,
    UmiEventId::FRAME
);

view!(
    /// The frame is about to be drawn, with the final camera (after every
    /// `render_camera` layer). Render thread; read-only and not cancellable.
    /// Added in API 1.1.
    PreRender,
    UmiPreRender,
    UmiEventId::PRE_RENDER
);

/// Where a received chat line shows (the protocol's position byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatPosition {
    /// A player chat message.
    Chat,
    /// A system message.
    System,
    /// The action bar above the hotbar.
    ActionBar,
    /// A position this API version does not name; the raw byte.
    Other(u8),
}

impl From<u8> for ChatPosition {
    fn from(raw: u8) -> Self {
        match raw {
            UMI_CHAT_POSITION_CHAT => Self::Chat,
            UMI_CHAT_POSITION_SYSTEM => Self::System,
            UMI_CHAT_POSITION_ACTION_BAR => Self::ActionBar,
            other => Self::Other(other),
        }
    }
}

/// Asks the host to replace the text of the event `view` is a view of.
///
/// The pointer is derived from the whole view, not from its header field, so
/// it keeps the provenance the host needs to write the rest of the payload.
fn replace_text<V: Event>(view: &mut V, text: &str) -> Result<(), ModError> {
    let event = core::ptr::from_mut(view).cast::<UmiEventHeader>();
    // The host is the authority on whether the event is mutable, so the flag
    // is not checked here and cannot disagree with it.
    host::call(|host, _| (host.event_set_text)(event, host::utf8(text))).and_then(ModError::check)
}

impl PreTick {
    /// The index of the tick about to run.
    pub fn tick(&self) -> u64 {
        self.0.tick
    }
}

impl Frame {
    /// Monotonic time in milliseconds.
    pub fn now_ms(&self) -> u64 {
        self.0.now_ms
    }

    /// The player's feet position, in blocks.
    pub fn position(&self) -> [f64; 3] {
        self.0.position
    }

    /// Frames drawn during the last second.
    pub fn fps(&self) -> u32 {
        self.0.fps
    }

    /// Smoothed duration of a frame, in milliseconds.
    pub fn frame_ms(&self) -> f32 {
        self.0.frame_ms
    }

    /// The view yaw in degrees (Minecraft yaw).
    pub fn yaw(&self) -> f32 {
        self.0.yaw
    }

    /// The view pitch in degrees; positive looks down.
    pub fn pitch(&self) -> f32 {
        self.0.pitch
    }

    /// Left clicks since the previous frame.
    pub fn left_clicks(&self) -> u32 {
        self.0.left_clicks
    }

    /// Right clicks since the previous frame.
    pub fn right_clicks(&self) -> u32 {
        self.0.right_clicks
    }

    /// The round trip to the server in milliseconds, `None` while it has not
    /// been measured (singleplayer or just connected).
    pub fn ping_ms(&self) -> Option<u32> {
        u32::try_from(self.0.ping_ms).ok()
    }

    /// Whether the player is sprinting.
    pub fn sprinting(&self) -> bool {
        self.0.sprinting != 0
    }

    /// The camera eye this frame is drawn from, in blocks (it differs from the
    /// player's eye in the third person or under a camera mod).
    pub fn eye(&self) -> [f64; 3] {
        self.0.eye
    }

    /// The integer corner GPU positions are relative to; see
    /// [`view_proj`](Self::view_proj).
    pub fn render_origin(&self) -> [f64; 3] {
        self.0.render_origin
    }

    /// The column-major view-projection of the frame, for points relative to
    /// [`render_origin`](Self::render_origin): `clip = view_proj * (p - origin, 1)`,
    /// depth 0..1. It holds the effective field of view and the view bobbing.
    pub fn view_proj(&self) -> [[f32; 4]; 4] {
        self.0.view_proj
    }

    /// The framebuffer size, in physical pixels.
    pub fn viewport(&self) -> [u32; 2] {
        self.0.viewport
    }

    /// Physical pixels per GUI pixel (the unit HUD widgets draw in).
    pub fn gui_scale(&self) -> f32 {
        self.0.gui_scale
    }

    /// The effective vertical field of view in degrees: the option times the
    /// zoom, sprint and flight modifiers.
    pub fn fov_y(&self) -> f32 {
        self.0.fov_y
    }

    /// The viewport width over its height.
    pub fn aspect(&self) -> f32 {
        self.0.aspect
    }

    /// The column-major camera-space view bobbing, identity when still.
    pub fn view_bob(&self) -> [[f32; 4]; 4] {
        self.0.view_bob
    }

    /// Where `point` (in blocks) is on the screen, in GUI pixels from the
    /// top-left corner, for a widget anchored with
    /// [`Anchor::Screen`](crate::Anchor::Screen); `None` when the point is behind
    /// the camera or outside the viewport.
    ///
    /// It uses the exact camera of this frame: the render origin in `f64`, the
    /// effective field of view and the view bobbing.
    pub fn world_to_screen(&self, point: [f64; 3]) -> Option<[f32; 2]> {
        crate::projection::world_to_screen(&self.0, point)
    }
}

impl PreRender {
    /// The camera eye, in blocks.
    pub fn eye(&self) -> [f64; 3] {
        self.0.camera.eye
    }

    /// The view yaw in degrees (Minecraft yaw).
    pub fn yaw(&self) -> f32 {
        self.0.camera.yaw
    }

    /// The view pitch in degrees, positive looks down.
    pub fn pitch(&self) -> f32 {
        self.0.camera.pitch
    }

    /// The effective vertical field of view, in degrees.
    pub fn fov_y(&self) -> f32 {
        self.0.camera.fov_y
    }

    /// The viewport width over its height.
    pub fn aspect(&self) -> f32 {
        self.0.camera.aspect
    }

    /// The column-major camera-space view bobbing.
    pub fn view_bob(&self) -> [[f32; 4]; 4] {
        self.0.camera.view_bob
    }
}

impl PostTick {
    /// The index of the tick that just ran.
    pub fn tick(&self) -> u64 {
        self.0.tick
    }
}

impl ChatReceived {
    /// The chat component as JSON, exactly as the server sent it (or as an
    /// earlier handler replaced it). Reads as empty if the host sent bad UTF-8.
    pub fn json(&self) -> &str {
        str_of(&self.0.json)
    }

    /// Where the line shows.
    pub fn position(&self) -> ChatPosition {
        self.0.position.into()
    }

    /// Replaces the JSON; later handlers and the client see the new text.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` when called from an observer
    /// (observers get a copy, so the event is not `MUTABLE`).
    pub fn set_json(&mut self, json: &str) -> Result<(), ModError> {
        replace_text(self, json)
    }
}

impl ChatSend {
    /// The line as typed, commands included (`/...`).
    pub fn text(&self) -> &str {
        str_of(&self.0.text)
    }

    /// Replaces the line that will be sent.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` when called from an observer.
    pub fn set_text(&mut self, text: &str) -> Result<(), ModError> {
        replace_text(self, text)
    }
}

impl KeyPressed {
    /// The id the mod gave the binding in [`InitCx::key`](crate::InitCx::key).
    pub fn key(&self) -> &str {
        str_of(&self.0.key)
    }
}

impl ModuleToggled {
    /// The key of the module, as [`Module::key`](crate::Module::key) gives it.
    pub fn module(&self) -> &str {
        str_of(&self.0.module)
    }

    /// The new state of the switch.
    pub fn enabled(&self) -> bool {
        self.0.enabled != 0
    }
}

impl ButtonPressed {
    /// The module whose card holds the button, empty for the mod's own card.
    pub fn module(&self) -> &str {
        str_of(&self.0.module)
    }

    /// The key the mod gave the button.
    pub fn key(&self) -> &str {
        str_of(&self.0.key)
    }
}

impl Connected {
    /// The server address as the player entered it (`host[:port]`).
    pub fn address(&self) -> &str {
        str_of(&self.0.address)
    }
}

impl Disconnected {
    /// Why the connection ended, as plain text; empty when the player left.
    pub fn reason(&self) -> &str {
        str_of(&self.0.reason)
    }
}

/// Declares `name()` on a screen view; both screen events share one payload.
macro_rules! screen_name {
    ($($view:ident),+) => {$(
        impl $view {
            /// The stable screen name (`chat`, `inventory`, `game_menu`, ...).
            pub fn name(&self) -> &str {
                str_of(&self.0.name)
            }
        }
    )+};
}

screen_name!(ScreenOpened, ScreenClosed);

/// Declares `id()` and `data()` on a packet view; both packet events share one
/// payload.
macro_rules! packet_fields {
    ($($view:ident),+) => {$(
        impl $view {
            /// The play-state packet id (protocol 47). A host sending a value
            /// above 255, which no packet has, reads as `u8::MAX`: that is a
            /// wrong id either way, and it never aliases a real low id the way
            /// truncation would.
            pub fn id(&self) -> u8 {
                u8::try_from(self.0.id).unwrap_or(u8::MAX)
            }

            /// The packet body: no id and no length prefix, never compressed.
            /// Borrowed from the event, so copy it to keep it. Empty for a
            /// packet without a body.
            pub fn data(&self) -> &[u8] {
                bytes_of(&self.0.data)
            }
        }
    )+};
}

packet_fields!(PacketReceived, PacketSend);
