// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use umicraft_protocol::packets::play::serverbound::{Animation, Chat};

use super::*;
use crate::manifest::Permissions;
use crate::ui::shared::HostShared;
use crate::ui::PACKET_OUTBOX_CAPACITY;

fn permitted() -> Permissions {
    Permissions {
        network_packets: true,
        ..Permissions::default()
    }
}

fn context(permissions: Permissions) -> Arc<ModContext> {
    let cx = ModContext::with_shared(
        "net".to_owned().try_into().unwrap(),
        permissions,
        Arc::default(),
    );
    cx.finish_init(true);
    cx
}

fn body_of(packet: &PlayServerbound) -> Vec<u8> {
    let mut body = Vec::new();
    packet.encode_body(&mut body).unwrap();
    body
}

fn chat(text: &str) -> (u32, Vec<u8>) {
    let packet = PlayServerbound::Chat(Chat {
        message: text.to_owned(),
    });
    (u32::from(packet.packet_id()), body_of(&packet))
}

fn animation() -> (u32, Vec<u8>) {
    let packet = PlayServerbound::Animation(Animation);
    (u32::from(packet.packet_id()), body_of(&packet))
}

fn drained(cx: &ModContext) -> Vec<PlayServerbound> {
    let mut out = Vec::new();
    PacketOutbox::new(cx.shared.packet_inbox()).drain(&mut out);
    out
}

#[test]
fn a_valid_packet_is_queued_and_counted() {
    let cx = context(permitted());
    let (id, body) = chat("hello");
    assert_eq!(cx.packet_send(id, &body), UmiStatus::OK);
    let (id, body) = animation();
    assert_eq!(cx.packet_send(id, &body), UmiStatus::OK);

    assert_eq!(cx.stats().injected, 2);
    let queued = drained(&cx);
    assert_eq!(queued.len(), 2);
    assert!(matches!(&queued[0], PlayServerbound::Chat(c) if c.message == "hello"));
    assert!(matches!(queued[1], PlayServerbound::Animation(_)));
}

#[test]
fn without_the_permission_the_call_is_denied() {
    let cx = context(Permissions::default());
    let (id, body) = animation();
    assert_eq!(cx.packet_send(id, &body), UmiStatus::DENIED);
    assert!(drained(&cx).is_empty());
    assert_eq!(cx.stats().injected, 0);
}

#[test]
fn before_init_ended_switched_off_or_failed_the_call_is_denied() {
    let (id, body) = animation();

    let early = ModContext::with_shared(
        "early".to_owned().try_into().unwrap(),
        permitted(),
        Arc::default(),
    );
    assert_eq!(early.packet_send(id, &body), UmiStatus::DENIED);

    let off = context(permitted());
    off.set_wanted(false);
    assert_eq!(off.packet_send(id, &body), UmiStatus::DENIED);

    let failed = context(permitted());
    failed.fail("test");
    assert_eq!(failed.packet_send(id, &body), UmiStatus::DENIED);
}

#[test]
fn server_compat_denies_injection_and_off_allows_it_again() {
    let shared = Arc::<HostShared>::default();
    let cx = ModContext::with_shared(
        "net".to_owned().try_into().unwrap(),
        permitted(),
        shared.clone(),
    );
    cx.finish_init(true);
    let (id, body) = animation();

    shared.set_server_compat(true);
    assert_eq!(cx.packet_send(id, &body), UmiStatus::DENIED);
    shared.set_server_compat(false);
    assert_eq!(cx.packet_send(id, &body), UmiStatus::OK);
}

#[test]
fn a_bad_id_or_body_is_an_invalid_argument() {
    let cx = context(permitted());
    let (id, body) = chat("x");

    assert_eq!(cx.packet_send(0x100, &[]), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(
        cx.packet_send(0x7f, &[]),
        UmiStatus::INVALID_ARGUMENT,
        "unknown id"
    );
    assert_eq!(
        cx.packet_send(id, &body[..body.len() - 1]),
        UmiStatus::INVALID_ARGUMENT,
        "truncated"
    );
    let mut long = body.clone();
    long.push(0);
    assert_eq!(
        cx.packet_send(id, &long),
        UmiStatus::INVALID_ARGUMENT,
        "leftover bytes"
    );
    let (id, _) = animation();
    assert_eq!(
        cx.packet_send(id, &[1]),
        UmiStatus::INVALID_ARGUMENT,
        "leftover bytes"
    );
    assert!(drained(&cx).is_empty());
}

#[test]
fn a_full_queue_answers_busy_and_counts_the_refusal() {
    let cx = context(permitted());
    let (id, body) = animation();
    for _ in 0..PACKET_OUTBOX_CAPACITY {
        assert_eq!(cx.packet_send(id, &body), UmiStatus::OK);
    }
    assert_eq!(cx.packet_send(id, &body), UmiStatus::BUSY);
    assert_eq!(cx.stats().dropped, 1);
    assert_eq!(cx.stats().injected, PACKET_OUTBOX_CAPACITY as u64);

    assert_eq!(drained(&cx).len(), PACKET_OUTBOX_CAPACITY);
    assert_eq!(cx.packet_send(id, &body), UmiStatus::OK, "room again");
}

#[test]
fn the_outbox_drains_in_order_clears_and_reports_emptiness() {
    let cx = context(permitted());
    let outbox = PacketOutbox::new(cx.shared.packet_inbox());
    assert!(outbox.is_empty());

    let (id, body) = chat("first");
    cx.packet_send(id, &body);
    let (id, body) = chat("second");
    cx.packet_send(id, &body);
    assert!(!outbox.is_empty());

    let mut out = vec![PlayServerbound::Animation(Animation)];
    outbox.drain(&mut out);
    assert_eq!(out.len(), 3, "appends");
    assert!(matches!(&out[1], PlayServerbound::Chat(c) if c.message == "first"));
    assert!(outbox.is_empty());

    let (id, body) = animation();
    cx.packet_send(id, &body);
    outbox.clone().clear();
    assert!(outbox.is_empty());
}
