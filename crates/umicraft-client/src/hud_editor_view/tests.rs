// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the registry to movable HUD widgets bridge.

use umicraft_builtin_mods::{mods, Anchor, ModContext};

use super::*;

const DISPLAY: (u32, u32) = (800, 600);

fn registry() -> ModRegistry {
    ModRegistry::new(mods::all()).expect("the built-in mods have distinct ids")
}

fn lang() -> Lang {
    crate::language::load(Lang::DEFAULT_CODE)
}

fn view_of(registry: &ModRegistry) -> HudEditorView {
    build(registry, &lang(), &FontAtlas::fallback(), DISPLAY)
}

fn placement(registry: &ModRegistry, id: &str) -> HudPlacement {
    registry
        .mods()
        .find(|view| view.meta.id.as_str() == id)
        .and_then(|view| view.placement)
        .expect("a HUD mod")
}

fn placed(id: &str, rect: Rect, scale: f32) -> HudEditorOutcome {
    HudEditorOutcome::Placed {
        id: id.into(),
        rect,
        scale,
        screen: [400.0, 300.0],
    }
}

#[test]
fn only_the_enabled_hud_mods_are_previewed() {
    let mut registry = registry();
    registry.update(&ModContext {
        fps: 60,
        ..ModContext::default()
    });
    let view = view_of(&registry);
    let ids: Vec<_> = view.widgets.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["fps"], "toggle sprint has no widget, the rest is off");
    assert_eq!(view.widgets[0].name, "FPS");
    assert_eq!(view.widgets[0].widget.lines[0].text, "60 FPS");
    assert_eq!(view.scale_range, (0.5, 3.0));
    let gui = gui_scale(DISPLAY.0, DISPLAY.1) as f32;
    assert_eq!(
        view.screen,
        [DISPLAY.0 as f32 / gui, DISPLAY.1 as f32 / gui]
    );
}

#[test]
fn a_mod_with_nothing_to_show_gets_its_name_so_it_can_be_grabbed() {
    let mut registry = registry();
    registry.set_enabled("coordinates", true).expect("exists");
    // No update yet: the mod has no line.
    let view = view_of(&registry);
    let coordinates = view
        .widgets
        .iter()
        .find(|w| w.id == "coordinates")
        .expect("previewed");
    assert_eq!(coordinates.widget.lines.len(), 1);
    assert_eq!(coordinates.widget.lines[0].text, coordinates.name);
}

#[test]
fn a_dropped_widget_keeps_the_place_it_was_dropped_at() {
    let mut registry = registry();
    let rect = Rect::new(290.0, 10.0, 100.0, 20.0);
    assert_eq!(apply(&mut registry, &placed("fps", rect, 1.5)), Ok(true));
    let placement = placement(&registry, "fps");
    assert_eq!(placement.anchor, Anchor::TopRight);
    assert!((placement.scale - 1.5).abs() < 1e-6);
    let back = placement.rect((100.0 / 1.5, 20.0 / 1.5), (400.0, 300.0));
    assert!((back.x - 290.0).abs() < 1e-3 && (back.y - 10.0).abs() < 1e-3);
}

#[test]
fn a_bad_scale_from_the_editor_is_clamped_by_the_registry() {
    let mut registry = registry();
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    apply(&mut registry, &placed("fps", rect, 99.0)).expect("applies");
    assert_eq!(placement(&registry, "fps").scale, 3.0);
}

#[test]
fn reset_restores_the_default_placement_of_one_widget() {
    let defaults = placement(&registry(), "fps");
    let mut registry = registry();
    let rect = Rect::new(200.0, 200.0, 50.0, 10.0);
    apply(&mut registry, &placed("fps", rect, 2.0)).expect("applies");
    assert_ne!(placement(&registry, "fps"), defaults);
    let reset = HudEditorOutcome::Reset { id: "fps".into() };
    assert_eq!(apply(&mut registry, &reset), Ok(true));
    assert_eq!(placement(&registry, "fps"), defaults);
}

#[test]
fn reset_all_restores_every_hud_widget() {
    let fresh = registry();
    let mut registry = registry();
    for id in ["fps", "coordinates", "cps"] {
        let rect = Rect::new(100.0, 100.0, 50.0, 10.0);
        apply(&mut registry, &placed(id, rect, 2.0)).expect("applies");
    }
    assert_eq!(apply(&mut registry, &HudEditorOutcome::ResetAll), Ok(true));
    for id in ["fps", "coordinates", "cps"] {
        assert_eq!(placement(&registry, id), placement(&fresh, id), "{id}");
    }
}

#[test]
fn an_unknown_mod_is_an_error() {
    let mut registry = registry();
    let ghost = HudEditorOutcome::Reset { id: "ghost".into() };
    assert_eq!(
        apply(&mut registry, &ghost),
        Err(RegistryError::UnknownMod("ghost".into()))
    );
}

#[test]
fn the_snap_pulls_a_widget_near_the_edge_onto_it() {
    let near = Rect::new(2.0, 3.0, 50.0, 10.0);
    let snapped = snap(near, [400.0, 300.0]);
    assert_eq!((snapped.x, snapped.y), (0.0, 0.0));
    assert_eq!((snapped.width, snapped.height), (50.0, 10.0));
    let far = Rect::new(100.0, 100.0, 50.0, 10.0);
    assert_eq!(snap(far, [400.0, 300.0]), far);
}
