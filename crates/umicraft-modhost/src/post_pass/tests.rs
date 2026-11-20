// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use umicraft_modapi_sys::{UmiHost, UmiStatus, UmiStr};

use super::*;
use crate::ffi::{handle_of, host_table};

const GREY: &str = "@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    return textureSample(umi_scene, umi_sampler, in.uv);
}";

fn text(s: &str) -> UmiStr {
    UmiStr {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}

fn setup(id: &str) -> (Arc<ModContext>, UmiHost) {
    (
        ModContext::new(id.to_owned().try_into().unwrap()),
        host_table(),
    )
}

fn register(host: &UmiHost, cx: &ModContext, key: &str, wgsl: &str) -> UmiStatus {
    (host.register_post_pass)(handle_of(cx), text(key), text("Grey"), text(wgsl))
}

#[test]
fn a_pass_registers_and_a_duplicate_conflicts() {
    let (cx, host) = setup("m");
    assert_eq!(register(&host, &cx, "grey", GREY), UmiStatus::OK);
    assert_eq!(register(&host, &cx, "grey", GREY), UmiStatus::CONFLICT);
    assert_eq!(register(&host, &cx, "other", GREY), UmiStatus::OK);
    cx.finish_init(true);
    let keys: Vec<_> = cx.post_passes().map(|p| p.key.to_owned()).collect();
    assert_eq!(keys, ["grey", "other"]);
    let first = cx.post_passes().next().unwrap();
    assert_eq!(first.label, "Grey");
    assert_eq!(first.wgsl, crate::post::compose(GREY));
    assert_eq!(first.mod_id.as_str(), "m");
}

#[test]
fn bad_input_is_invalid_argument_and_late_is_denied() {
    let (cx, host) = setup("m");
    assert_eq!(
        register(&host, &cx, "Bad Key", GREY),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        register(&host, &cx, "ok", "@fragment fn fs_main( {"),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        register(&host, &cx, "ok", "fn nothing() {}"),
        UmiStatus::INVALID_ARGUMENT
    );
    let bad_utf8 = [0xffu8, 0xfe];
    let status = (host.register_post_pass)(
        handle_of(&cx),
        text("ok"),
        text("L"),
        UmiStr {
            ptr: bad_utf8.as_ptr(),
            len: 2,
        },
    );
    assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
    assert_eq!(cx.post_passes().count(), 0);
    cx.finish_init(true);
    assert_eq!(register(&host, &cx, "late", GREY), UmiStatus::DENIED);
}

#[test]
fn a_failed_init_drops_its_passes() {
    let (cx, host) = setup("m");
    assert_eq!(register(&host, &cx, "grey", GREY), UmiStatus::OK);
    cx.finish_init(false);
    assert_eq!(cx.post_passes().count(), 0);
}

#[test]
fn params_round_trip_and_reject_bad_arguments() {
    let (cx, host) = setup("m");
    register(&host, &cx, "grey", GREY);
    cx.finish_init(true);
    let me = handle_of(&cx);
    let set =
        |key: &str, index, x: f32| (host.post_set_param)(me, text(key), index, x, 2.0, 3.0, 4.0);
    assert_eq!(set("grey", 0, 1.0), UmiStatus::OK);
    assert_eq!(set("grey", 3, 5.0), UmiStatus::OK);
    let params = cx.post_passes().next().unwrap().params();
    assert_eq!(params[0], [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(params[3], [5.0, 2.0, 3.0, 4.0]);
    assert_eq!(params[1], [0.0; 4]);
    assert_eq!(set("nope", 0, 1.0), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(set("grey", 4, 1.0), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(set("grey", 0, f32::NAN), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(set("grey", 0, f32::INFINITY), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(cx.post_passes().next().unwrap().params()[0][0], 1.0);
}

#[test]
fn enabled_follows_the_mod_and_refusal_is_permanent() {
    let (cx, host) = setup("m");
    register(&host, &cx, "grey", GREY);
    register(&host, &cx, "other", GREY);
    cx.finish_init(true);
    let enabled = |cx: &ModContext| cx.post_passes().map(|p| p.enabled).collect::<Vec<_>>();
    assert_eq!(enabled(&cx), [true, true]);

    assert!(cx.set_wanted(false));
    assert_eq!(enabled(&cx), [false, false]);
    // Params may be set while the mod is off.
    let status = (host.post_set_param)(handle_of(&cx), text("grey"), 0, 1.0, 0.0, 0.0, 0.0);
    assert_eq!(status, UmiStatus::OK);
    assert!(cx.set_wanted(true));

    assert!(cx.refuse_post_pass("grey", "no good"));
    assert!(cx.refuse_post_pass("grey", "again"));
    assert!(!cx.refuse_post_pass("nope", "x"));
    assert_eq!(enabled(&cx), [false, true]);
    assert!(cx.is_enabled());
    cx.set_wanted(false);
    cx.set_wanted(true);
    assert_eq!(enabled(&cx), [false, true]);
}
