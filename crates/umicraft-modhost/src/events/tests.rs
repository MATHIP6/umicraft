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
    assert_send::<KeyPressed>();
    assert_send::<PacketReceived>();
    assert_send::<PacketSend>();
    assert_send::<Frame>();
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
fn known_ids_are_one_to_thirteen() {
    // 13 is `PRE_RENDER`, added in API 1.1.
    assert!((1..=13).all(|id| is_known(UmiEventId(id))));
    assert!(!is_known(UmiEventId(0)) && !is_known(UmiEventId(14)));
}

#[test]
fn packet_events_carry_id_and_body_and_clone_repoints() {
    let original = PacketReceived::new(0x0A, &[1, 2, 3]);
    assert_eq!(original.id(), 0x0A);
    assert_eq!(original.data(), [1, 2, 3]);
    assert_eq!(original.header().id, UmiEventId::PACKET_RECEIVED);
    assert_eq!(
        original.header().size as usize,
        size_of::<umicraft_modapi_sys::UmiPacket>()
    );
    assert_eq!(original.raw.data.ptr, original.data().as_ptr());
    assert_eq!(original.raw.data.len, 3);

    let copy = original.clone();
    assert_ne!(copy.raw.data.ptr, original.raw.data.ptr);
    assert_eq!(copy.raw.data.ptr, copy.data().as_ptr());
    assert_eq!(copy.data(), [1, 2, 3]);

    let send = PacketSend::new(0x01, &[]);
    assert_eq!(send.header().id, UmiEventId::PACKET_SEND);
    assert!(send.raw.data.ptr.is_null() && send.raw.data.len == 0);
    assert!(send.data().is_empty());
}

#[test]
fn packet_events_are_cancellable_immutable_network_events() {
    assert_eq!(PacketReceived::THREAD, EventThread::Network);
    assert_eq!(PacketSend::THREAD, EventThread::Network);
    const { assert!(PacketReceived::CANCELLABLE && PacketSend::CANCELLABLE) };
    const { assert!(!PacketReceived::MUTABLE && !PacketSend::MUTABLE) };
    assert_eq!(PacketSend::new(1, &[]).target(), None);
}

#[test]
fn key_pressed_is_aimed_at_one_mod_and_the_others_at_everyone() {
    let id: ModId = "target".to_owned().try_into().unwrap();
    let event = KeyPressed::new(id.clone(), "jump");
    assert_eq!(event.target(), Some(&id));
    assert_eq!(event.clone().target(), Some(&id));
    assert_eq!(event.key(), "jump");
    assert_eq!(event.header().id, UmiEventId::KEY_PRESSED);
    assert_eq!(KeyPressed::THREAD, EventThread::Render);
    assert_eq!(PreTick::new(1).target(), None);
    assert_eq!(ChatSend::new("x").target(), None);
}

fn facts() -> FrameFacts {
    FrameFacts {
        now_ms: 123_456,
        position: [1.5, 64.0, -3.25],
        fps: 144,
        frame_ms: 6.9,
        yaw: -90.0,
        pitch: 12.5,
        left_clicks: 2,
        right_clicks: 1,
        ping_ms: Some(42),
        sprinting: true,
        camera: FrameCamera {
            eye: [1.5, 65.62, -3.25],
            render_origin: [1.0, 65.0, -4.0],
            viewport: [1920, 1080],
            gui_scale: 3.0,
            fov_y: 77.0,
            aspect: 1920.0 / 1080.0,
            ..FrameCamera::default()
        },
    }
}

#[test]
fn frame_fills_header_payload_and_accessors() {
    let frame = Frame::new(facts());
    assert_eq!(frame.header().id, UmiEventId::FRAME);
    assert_eq!(frame.header().flags, 0);
    assert_eq!(
        frame.header().size as usize,
        size_of::<umicraft_modapi_sys::UmiFrame>()
    );
    assert_eq!(frame.facts(), facts());
    assert_eq!(frame.now_ms(), 123_456);
    assert_eq!(frame.position(), [1.5, 64.0, -3.25]);
    assert_eq!((frame.fps(), frame.frame_ms()), (144, 6.9));
    assert_eq!((frame.yaw(), frame.pitch()), (-90.0, 12.5));
    assert_eq!((frame.left_clicks(), frame.right_clicks()), (2, 1));
    assert_eq!(frame.ping_ms(), Some(42));
    assert!(frame.sprinting());
    assert_eq!(frame.raw.sprinting, 1);
}

#[test]
fn frame_ping_unknown_is_minus_one_in_the_payload() {
    let unknown = Frame::new(FrameFacts {
        ping_ms: None,
        sprinting: false,
        ..facts()
    });
    assert_eq!(unknown.raw.ping_ms, -1);
    assert_eq!(unknown.raw.sprinting, 0);
    assert_eq!(unknown.ping_ms(), None);
    assert!(!unknown.sprinting());

    let huge = Frame::new(FrameFacts {
        ping_ms: Some(u32::MAX),
        ..facts()
    });
    assert_eq!(
        huge.raw.ping_ms, -1,
        "a ping the ABI cannot hold is unknown"
    );
    let zero = Frame::new(FrameFacts {
        ping_ms: Some(0),
        ..facts()
    });
    assert_eq!(zero.ping_ms(), Some(0));
}

#[test]
fn frame_is_a_render_event_that_mods_cannot_cancel_or_change() {
    assert_eq!(Frame::THREAD, EventThread::Render);
    const { assert!(!Frame::CANCELLABLE && !Frame::MUTABLE) };
    assert_eq!(Frame::ID, UmiEventId::FRAME);
}
