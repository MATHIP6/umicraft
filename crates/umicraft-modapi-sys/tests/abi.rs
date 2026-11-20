// SPDX-License-Identifier: MIT OR Apache-2.0

//! Layout, version and symbol checks that pin the ABI of ADR-0087.

use core::mem::{align_of, offset_of, size_of};

use umicraft_modapi_sys::*;

#[cfg(target_pointer_width = "64")]
#[test]
fn layouts_are_stable_on_64_bit() {
    assert_eq!((size_of::<UmiStr>(), align_of::<UmiStr>()), (16, 8));
    assert_eq!(offset_of!(UmiStr, ptr), 0);
    assert_eq!(offset_of!(UmiStr, len), 8);

    assert_eq!((size_of::<UmiVersion>(), align_of::<UmiVersion>()), (4, 2));
    assert_eq!(offset_of!(UmiVersion, major), 0);
    assert_eq!(offset_of!(UmiVersion, minor), 2);

    assert_eq!(size_of::<UmiStatus>(), 4);
    assert_eq!(size_of::<UmiModHandle>(), size_of::<usize>());
    assert_eq!(size_of::<UmiEventId>(), 4);
    assert_eq!(size_of::<UmiMode>(), 4);
    assert_eq!(size_of::<UmiLevel>(), 4);

    assert_eq!(
        (size_of::<UmiEventHeader>(), align_of::<UmiEventHeader>()),
        (12, 4)
    );
    assert_eq!(offset_of!(UmiEventHeader, size), 0);
    assert_eq!(offset_of!(UmiEventHeader, id), 4);
    assert_eq!(offset_of!(UmiEventHeader, flags), 8);

    for (size, align, header, tick) in [
        (
            size_of::<UmiPreTick>(),
            align_of::<UmiPreTick>(),
            offset_of!(UmiPreTick, header),
            offset_of!(UmiPreTick, tick),
        ),
        (
            size_of::<UmiPostTick>(),
            align_of::<UmiPostTick>(),
            offset_of!(UmiPostTick, header),
            offset_of!(UmiPostTick, tick),
        ),
    ] {
        assert_eq!((size, align, header, tick), (24, 8, 0, 16));
    }

    assert_eq!((size_of::<UmiHost>(), align_of::<UmiHost>()), (184, 8));
    assert_eq!(offset_of!(UmiHost, size), 0);
    assert_eq!(offset_of!(UmiHost, version), 8);
    assert_eq!(offset_of!(UmiHost, log), 16);
    assert_eq!(offset_of!(UmiHost, subscribe), 24);
}

#[test]
fn version_compatibility_rules() {
    let v = |major, minor| UmiVersion { major, minor };
    let table = [
        // (host, mod, accepted)
        (v(0, 1), v(0, 1), true),
        (v(0, 2), v(0, 1), false),
        (v(0, 1), v(0, 2), false),
        (v(0, 1), v(1, 1), false),
        (v(1, 0), v(0, 0), false),
        (v(1, 2), v(1, 2), true),
        (v(1, 3), v(1, 2), true),
        (v(1, 1), v(1, 2), false),
        (v(2, 0), v(1, 0), false),
        (v(1, 0), v(2, 0), false),
    ];
    for (host, module, expected) in table {
        assert_eq!(host.accepts(module), expected, "{host:?} vs {module:?}");
    }
    assert!(UMI_API_VERSION.accepts(UMI_API_VERSION));
    assert_eq!(UMI_API_VERSION, v(0, 6));
}

#[test]
fn symbols_are_nul_terminated_adr_names() {
    let cases: [(&[u8], &str); 3] = [
        (UMI_MOD_API_VERSION_SYMBOL, "umi_mod_api_version"),
        (UMI_MOD_INIT_SYMBOL, "umi_mod_init"),
        (UMI_MOD_SHUTDOWN_SYMBOL, "umi_mod_shutdown"),
    ];
    for (symbol, name) in cases {
        assert_eq!(symbol.last(), Some(&0));
        assert_eq!(&symbol[..symbol.len() - 1], name.as_bytes());
    }
}

#[test]
fn status_values_and_ids_match_the_contract() {
    let statuses = [
        UmiStatus::OK,
        UmiStatus::ERROR,
        UmiStatus::PANICKED,
        UmiStatus::UNSUPPORTED,
        UmiStatus::INVALID_ARGUMENT,
        UmiStatus::DENIED,
        UmiStatus::CONFLICT,
    ];
    for (i, status) in statuses.iter().enumerate() {
        assert_eq!(status.0, i as i32);
    }
    assert!(UmiStatus::OK.is_ok());
    assert!(!UmiStatus(99).is_ok());
    assert_eq!((UmiMode::OBSERVE.0, UmiMode::INTERCEPT.0), (0, 1));
    assert_eq!(
        [
            UmiLevel::ERROR,
            UmiLevel::WARN,
            UmiLevel::INFO,
            UmiLevel::DEBUG,
            UmiLevel::TRACE
        ]
        .map(|l| l.0),
        [1, 2, 3, 4, 5]
    );
    assert_eq!((UmiEventId::PRE_TICK.0, UmiEventId::POST_TICK.0), (1, 2));
    assert_eq!(
        (
            UMI_EVENT_CANCELLABLE,
            UMI_EVENT_CANCELLED,
            UMI_EVENT_MUTABLE
        ),
        (1, 2, 4)
    );
    let ids = [
        UmiEventId::CHAT_RECEIVED,
        UmiEventId::CHAT_SEND,
        UmiEventId::CONNECTED,
        UmiEventId::DISCONNECTED,
        UmiEventId::SCREEN_OPENED,
        UmiEventId::SCREEN_CLOSED,
    ];
    assert_eq!(ids.map(|id| id.0), [3, 4, 5, 6, 7, 8]);
}

#[test]
fn umistr_constructors() {
    assert!(UmiStr::EMPTY.ptr.is_null());
    assert_eq!(UmiStr::EMPTY.len, 0);
    let s = UmiStr::from_static("héllo");
    assert_eq!(s.len, 6);
    assert!(!s.ptr.is_null());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn string_event_layouts() {
    // header (12) + padding (4) + UmiStr (16), then the position byte.
    assert_eq!(offset_of!(UmiChatReceived, json), 16);
    assert_eq!(offset_of!(UmiChatReceived, position), 32);
    assert_eq!(size_of::<UmiChatReceived>(), 40);
    for (size, offset) in [
        (size_of::<UmiChatSend>(), offset_of!(UmiChatSend, text)),
        (size_of::<UmiConnected>(), offset_of!(UmiConnected, address)),
        (
            size_of::<UmiDisconnected>(),
            offset_of!(UmiDisconnected, reason),
        ),
        (size_of::<UmiScreen>(), offset_of!(UmiScreen, name)),
    ] {
        assert_eq!((size, offset), (32, 16));
    }
    assert_eq!(offset_of!(UmiHost, event_set_text), 32);
}

#[cfg(target_pointer_width = "64")]
#[test]
fn ui_layouts() {
    assert_eq!(UmiEventId::KEY_PRESSED.0, 9);
    assert_eq!(size_of::<UmiKeyPressed>(), 32);
    assert_eq!(offset_of!(UmiSettingDef, key), 8);
    assert_eq!(offset_of!(UmiSettingDef, choices), 64);
    assert_eq!(size_of::<UmiSettingDef>(), 88);
    assert_eq!(size_of::<UmiSettingValue>(), 20);
    assert_eq!(size_of::<UmiHudPlacement>(), 16);
    assert_eq!(offset_of!(UmiHost, register_setting), 40);
    assert_eq!(offset_of!(UmiHost, chat_print), 120);
    assert_eq!(
        [
            UmiSettingKind::TOGGLE,
            UmiSettingKind::SLIDER,
            UmiSettingKind::CHOICE,
            UmiSettingKind::COLOR
        ]
        .map(|k| k.0),
        [1, 2, 3, 4]
    );
}

#[cfg(target_pointer_width = "64")]
#[test]
fn packet_payload_layout() {
    assert_eq!((size_of::<UmiBytes>(), align_of::<UmiBytes>()), (16, 8));
    assert_eq!(offset_of!(UmiBytes, ptr), 0);
    assert_eq!(offset_of!(UmiBytes, len), 8);
    assert_eq!(offset_of!(UmiPacket, header), 0);
    assert_eq!(offset_of!(UmiPacket, id), 12);
    assert_eq!(offset_of!(UmiPacket, data), 16);
    assert_eq!(size_of::<UmiPacket>(), 32);
    assert_eq!(offset_of!(UmiHost, packet_send), 128);
    assert_eq!(UmiEventId::PACKET_RECEIVED, UmiEventId(10));
    assert_eq!(UmiEventId::PACKET_SEND, UmiEventId(11));
    assert_eq!(UmiStatus::BUSY, UmiStatus(7));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn service_and_world_layouts() {
    assert_eq!(offset_of!(UmiPlayerPhysics, flags), 4);
    assert_eq!(offset_of!(UmiPlayerPhysics, position), 8);
    assert_eq!(offset_of!(UmiPlayerPhysics, velocity), 32);
    assert_eq!(offset_of!(UmiPlayerPhysics, yaw), 56);
    assert_eq!(offset_of!(UmiPlayerPhysics, fall_distance), 64);
    assert_eq!(size_of::<UmiPlayerPhysics>(), 72);
    assert_eq!(offset_of!(UmiPhysicsInput, forward), 8);
    assert_eq!(size_of::<UmiPhysicsInput>(), 24);
    assert_eq!(offset_of!(UmiEntity, kind), 8);
    assert_eq!(offset_of!(UmiEntity, position), 24);
    assert_eq!(offset_of!(UmiEntity, yaw), 48);
    assert_eq!(offset_of!(UmiEntity, name), 56);
    assert_eq!(size_of::<UmiEntity>(), 72);
    assert_eq!(offset_of!(UmiHost, register_physics), 136);
    assert_eq!(offset_of!(UmiHost, physics_next), 144);
    assert_eq!(offset_of!(UmiHost, world_block), 152);
    assert_eq!(offset_of!(UmiHost, world_entities), 160);
    assert_eq!(offset_of!(UmiHost, register_post_pass), 168);
    assert_eq!(offset_of!(UmiHost, post_set_param), 176);
    assert_eq!(UmiStatus::UNAVAILABLE, UmiStatus(8));
    assert_eq!(UmiServiceMode::REPLACE, UmiServiceMode(1));
}
