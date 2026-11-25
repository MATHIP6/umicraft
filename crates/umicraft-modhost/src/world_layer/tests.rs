// SPDX-License-Identifier: MIT OR Apache-2.0

use core::ffi::c_void;
use std::sync::Arc;

use umicraft_modapi_sys::{UmiWorldCx, UMI_WORLD_FILL, UMI_WORLD_THROUGH_WALLS};

use super::*;
use crate::ffi::host_table;
use crate::ui::menu::ModuleDef;
use crate::ui::testing::user;

fn with_layer(key: &str, draw: UmiWorldDrawFn) -> Arc<ModContext> {
    let cx = ModContext::new("esp".to_owned().try_into().unwrap());
    cx.register_world_layer(key, Some(draw), user()).unwrap();
    cx.finish_init(true);
    cx
}

fn drawn(cx: &ModContext) -> WorldDraw {
    let mut out = WorldDraw::default();
    cx.draw_world_layers(&mut out);
    out
}

/// A line, a filled box, a pentagon and a mark through walls.
extern "C" fn draw_all(world: *mut UmiWorldCx, _user: *mut c_void) -> UmiStatus {
    let host = host_table();
    let line = (host.world_line)(
        world,
        0.0,
        64.0,
        0.0,
        1.0,
        65.0,
        1.0,
        0xff_ff0000,
        0.5,
        100.0,
        0,
    );
    let cuboid = (host.world_box)(
        world,
        1.0,
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0x00ff00,
        1.0,
        UMI_WORLD_FILL,
    );
    let points = [
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.5, 0.0, 1.0, 0.5, 0.0, 2.0, -0.5, 0.0, 1.0,
    ];
    let polygon = (host.world_polygon)(world, points.as_ptr(), 5, 0x0000ff, 1.0, 0);
    let mark = (host.world_highlight)(world, 42, 0xffffff, 1.0, UMI_WORLD_THROUGH_WALLS);
    [line, cuboid, polygon, mark]
        .into_iter()
        .find(|status| !status.is_ok())
        .unwrap_or(UmiStatus::OK)
}

#[test]
fn every_primitive_lands_in_the_frame() {
    let out = drawn(&with_layer("all", draw_all));
    assert_eq!(out.lines.len(), 1);
    let line = out.lines[0];
    assert_eq!((line.from, line.to), ([0.0, 64.0, 0.0], [1.0, 65.0, 1.0]));
    assert_eq!(
        line.color,
        [1.0, 0.0, 0.0, 0.5],
        "the alpha byte is dropped"
    );
    assert_eq!(line.thickness, LINE_THICKNESS.1);
    assert!(!line.through_walls);
    assert_eq!(
        out.triangles.len(),
        12 + 3,
        "a box's six faces and a pentagon"
    );
    assert!(out.triangles[..12]
        .iter()
        .flat_map(|t| t.points)
        .all(|p| p.iter().all(|&v| v == 0.0 || v == 1.0)));
    assert_eq!(out.triangles[12].points[0], [0.0, 0.0, 0.0]);
    assert_eq!(
        out.highlights,
        [Highlight {
            entity: 42,
            color: [1.0; 4],
            through_walls: true,
        }]
    );
}

extern "C" fn draw_box_edges(world: *mut UmiWorldCx, _user: *mut c_void) -> UmiStatus {
    (host_table().world_box)(world, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0, 1.0, 0)
}

#[test]
fn a_box_without_fill_is_its_twelve_edges() {
    let out = drawn(&with_layer("edges", draw_box_edges));
    assert_eq!(out.lines.len(), 12);
    for line in &out.lines {
        let differing = (0..3).filter(|&a| line.from[a] != line.to[a]).count();
        assert_eq!(differing, 1, "an edge runs along one axis");
    }
}

/// Marks entities until the host refuses; returns `OK` so the mod keeps running.
extern "C" fn mark_until_refused(world: *mut UmiWorldCx, _user: *mut c_void) -> UmiStatus {
    let host = host_table();
    for id in 0.. {
        if (host.world_highlight)(world, id, 0, 1.0, 0) == UmiStatus::DENIED {
            break;
        }
    }
    UmiStatus::OK
}

#[test]
fn a_mod_is_capped_per_frame_and_starts_again_next_frame() {
    let cx = with_layer("marks", mark_until_refused);
    assert_eq!(drawn(&cx).highlights.len(), MAX_HIGHLIGHTS);
    assert_eq!(drawn(&cx).highlights.len(), MAX_HIGHLIGHTS);
}

extern "C" fn draw_bad(world: *mut UmiWorldCx, _user: *mut c_void) -> UmiStatus {
    let host = host_table();
    let nan = (host.world_line)(world, f64::NAN, 0.0, 0.0, 1.0, 1.0, 1.0, 0, 1.0, 1.0, 0);
    let few = (host.world_polygon)(world, [0.0; 6].as_ptr(), 2, 0, 1.0, 0);
    let null = (host.world_polygon)(world, core::ptr::null(), 3, 0, 1.0, 0);
    let all_invalid = [nan, few, null]
        .iter()
        .all(|&status| status == UmiStatus::INVALID_ARGUMENT);
    if all_invalid {
        UmiStatus::OK
    } else {
        UmiStatus::INVALID_ARGUMENT
    }
}

#[test]
fn bad_numbers_and_counts_are_refused() {
    let cx = with_layer("bad", draw_bad);
    assert!(drawn(&cx).is_empty());
    assert!(cx.is_enabled(), "the layer saw every refusal");
}

/// Adds a line, then fails.
extern "C" fn line_then_fail(world: *mut UmiWorldCx, _user: *mut c_void) -> UmiStatus {
    (host_table().world_line)(world, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0, 1.0, 1.0, 0);
    UmiStatus::INVALID_ARGUMENT
}

#[test]
fn a_failing_mod_leaves_nothing_of_the_frame() {
    let cx = ModContext::new("esp".to_owned().try_into().unwrap());
    cx.register_world_layer("ok", Some(draw_box_edges), user())
        .unwrap();
    cx.register_world_layer("bad", Some(line_then_fail), user())
        .unwrap();
    cx.finish_init(true);
    assert!(drawn(&cx).is_empty());
    assert!(!cx.is_enabled(), "the failure failed the mod");
}

#[test]
fn layers_are_declared_with_the_usual_checks() {
    let cx = ModContext::new("esp".to_owned().try_into().unwrap());
    for index in 0..MAX_WORLD_LAYERS {
        cx.register_world_layer(&format!("l{index}"), Some(draw_box_edges), user())
            .unwrap();
    }
    assert!(matches!(
        cx.register_world_layer("more", Some(draw_box_edges), user()),
        Err(DeclareError::TooMany { .. })
    ));
    let cx = ModContext::new("esp2".to_owned().try_into().unwrap());
    cx.register_world_layer("a", Some(draw_box_edges), user())
        .unwrap();
    assert!(matches!(
        cx.register_world_layer("a", Some(draw_box_edges), user()),
        Err(DeclareError::Duplicate { .. })
    ));
    assert!(matches!(
        cx.register_world_layer("b", None, user()),
        Err(DeclareError::Invalid { .. })
    ));
    cx.finish_init(true);
    assert!(matches!(
        cx.register_world_layer("late", Some(draw_box_edges), user()),
        Err(DeclareError::Late)
    ));
}

#[test]
fn an_off_mod_or_module_draws_nothing() {
    let cx = ModContext::new("esp".to_owned().try_into().unwrap());
    cx.register_module(&ModuleDef {
        key: "esp",
        label: "ESP",
        description: "",
        category: "",
        default_on: false,
    })
    .unwrap();
    cx.register_world_layer("boxes", Some(draw_box_edges), user())
        .unwrap();
    cx.module_attach("esp", crate::ui::menu::Attach::WorldLayer, "boxes")
        .unwrap();
    cx.finish_init(true);
    assert!(drawn(&cx).is_empty(), "the module is off");
    cx.set_module("esp", true).unwrap();
    assert_eq!(drawn(&cx).lines.len(), 12);
    cx.set_wanted(false);
    assert!(drawn(&cx).is_empty(), "the mod is off");
}

#[test]
fn outside_a_draw_the_functions_are_denied() {
    let mut fake = 0_u8;
    let stray = core::ptr::from_mut(&mut fake).cast::<UmiWorldCx>();
    let status = (host_table().world_highlight)(stray, 1, 0, 1.0, 0);
    assert_eq!(status, UmiStatus::DENIED);
    let status = (host_table().world_highlight)(core::ptr::null_mut(), 1, 0, 1.0, 0);
    assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
}
