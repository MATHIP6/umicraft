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

    // 184 in 1.0; appended functions grow it (1.1: 224, 1.2: 272, 1.3: 280, 1.4: 360).
    assert_eq!((size_of::<UmiHost>(), align_of::<UmiHost>()), (360, 8));
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
    // 1.0 at the freeze; the current minor is pinned by `api_1_1_layouts`.
    assert_eq!(UMI_API_VERSION.major, 1);
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
    assert_eq!(size_of::<UmiPlayerPhysics>(), 80); // 72 in 1.0
    assert_eq!(offset_of!(UmiPhysicsInput, forward), 8);
    assert_eq!(size_of::<UmiPhysicsInput>(), 24);
    assert_eq!(offset_of!(UmiEntity, kind), 8);
    assert_eq!(offset_of!(UmiEntity, position), 24);
    assert_eq!(offset_of!(UmiEntity, yaw), 48);
    assert_eq!(offset_of!(UmiEntity, name), 56);
    assert_eq!(size_of::<UmiEntity>(), 280); // 72 in 1.0, 168 in 1.1
    assert_eq!(offset_of!(UmiHost, register_physics), 136);
    assert_eq!(offset_of!(UmiHost, physics_next), 144);
    assert_eq!(offset_of!(UmiHost, world_block), 152);
    assert_eq!(offset_of!(UmiHost, world_entities), 160);
    assert_eq!(offset_of!(UmiHost, register_post_pass), 168);
    assert_eq!(offset_of!(UmiHost, post_set_param), 176);
    assert_eq!(UmiStatus::UNAVAILABLE, UmiStatus(8));
    assert_eq!(UmiServiceMode::REPLACE, UmiServiceMode(1));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn frame_payload_layout() {
    assert_eq!(UmiEventId::FRAME, UmiEventId(12));
    assert_eq!(offset_of!(UmiFrame, now_ms), 16);
    assert_eq!(offset_of!(UmiFrame, position), 24);
    assert_eq!(offset_of!(UmiFrame, fps), 48);
    assert_eq!(offset_of!(UmiFrame, frame_ms), 52);
    assert_eq!(offset_of!(UmiFrame, yaw), 56);
    assert_eq!(offset_of!(UmiFrame, pitch), 60);
    assert_eq!(offset_of!(UmiFrame, left_clicks), 64);
    assert_eq!(offset_of!(UmiFrame, right_clicks), 68);
    assert_eq!(offset_of!(UmiFrame, ping_ms), 72);
    assert_eq!(offset_of!(UmiFrame, sprinting), 76);
    assert_eq!(size_of::<UmiFrame>(), 280); // 80 in 1.0
}

/// API 1.1 (ADR-0094): every addition sits after the last 1.0 field.
#[cfg(target_pointer_width = "64")]
#[test]
fn api_1_1_layouts() {
    assert_eq!(UmiEventId::PRE_RENDER, UmiEventId(13));

    assert_eq!(offset_of!(UmiFrame, eye), 80);
    assert_eq!(offset_of!(UmiFrame, render_origin), 104);
    assert_eq!(offset_of!(UmiFrame, view_proj), 128);
    assert_eq!(offset_of!(UmiFrame, viewport), 192);
    assert_eq!(offset_of!(UmiFrame, gui_scale), 200);
    assert_eq!(offset_of!(UmiFrame, fov_y), 204);
    assert_eq!(offset_of!(UmiFrame, aspect), 208);
    assert_eq!(offset_of!(UmiFrame, view_bob), 212);

    assert_eq!(offset_of!(UmiRenderCamera, flags), 4);
    assert_eq!(offset_of!(UmiRenderCamera, eye), 8);
    assert_eq!(offset_of!(UmiRenderCamera, yaw), 32);
    assert_eq!(offset_of!(UmiRenderCamera, pitch), 36);
    assert_eq!(offset_of!(UmiRenderCamera, fov_y), 40);
    assert_eq!(offset_of!(UmiRenderCamera, aspect), 44);
    assert_eq!(offset_of!(UmiRenderCamera, view_bob), 48);
    assert_eq!(size_of::<UmiRenderCamera>(), 112);
    assert_eq!(offset_of!(UmiPreRender, camera), 16);
    assert_eq!(size_of::<UmiPreRender>(), 128);
    assert_eq!(offset_of!(UmiCameraInput, look_dx), 8);
    assert_eq!(offset_of!(UmiCameraInput, up), 24);
    assert_eq!(offset_of!(UmiCameraInput, frame_seconds), 28);
    assert_eq!(size_of::<UmiCameraInput>(), 32);

    assert_eq!(offset_of!(UmiPlayerPhysics, fly_speed), 68);
    assert_eq!(offset_of!(UmiPlayerPhysics, game_mode), 72);
    assert_eq!(UMI_PHYSICS_ALLOW_FLYING, 32);
    assert_eq!(
        [
            UMI_GAME_MODE_SURVIVAL,
            UMI_GAME_MODE_CREATIVE,
            UMI_GAME_MODE_ADVENTURE,
            UMI_GAME_MODE_SPECTATOR
        ],
        [0, 1, 2, 3]
    );

    assert_eq!(offset_of!(UmiEntity, health), 72);
    assert_eq!(offset_of!(UmiEntity, max_health), 76);
    assert_eq!(offset_of!(UmiEntity, armor), 80);
    assert_eq!(offset_of!(UmiEntity, team), 88);
    assert_eq!(offset_of!(UmiEntity, width), 104);
    assert_eq!(offset_of!(UmiEntity, height), 108);
    assert_eq!(offset_of!(UmiEntity, velocity), 112);
    assert_eq!(offset_of!(UmiEntity, flags), 136);
    assert_eq!(offset_of!(UmiEntity, is_player), 140);
    assert_eq!(offset_of!(UmiEntity, render_position), 144);
    assert_eq!(
        [
            UMI_ENTITY_ON_FIRE,
            UMI_ENTITY_SNEAKING,
            UMI_ENTITY_SPRINTING,
            UMI_ENTITY_USING_ITEM,
            UMI_ENTITY_INVISIBLE,
            UMI_ENTITY_LIVING
        ],
        [1, 2, 4, 8, 16, 32]
    );

    assert_eq!(UMI_HUD_ANCHOR_SCREEN, 9);
    assert_eq!(offset_of!(UmiHost, register_camera), 184);
    assert_eq!(offset_of!(UmiHost, camera_next), 192);
    assert_eq!(offset_of!(UmiHost, world_entities_snapshot), 200);
    assert_eq!(offset_of!(UmiHost, hud_outline), 208);
    assert_eq!(offset_of!(UmiHost, hud_segment), 216);
}

#[test]
fn a_1_1_host_loads_1_0_mods_and_a_1_0_host_refuses_1_1_mods() {
    let v = |major, minor| UmiVersion { major, minor };
    assert!(UMI_API_VERSION.accepts(v(1, 0)));
    assert!(UMI_API_VERSION.accepts(v(1, 1)));
    assert!(!v(1, 0).accepts(v(1, 1)));
}

/// API 1.2 (ADR-0095): the menu additions sit after the last 1.1 field.
#[cfg(target_pointer_width = "64")]
#[test]
fn api_1_2_layouts() {
    assert_eq!(UmiEventId::MODULE_TOGGLED, UmiEventId(14));
    assert_eq!(UmiEventId::BUTTON_PRESSED, UmiEventId(15));

    assert_eq!(offset_of!(UmiModuleToggled, module), 16);
    assert_eq!(offset_of!(UmiModuleToggled, enabled), 32);
    assert_eq!(size_of::<UmiModuleToggled>(), 40);
    assert_eq!(offset_of!(UmiButtonPressed, module), 16);
    assert_eq!(offset_of!(UmiButtonPressed, key), 32);
    assert_eq!(size_of::<UmiButtonPressed>(), 48);

    assert_eq!(offset_of!(UmiModuleDef, flags), 4);
    assert_eq!(offset_of!(UmiModuleDef, key), 8);
    assert_eq!(offset_of!(UmiModuleDef, label), 24);
    assert_eq!(offset_of!(UmiModuleDef, description), 40);
    assert_eq!(offset_of!(UmiModuleDef, category), 56);
    assert_eq!(size_of::<UmiModuleDef>(), 72);
    assert_eq!(UMI_MODULE_DEFAULT_ON, 1);
    assert_eq!((UMI_ATTACH_HUD_WIDGET, UMI_ATTACH_POST_PASS), (1, 2));

    assert_eq!(offset_of!(UmiHost, register_category), 224);
    assert_eq!(offset_of!(UmiHost, register_module), 232);
    assert_eq!(offset_of!(UmiHost, register_setting_in), 240);
    assert_eq!(offset_of!(UmiHost, register_button), 248);
    assert_eq!(offset_of!(UmiHost, module_attach), 256);
    assert_eq!(offset_of!(UmiHost, module_enabled), 264);
}

#[test]
fn a_1_2_host_loads_older_mods_and_a_1_1_host_refuses_1_2_mods() {
    let v = |major, minor| UmiVersion { major, minor };
    assert!(UMI_API_VERSION.accepts(v(1, 1)));
    assert!(UMI_API_VERSION.accepts(v(1, 2)));
    assert!(!v(1, 1).accepts(v(1, 2)));
}

/// API 1.3 (ADR-0096): one host function appended.
#[cfg(target_pointer_width = "64")]
#[test]
fn api_1_3_layouts() {
    assert_eq!(offset_of!(UmiHost, camera_capture), 272);
}

#[test]
fn a_1_3_host_loads_older_mods_and_a_1_2_host_refuses_1_3_mods() {
    let v = |major, minor| UmiVersion { major, minor };
    assert!(UMI_API_VERSION.accepts(v(1, 2)));
    assert!(UMI_API_VERSION.accepts(v(1, 3)));
    assert!(!v(1, 2).accepts(v(1, 3)));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn api_1_4_layouts() {
    assert_eq!(UMI_API_VERSION, UmiVersion { major: 1, minor: 4 });
    for (offset, expected) in [
        (offset_of!(UmiHost, world_local_player), 280),
        (offset_of!(UmiHost, hud_circle), 288),
        (offset_of!(UmiHost, hud_polygon), 296),
        (offset_of!(UmiHost, hud_clip_push), 304),
        (offset_of!(UmiHost, hud_clip_pop), 312),
        (offset_of!(UmiHost, register_world_layer), 320),
        (offset_of!(UmiHost, world_line), 328),
        (offset_of!(UmiHost, world_box), 336),
        (offset_of!(UmiHost, world_polygon), 344),
        (offset_of!(UmiHost, world_highlight), 352),
        (offset_of!(UmiEntity, hurt_time), 168),
        (offset_of!(UmiEntity, death_time), 172),
        (offset_of!(UmiEntity, held_item), 176),
        (offset_of!(UmiEntity, held_damage), 180),
        (offset_of!(UmiEntity, display_name), 184),
        (offset_of!(UmiEntity, team_color), 200),
        (offset_of!(UmiEntity, ping), 204),
        (offset_of!(UmiEntity, uuid_version), 208),
        (offset_of!(UmiEntity, effect_count), 212),
        (offset_of!(UmiEntity, effects), 216),
    ] {
        assert_eq!(offset, expected);
    }
    assert_eq!((size_of::<UmiEffect>(), align_of::<UmiEffect>()), (8, 4));
    assert_eq!(offset_of!(UmiEffect, duration), 4);
    assert_eq!(UMI_MAX_ENTITY_EFFECTS, 8);
    assert_eq!(UMI_NO_COLOR, 0xFFFF_FFFF);
    assert_eq!(
        [
            UMI_ENTITY_SPECTATOR,
            UMI_ENTITY_IN_TAB_LIST,
            UMI_ENTITY_VISIBLE,
            UMI_ENTITY_TEAMMATE
        ],
        [64, 128, 256, 512]
    );
    assert_eq!((UMI_WORLD_THROUGH_WALLS, UMI_WORLD_FILL), (1, 2));
    assert_eq!(UMI_ATTACH_WORLD_LAYER, 3);
}

#[test]
fn a_1_4_host_loads_older_mods_and_a_1_3_host_refuses_1_4_mods() {
    let v = |major, minor| UmiVersion { major, minor };
    assert!(UMI_API_VERSION.accepts(v(1, 3)));
    assert!(UMI_API_VERSION.accepts(v(1, 4)));
    assert!(!v(1, 3).accepts(v(1, 4)));
    assert!(!UMI_API_VERSION.accepts(v(1, 5)));
}
