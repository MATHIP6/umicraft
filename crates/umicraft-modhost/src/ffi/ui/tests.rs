// SPDX-License-Identifier: MIT OR Apache-2.0

use umicraft_modapi_sys::{UmiHost, UmiSettingKind, UMI_API_VERSION};

use super::*;
use crate::context::ModContext;
use crate::ffi::{handle_of, host_table};
use crate::ui::testing::{draw_lines, run_ok};

fn str_of(text: &'static str) -> UmiStr {
    UmiStr::from_static(text)
}

fn def(kind: UmiSettingKind) -> UmiSettingDef {
    UmiSettingDef {
        size: size_of::<UmiSettingDef>() as u32,
        kind,
        key: str_of("size"),
        label: str_of("Size"),
        toggle_default: 0,
        slider_min: 0.0,
        slider_max: 10.0,
        slider_step: 1.0,
        slider_default: 5.0,
        choices: core::ptr::null(),
        choices_len: 0,
        choice_default: 0,
        color_default: 0x00_ff_00,
    }
}

fn setup() -> (std::sync::Arc<ModContext>, UmiHost) {
    let cx = ModContext::new("m".to_owned().try_into().unwrap());
    (cx, host_table())
}

#[test]
fn the_table_is_complete_for_api_0_3() {
    assert_eq!(host_table().version, UMI_API_VERSION);
}

#[test]
fn a_setting_registers_and_reads_back_through_the_table() {
    let (cx, host) = setup();
    let me = handle_of(&cx);
    let slider = def(UmiSettingKind::SLIDER);
    assert_eq!((host.register_setting)(me, &slider), UmiStatus::OK);
    assert_eq!((host.register_setting)(me, &slider), UmiStatus::CONFLICT);

    let mut out = core::mem::MaybeUninit::<UmiSettingValue>::uninit();
    assert_eq!(
        (host.setting_get)(me, str_of("size"), out.as_mut_ptr()),
        UmiStatus::OK
    );
    // SAFETY: `setting_get` returned OK, so it wrote the whole value.
    let out = unsafe { out.assume_init() };
    assert_eq!((out.kind, out.slider), (UmiSettingKind::SLIDER, 5.0));
    assert_eq!(
        (host.setting_get)(me, str_of("size"), core::ptr::null_mut()),
        UmiStatus::INVALID_ARGUMENT
    );
    let mut ignored = out;
    assert_eq!(
        (host.setting_get)(me, str_of("nope"), &mut ignored),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn a_choice_reads_its_labels_and_caps_the_count() {
    let (cx, host) = setup();
    let me = handle_of(&cx);
    let options = [str_of("a"), str_of("b")];
    let choice = UmiSettingDef {
        choices: options.as_ptr(),
        choices_len: 2,
        choice_default: 1,
        ..def(UmiSettingKind::CHOICE)
    };
    assert_eq!((host.register_setting)(me, &choice), UmiStatus::OK);

    // A huge `choices_len` is refused without reading that many entries.
    let huge = UmiSettingDef {
        key: str_of("huge"),
        choices_len: usize::MAX,
        ..choice
    };
    assert_eq!(
        (host.register_setting)(me, &huge),
        UmiStatus::INVALID_ARGUMENT
    );
    let null_list = UmiSettingDef {
        key: str_of("nul"),
        choices: core::ptr::null(),
        ..choice
    };
    assert_eq!(
        (host.register_setting)(me, &null_list),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn malformed_setting_definitions_are_refused() {
    let (cx, host) = setup();
    let me = handle_of(&cx);
    let bad_utf8 = [0xffu8];
    let bad_key = UmiSettingDef {
        key: UmiStr {
            ptr: bad_utf8.as_ptr(),
            len: 1,
        },
        ..def(UmiSettingKind::TOGGLE)
    };
    let short = UmiSettingDef {
        size: 4,
        ..def(UmiSettingKind::TOGGLE)
    };
    let toggle_two = UmiSettingDef {
        toggle_default: 2,
        ..def(UmiSettingKind::TOGGLE)
    };
    for bad in [bad_key, short, toggle_two, def(UmiSettingKind(99))] {
        assert_eq!(
            (host.register_setting)(me, &bad),
            UmiStatus::INVALID_ARGUMENT
        );
    }
    assert_eq!(
        (host.register_setting)(me, core::ptr::null()),
        UmiStatus::INVALID_ARGUMENT
    );
    let null_me = UmiModHandle(core::ptr::null_mut());
    assert_eq!(
        (host.register_setting)(null_me, &def(UmiSettingKind::TOGGLE)),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn widgets_keys_and_commands_register_and_report_their_errors() {
    let (cx, host) = setup();
    let me = handle_of(&cx);
    let placement = UmiHudPlacement {
        anchor: 4,
        offset_x: 0.0,
        offset_y: 0.0,
        scale: 1.0,
    };
    let widget = |key, anchor| {
        (host.register_hud_widget)(
            me,
            key,
            str_of("W"),
            UmiHudPlacement {
                anchor,
                ..placement
            },
            Some(draw_lines),
            core::ptr::null_mut(),
        )
    };
    assert_eq!(widget(str_of("main"), 4), UmiStatus::OK);
    assert_eq!(widget(str_of("main"), 4), UmiStatus::CONFLICT);
    assert_eq!(widget(str_of("other"), 9), UmiStatus::INVALID_ARGUMENT);

    let key = |id| (host.register_key)(me, id, str_of("Label"), str_of("KeyG"));
    assert_eq!(key(str_of("toggle")), UmiStatus::OK);
    assert_eq!(key(str_of("toggle")), UmiStatus::CONFLICT);
    assert_eq!(key(str_of("Bad")), UmiStatus::INVALID_ARGUMENT);

    let command =
        |name| (host.register_command)(me, name, str_of("/x"), Some(run_ok), core::ptr::null_mut());
    assert_eq!(command(str_of("hello")), UmiStatus::OK);
    assert_eq!(command(str_of("hello")), UmiStatus::CONFLICT);
    assert_eq!(command(str_of("Bad")), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(
        (host.register_command)(me, str_of("x"), UmiStr::EMPTY, None, core::ptr::null_mut()),
        UmiStatus::INVALID_ARGUMENT
    );

    cx.finish_init(true);
    assert_eq!(key(str_of("late")), UmiStatus::DENIED);
}

#[test]
fn chat_print_reaches_the_outbox_and_refuses_bad_text() {
    let (cx, host) = setup();
    let me = handle_of(&cx);
    cx.finish_init(true);

    assert_eq!((host.chat_print)(me, str_of("hello")), UmiStatus::OK);
    let bad = [0xffu8];
    let bad = UmiStr {
        ptr: bad.as_ptr(),
        len: 1,
    };
    assert_eq!((host.chat_print)(me, bad), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(cx.shared.chat_outbox().try_recv().as_deref(), Ok("hello"));
}
