// SPDX-License-Identifier: MIT OR Apache-2.0

use umicraft_modapi_sys::{UmiEventId, UMI_CHAT_POSITION_SYSTEM, UMI_EVENT_CANCELLED};

use super::*;

#[test]
fn events_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<PreTick>();
    assert_send::<PostTick>();
    assert_send::<ChatReceived>();
    assert_send::<ChatSend>();
    assert_send::<Connected>();
    assert_send::<Disconnected>();
    assert_send::<ScreenOpened>();
    assert_send::<ScreenClosed>();
}

#[test]
fn constructors_fill_header_and_accessors() {
    let tick = PreTick::new(7);
    assert_eq!(tick.tick(), 7);
    assert_eq!(tick.header().id, UmiEventId::PRE_TICK);
    assert_eq!(tick.header().flags, 0);
    assert_eq!(
        tick.header().size as usize,
        size_of::<umicraft_modapi_sys::UmiPreTick>()
    );

    let chat = ChatReceived::new("{\"text\":\"hi\"}", UMI_CHAT_POSITION_SYSTEM);
    assert_eq!(chat.json(), "{\"text\":\"hi\"}");
    assert_eq!(chat.position(), UMI_CHAT_POSITION_SYSTEM);
    assert_eq!(ChatSend::new("/home").text(), "/home");
    assert_eq!(
        Connected::new("example.org:25565").address(),
        "example.org:25565"
    );
    assert_eq!(Disconnected::new("").reason(), "");
    assert_eq!(ScreenOpened::new("chat").name(), "chat");
    assert_eq!(ScreenClosed::new("inventory").name(), "inventory");
}

#[test]
fn payload_points_into_the_owned_string_and_survives_moves() {
    let event = ChatSend::new("hello");
    let ptr_before = event.raw.text.ptr;
    let boxed = Box::new(event);
    assert_eq!(boxed.raw.text.ptr, ptr_before);
    assert_eq!(boxed.raw.text.ptr, boxed.text().as_ptr());
    assert_eq!(boxed.raw.text.len, 5);
}

#[test]
fn clone_repoints_the_payload_at_its_own_buffer() {
    let original = ChatReceived::new("{}", 0);
    let copy = original.clone();
    assert_ne!(copy.raw.json.ptr, original.raw.json.ptr);
    assert_eq!(copy.raw.json.ptr, copy.json().as_ptr());
    assert_eq!(copy.json(), "{}");

    let connected = Connected::new("a");
    let copy = connected.clone();
    assert_ne!(copy.raw.address.ptr, connected.raw.address.ptr);
    assert_eq!(copy.raw.address.ptr, copy.address().as_ptr());
}

#[test]
fn clone_of_a_changed_event_carries_the_current_text_and_no_flags() {
    let mut event = ChatSend::new("old");
    event.set_text("new");
    event.header_mut().flags = UMI_EVENT_CANCELLED;
    let copy = event.clone();
    assert_eq!(copy.text(), "new");
    assert!(!copy.is_cancelled());
}

#[test]
fn setters_replace_text_and_repoint() {
    let mut chat = ChatSend::new("a");
    chat.set_text("longer line");
    assert_eq!(chat.text(), "longer line");
    assert_eq!(chat.raw.text.ptr, chat.text().as_ptr());
    assert_eq!(chat.raw.text.len, 11);

    let mut received = ChatReceived::new("{}", 1);
    received.set_json("[]");
    assert_eq!(received.json(), "[]");
    assert_eq!(received.raw.json.len, 2);
}

#[test]
fn cancel_flag_is_read_back() {
    let mut event = ChatSend::new("x");
    assert!(!event.is_cancelled());
    event.header_mut().flags |= UMI_EVENT_CANCELLED;
    assert!(event.is_cancelled());
}

#[test]
fn header_pointer_is_the_event_address() {
    let mut event = ChatSend::new("x");
    let address = std::ptr::from_mut(&mut event) as usize;
    assert_eq!(event.header_ptr() as usize, address);
}

#[test]
fn table_matches_the_documented_threads_and_flags() {
    assert_eq!(PreTick::THREAD, EventThread::Game);
    assert_eq!(ChatSend::THREAD, EventThread::Game);
    assert_eq!(Connected::THREAD, EventThread::Render);
    assert_eq!(ScreenClosed::THREAD, EventThread::Render);
    const { assert!(ChatSend::CANCELLABLE && ChatSend::MUTABLE) };
    const { assert!(ChatReceived::CANCELLABLE && ChatReceived::MUTABLE) };
    const { assert!(!PreTick::CANCELLABLE && !Connected::MUTABLE) };
}

#[test]
fn known_ids_are_one_to_eight() {
    assert!((1..=8).all(|id| is_known(UmiEventId(id))));
    assert!(!is_known(UmiEventId(0)) && !is_known(UmiEventId(9)));
}
