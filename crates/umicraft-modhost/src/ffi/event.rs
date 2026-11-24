// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of host events: the `Send` claim for events that carry strings and
//! the `event_set_text` host function, which turns an ABI pointer back into the host
//! event it points into (ADR-0087).
#![allow(unsafe_code)]

use std::panic::{catch_unwind, AssertUnwindSafe};

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiStatus, UmiStr, UMI_EVENT_MUTABLE};

use super::bytes_of;
use crate::events::{
    ButtonPressed, ChatReceived, ChatSend, Connected, Disconnected, KeyPressed, ModuleToggled,
    PacketReceived, PacketSend, ScreenClosed, ScreenOpened,
};

/// Declares host events `Send`: the raw pointers in their payload target the `String`
/// the same struct owns.
macro_rules! send_events {
    ($($name:ty),+) => {$(
        // SAFETY: the only `!Send` part is the `UmiStr` in the payload, whose pointer
        // targets the heap buffer of a `String` owned by the same value. That buffer
        // moves with the value and nothing else aliases it, so sending the value
        // sends everything its pointers reach.
        unsafe impl Send for $name {}
    )+};
}

send_events!(
    ChatReceived,
    ChatSend,
    Connected,
    Disconnected,
    KeyPressed,
    ModuleToggled,
    ButtonPressed,
    ScreenOpened,
    ScreenClosed
);

/// Declares the packet events `Send`: like the string events, with a `Vec<u8>`.
macro_rules! send_packet_events {
    ($($name:ty),+) => {$(
        // SAFETY: the only `!Send` part is the `UmiBytes` in the payload, whose pointer
        // targets the heap buffer of a `Vec<u8>` owned by the same value. That buffer
        // moves with the value and nothing else aliases it, so sending the value
        // sends everything its pointer reaches.
        unsafe impl Send for $name {}
    )+};
}

send_packet_events!(PacketReceived, PacketSend);

/// Reborrows the host event of type `T` that `header` points at.
///
/// # Safety
///
/// `header` must be the address of a live `T` (see the cast below), with no other
/// reference to it in use.
unsafe fn event_as<'a, T>(header: *mut UmiEventHeader) -> &'a mut T {
    // SAFETY: forwarded to the caller.
    unsafe { &mut *header.cast::<T>() }
}

/// `event_set_text` as seen by mods; policy is in [`set_text`].
pub(super) extern "C" fn host_event_set_text(
    event: *mut UmiEventHeader,
    text: UmiStr,
) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(|| set_text(event, text))).unwrap_or(UmiStatus::PANICKED)
}

fn set_text(event: *mut UmiEventHeader, text: UmiStr) -> UmiStatus {
    // SAFETY: a non-null `event` is the pointer the host gave to the running handler
    // (see `HostEvent::header_ptr`), so it addresses a live event whose first field is
    // the header. Only a shared read of the header is made here.
    let Some(header) = (unsafe { event.as_ref() }) else {
        return UmiStatus::INVALID_ARGUMENT;
    };
    if header.flags & UMI_EVENT_MUTABLE == 0 {
        return UmiStatus::DENIED;
    }
    // Copied out so the shared borrow of the header ends before the mutable one below.
    let id = header.id;
    let Ok(text) = std::str::from_utf8(bytes_of(text, usize::MAX)) else {
        return UmiStatus::INVALID_ARGUMENT;
    };
    match id {
        // SAFETY: `MUTABLE` is set only by the host's intercept adapter, for the
        // duration of one handler call on an event it owns and has lent exclusively
        // to that call. `ChatSend` is `repr(C)` with the payload (hence the header) as
        // its first field, and the id says the event is a `ChatSend`, so the cast
        // recovers the original type. The `Send` argument above applies as well.
        UmiEventId::CHAT_SEND => unsafe { event_as::<ChatSend>(event) }.set_text(text),
        // SAFETY: as for `CHAT_SEND`, with `ChatReceived`.
        UmiEventId::CHAT_RECEIVED => unsafe { event_as::<ChatReceived>(event) }.set_json(text),
        _ => return UmiStatus::INVALID_ARGUMENT,
    }
    UmiStatus::OK
}

/// Test helpers: what a mod's trampoline does to the header, done here because only
/// this module may dereference ABI pointers.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// Reads the flags of the event behind `event`.
    pub(crate) fn flags_of(event: *mut UmiEventHeader) -> u32 {
        // SAFETY: tests pass the pointer a dispatcher handed to a handler.
        unsafe { (*event).flags }
    }

    /// Copies the text of a `UmiStr` a host function passed to a fake mod.
    pub(crate) fn text(s: UmiStr) -> String {
        String::from_utf8_lossy(bytes_of(s, usize::MAX)).into_owned()
    }

    /// Sets `flag` in the header of the event behind `event`.
    pub(crate) fn set_flag(event: *mut UmiEventHeader, flag: u32) {
        // SAFETY: as for `flags_of`; the handler has exclusive use of the event.
        unsafe { (*event).flags |= flag };
    }
}

#[cfg(test)]
mod tests {
    use umicraft_modapi_sys::UMI_EVENT_MUTABLE;

    use super::*;
    use crate::events::HostEvent;
    use crate::ffi::host_table;

    fn set(event: *mut UmiEventHeader, text: UmiStr) -> UmiStatus {
        (host_table().event_set_text)(event, text)
    }

    fn mutable_chat() -> ChatSend {
        let mut event = ChatSend::new("old");
        event.header_mut().flags |= UMI_EVENT_MUTABLE;
        event
    }

    #[test]
    fn set_text_replaces_text_when_mutable() {
        let mut event = mutable_chat();
        assert_eq!(
            set(event.header_ptr(), UmiStr::from_static("new")),
            UmiStatus::OK
        );
        assert_eq!(event.text(), "new");
    }

    #[test]
    fn set_text_refuses_null_immutable_wrong_id_and_bad_utf8() {
        assert_eq!(
            set(std::ptr::null_mut(), UmiStr::from_static("x")),
            UmiStatus::INVALID_ARGUMENT
        );

        let mut immutable = ChatSend::new("old");
        assert_eq!(
            set(immutable.header_ptr(), UmiStr::from_static("x")),
            UmiStatus::DENIED
        );
        assert_eq!(immutable.text(), "old");

        let mut tick = crate::events::PreTick::new(1);
        tick.header_mut().flags |= UMI_EVENT_MUTABLE;
        assert_eq!(
            set(tick.header_ptr(), UmiStr::from_static("x")),
            UmiStatus::INVALID_ARGUMENT
        );

        let mut event = mutable_chat();
        let bad = [0xffu8, 0xfe];
        let text = UmiStr {
            ptr: bad.as_ptr(),
            len: 2,
        };
        assert_eq!(set(event.header_ptr(), text), UmiStatus::INVALID_ARGUMENT);
        assert_eq!(event.text(), "old");
    }

    #[test]
    fn set_text_accepts_an_empty_replacement() {
        let mut event = mutable_chat();
        assert_eq!(set(event.header_ptr(), UmiStr::EMPTY), UmiStatus::OK);
        assert_eq!(event.text(), "");
    }
}
