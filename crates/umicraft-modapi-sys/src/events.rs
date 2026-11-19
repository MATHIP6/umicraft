//! Event identifiers and payload layouts.

/// Stable event id; ids are never reused (ADR-0087).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiEventId(pub u32);

impl UmiEventId {
    /// Fired before the client runs a game tick; payload [`UmiPreTick`].
    pub const PRE_TICK: Self = Self(1);
    /// Fired after the client ran a game tick; payload [`UmiPostTick`].
    pub const POST_TICK: Self = Self(2);
}

/// `UmiEventHeader::flags` bit: a handler may cancel the event.
pub const UMI_EVENT_CANCELLABLE: u32 = 1;
/// `UmiEventHeader::flags` bit: a handler cancelled the event.
pub const UMI_EVENT_CANCELLED: u32 = 2;

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
