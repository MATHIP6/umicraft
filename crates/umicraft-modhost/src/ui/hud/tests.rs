// SPDX-License-Identifier: MIT OR Apache-2.0

use umicraft_builtin_mods::Anchor;
use umicraft_modapi_sys::{UmiHudCx, UmiStatus, UmiStr};

use super::*;
use crate::budget::MAX_CONSECUTIVE_OVERRUNS;
use crate::ffi::host_table;
use crate::ui::testing::{draw_error, draw_lines, draw_panicked, draw_slow, user, SAVED_CX};

fn placement() -> HudPlacement {
    HudPlacement::at(Anchor::TopRight, -2.0, 3.0)
}

fn with_widget(draw: UmiHudDrawFn) -> std::sync::Arc<ModContext> {
    let cx = crate::ModContext::new("hud".to_owned().try_into().unwrap());
    cx.register_widget("main", "Main", placement(), Some(draw), user())
        .unwrap();
    cx.finish_init(true);
    cx
}

/// The lines each visited widget holds, as `(key, texts)`.
fn visited(cx: &ModContext) -> Vec<(String, Vec<String>)> {
    let mut seen = Vec::new();
    cx.visit_widgets(|w| {
        let texts = w.content.lines().iter().map(|l| l.text.clone()).collect();
        seen.push((w.key.to_owned(), texts));
    });
    seen
}

#[test]
fn a_drawn_widget_exposes_its_content_and_placement() {
    let cx = with_widget(draw_lines);

    cx.draw_widgets();

    let mut seen = Vec::new();
    cx.visit_widgets(|w| {
        seen.push((
            w.mod_id.to_string(),
            w.key.to_owned(),
            w.label.to_owned(),
            w.placement,
            w.content.lines().len(),
            w.content.lines()[1].color,
        ));
    });
    assert_eq!(
        seen,
        [(
            "hud".into(),
            "main".into(),
            "Main".into(),
            placement(),
            2,
            0x00_ff_00
        )]
    );
}

#[test]
fn each_frame_starts_from_an_empty_widget() {
    let cx = with_widget(draw_lines);
    cx.draw_widgets();
    cx.draw_widgets();
    assert_eq!(visited(&cx)[0].1, ["fps 60", "ping 12"]);
}

#[test]
fn a_mod_that_is_off_or_failed_is_neither_drawn_nor_visited() {
    let cx = with_widget(draw_lines);
    cx.draw_widgets();
    cx.set_wanted(false);
    assert!(visited(&cx).is_empty());

    cx.set_wanted(true);
    cx.fail("test");
    cx.draw_widgets();
    assert!(visited(&cx).is_empty());
    assert_eq!(cx.hud_stats().calls, 1, "only the first draw ran");
}

#[test]
fn an_error_or_a_panic_status_fails_the_mod() {
    for draw in [draw_error as UmiHudDrawFn, draw_panicked] {
        let cx = with_widget(draw);
        cx.draw_widgets();
        assert!(cx.is_failed() && !cx.is_enabled());
    }
}

#[test]
fn fifty_overruns_of_the_half_millisecond_budget_fail_the_mod() {
    let cx = with_widget(draw_slow);
    for _ in 0..MAX_CONSECUTIVE_OVERRUNS - 1 {
        cx.draw_widgets();
    }
    assert!(cx.is_enabled(), "49 overruns are tolerated");
    cx.draw_widgets();
    assert!(cx.is_failed());
    let stats = cx.hud_stats();
    assert_eq!(stats.overruns, MAX_CONSECUTIVE_OVERRUNS);
    assert_eq!(cx.stats().calls, 0, "event counters are separate");
}

#[test]
fn the_hud_pointer_is_only_good_during_the_draw_call() {
    let cx = with_widget(draw_lines);
    cx.draw_widgets();
    let stale = SAVED_CX.with(std::cell::Cell::get) as *mut UmiHudCx;
    let host = host_table();

    let late = (host.hud_line)(stale, UmiStr::from_static("late"), 0);
    let null = (host.hud_line)(std::ptr::null_mut(), UmiStr::from_static("x"), 0);

    assert_eq!(late, UmiStatus::DENIED);
    assert_eq!(null, UmiStatus::INVALID_ARGUMENT);
    assert_eq!(visited(&cx)[0].1.len(), 2, "nothing was added");
}

#[test]
fn hud_functions_refuse_bad_text_and_numbers_during_a_draw() {
    extern "C" fn draw_bad(cx: *mut UmiHudCx, _: *mut core::ffi::c_void) -> UmiStatus {
        let host = host_table();
        let bad_utf8 = [0xffu8, 0xfe];
        let text = UmiStr {
            ptr: bad_utf8.as_ptr(),
            len: 2,
        };
        let statuses = [
            (host.hud_line)(cx, text, 0),
            (host.hud_tick)(cx, 0.0, 0.0, f32::NAN, 1.0, 0, 1.0),
            (host.hud_reserve)(cx, -1.0, 1.0),
            (host.hud_cell)(cx, 0.0, 0.0, 5.0, 5.0, 1, UmiStr::from_static("W"), 0),
        ];
        if statuses[..3]
            .iter()
            .all(|s| *s == UmiStatus::INVALID_ARGUMENT)
            && statuses[3].is_ok()
        {
            UmiStatus::OK
        } else {
            UmiStatus::ERROR
        }
    }
    let cx = with_widget(draw_bad);
    cx.draw_widgets();
    assert!(cx.is_enabled(), "every refusal was the expected one");
    let mut cells = 0;
    cx.visit_widgets(|w| cells = w.content.cells().len());
    assert_eq!(cells, 1);
}

#[test]
fn a_placement_change_is_clamped_and_visible() {
    let cx = with_widget(draw_lines);
    let moved = HudPlacement {
        scale: 99.0,
        ..HudPlacement::at(Anchor::Center, 1.0, 2.0)
    };
    cx.set_placement("main", moved).unwrap();
    let mut seen = None;
    cx.visit_widgets(|w| seen = Some(w.placement));
    assert_eq!(seen.unwrap().scale, umicraft_builtin_mods::MAX_SCALE);
    assert_eq!(seen.unwrap().anchor, Anchor::Center);
}

#[test]
fn placements_are_validated_like_the_abi_says() {
    let raw = |anchor, offset_x, scale| UmiHudPlacement {
        anchor,
        offset_x,
        offset_y: 0.0,
        scale,
    };
    assert_eq!(
        placement_of(raw(8, 1.0, 10.0)).unwrap().scale,
        umicraft_builtin_mods::MAX_SCALE
    );
    assert_eq!(
        placement_of(raw(8, 1.0, 1.0)).unwrap().anchor,
        Anchor::BottomRight
    );
    assert!(placement_of(raw(9, 1.0, 1.0)).is_err());
    assert!(placement_of(raw(0, f32::NAN, 1.0)).is_err());
    assert!(placement_of(raw(0, 0.0, f32::INFINITY)).is_err());
}

#[test]
fn widget_declarations_are_validated() {
    let cx = crate::ModContext::new("hud".to_owned().try_into().unwrap());
    let register = |key: &str, label: &str, draw: Option<UmiHudDrawFn>| {
        cx.register_widget(key, label, placement(), draw, user())
    };
    assert!(register("main", "Main", Some(draw_lines)).is_ok());
    assert!(matches!(
        register("main", "Again", Some(draw_lines)),
        Err(DeclareError::Duplicate { .. })
    ));
    assert!(register("Bad Key", "x", Some(draw_lines)).is_err());
    assert!(register("empty_label", "", Some(draw_lines)).is_err());
    assert!(register("no_draw", "x", None).is_err());
}
