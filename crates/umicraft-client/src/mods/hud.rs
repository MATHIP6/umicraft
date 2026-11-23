// SPDX-License-Identifier: MIT OR Apache-2.0

//! Turns the widgets of the built-in mods into what the HUD renderer draws
//! (ADR-0065): measures each one, resolves its placement on the screen and hands
//! over a [`ModWidgetView`].

use umicraft_builtin_mods::{HudContent, HudPlacement, HudStyle, HudWidget, Shadow};
use umicraft_render::{
    gui_scale, measure_mod_widget_view, ModCell, ModLabel, ModLine, ModSegment, ModTick,
    ModWidgetView, ShadowStyle, WidgetStyle,
};
use umicraft_ui::FontAtlas;

use crate::mods::icons::ModIcons;

/// The look the renderer draws for the style a mod chose.
fn widget_style(style: &HudStyle) -> WidgetStyle {
    let channel = |value: u32, shift: u32| ((value >> shift) & 0xFF) as f32 / 255.0;
    WidgetStyle {
        background: style
            .background
            .map(|(rgb, opacity)| [channel(rgb, 16), channel(rgb, 8), channel(rgb, 0), opacity]),
        radius: style.radius,
        padding: style.padding,
        shadow: match style.shadow {
            Shadow::Vanilla => ShadowStyle::Vanilla,
            Shadow::Color(rgb) => ShadowStyle::Color(rgb),
            Shadow::Off => ShadowStyle::Off,
        },
    }
}

/// What [`mod_widget_view`] draws of a built-in widget: its placement and content.
#[must_use]
pub fn item_of(widget: HudWidget<'_>) -> (HudPlacement, &HudContent) {
    (widget.placement, widget.content)
}

/// The view of one widget on a window of `display` pixels, `None` while it has
/// nothing to draw yet (first frame).
///
/// The placement is resolved in GUI units (window size divided by the GUI scale),
/// so a widget keeps its place when the window or the GUI scale changes. A widget
/// whose centre is on the right half of the screen is mirrored, so its pictures
/// stay at the edge; the `invert` option of the mod swaps that. Built-in and
/// external widgets both come through here, so they look and behave the same.
#[must_use]
pub fn mod_widget_view(
    (placement, content): (HudPlacement, &HudContent),
    font: &FontAtlas,
    display: (u32, u32),
    icons: &ModIcons<'_>,
) -> Option<ModWidgetView> {
    if content.is_empty() {
        return None;
    }
    let scale = gui_scale(display.0, display.1) as f32;
    let screen = (display.0 as f32 / scale, display.1 as f32 / scale);
    let mut view = content_view(content, icons);
    // Measured at scale 1: `HudPlacement::rect` applies the widget scale.
    let rect = placement.rect(measure_mod_widget_view(font, &view), screen);
    view.mirrored = (rect.x + rect.w / 2.0 > screen.0 / 2.0) != content.invert;
    view.position = [rect.x, rect.y];
    view.scale = placement.scale;
    Some(view)
}

/// The view of an external widget anchored to the screen (API 1.1): its content
/// starts at the offsets of `placement`, in GUI pixels, at scale 1, with no panel,
/// no padding and no mirroring, so a mod's coordinates are screen coordinates.
/// `None` while it has nothing to draw.
#[must_use]
pub fn screen_widget_view(
    (placement, content): (HudPlacement, &HudContent),
    icons: &ModIcons<'_>,
) -> Option<ModWidgetView> {
    if content.is_empty() {
        return None;
    }
    let mut view = content_view(content, icons);
    view.style.background = None;
    view.style.padding = 0.0;
    view.position = [placement.offset_x, placement.offset_y];
    view.scale = 1.0;
    Some(view)
}

/// The content of a widget as the UI draws it, at the origin and scale 1.
fn content_view(content: &HudContent, icons: &ModIcons<'_>) -> ModWidgetView {
    let lines: Vec<ModLine> = content
        .lines()
        .iter()
        .map(|line| ModLine {
            icon: icons.resolve(line.icon),
            detail: line.detail.clone(),
            detail_color: line.detail_color,
            ..ModLine::new(line.text.as_str(), line.color)
        })
        .collect();
    let cells: Vec<ModCell> = content
        .cells()
        .iter()
        .map(|cell| ModCell {
            rect: cell.rect,
            pressed: cell.pressed,
            color: cell.color,
            text: cell.text.clone(),
        })
        .collect();
    let labels = content
        .labels()
        .iter()
        .map(|label| ModLabel {
            at: label.at,
            text: label.text.clone(),
            color: label.color,
            alpha: label.alpha,
            scale: label.scale,
        })
        .collect();
    let ticks = content
        .ticks()
        .iter()
        .map(|tick| ModTick {
            rect: tick.rect,
            color: tick.color,
            alpha: tick.alpha,
        })
        .collect();
    let segments = content
        .segments()
        .iter()
        .map(|segment| ModSegment {
            from: segment.from,
            to: segment.to,
            thickness: segment.thickness,
            color: segment.color,
            alpha: segment.alpha,
        })
        .collect();
    ModWidgetView {
        lines,
        cells,
        labels,
        ticks,
        segments,
        reserve: content.reserve,
        style: widget_style(&content.style),
        ..ModWidgetView::default()
    }
}

/// The views of `widgets`, given as `(placement, content)`, on a window of
/// `display` pixels; see [`mod_widget_view`].
#[must_use]
pub fn mod_widget_views<'a>(
    widgets: impl Iterator<Item = (HudPlacement, &'a HudContent)>,
    font: &FontAtlas,
    display: (u32, u32),
    icons: &ModIcons<'_>,
) -> Vec<ModWidgetView> {
    widgets
        .filter_map(|widget| mod_widget_view(widget, font, display, icons))
        .collect()
}

#[cfg(test)]
mod tests {
    use umicraft_builtin_mods::{mods, Anchor, HudPlacement, ModContext, ModRegistry};
    use umicraft_render::measure_mod_widget;

    use super::*;

    fn views(placement: HudPlacement) -> Vec<ModWidgetView> {
        let mut registry = ModRegistry::new(mods::all()).expect("distinct ids");
        registry
            .set_placement("fps", placement)
            .expect("fps exists");
        registry.update(&ModContext {
            fps: 60,
            ..ModContext::default()
        });
        mod_widget_views(
            registry.hud_widgets().map(item_of),
            &FontAtlas::fallback(),
            (800, 600),
            &ModIcons::default(),
        )
    }

    #[test]
    fn a_widget_on_the_screen_draws_at_its_offset_without_panel_or_padding() {
        let mut content = HudContent::default();
        content.tick([100.0, 50.0, 2.0, 2.0], 0xffffff, 1.0);
        content.segment(umicraft_builtin_mods::HudSegment {
            from: [0.0, 0.0],
            to: [300.0, 200.0],
            thickness: 1.0,
            color: 0xff0000,
            alpha: 1.0,
        });
        let placement = HudPlacement {
            scale: 2.5,
            ..HudPlacement::at(Anchor::TopLeft, 3.0, 4.0)
        };
        let view = screen_widget_view((placement, &content), &ModIcons::default())
            .expect("something to draw");
        assert_eq!((view.position, view.scale), ([3.0, 4.0], 1.0));
        assert_eq!((view.style.background, view.style.padding), (None, 0.0));
        assert!(!view.mirrored);
        assert_eq!(view.segments[0].to, [300.0, 200.0]);
        let empty = HudContent::default();
        assert!(screen_widget_view((placement, &empty), &ModIcons::default()).is_none());
    }

    #[test]
    fn the_fps_widget_is_drawn_with_its_text() {
        let views = views(HudPlacement::default());
        assert_eq!(views.len(), 1, "only the fps mod is on by default");
        assert_eq!(views[0].lines[0].text, "60 FPS");
    }

    #[test]
    fn the_widget_scale_is_not_counted_twice() {
        // Regression guard: measuring at the widget scale AND letting
        // `HudPlacement::rect` scale again would push a bottom-right widget off its corner.
        let placement = HudPlacement {
            scale: 2.0,
            ..HudPlacement::at(Anchor::BottomRight, 0.0, 0.0)
        };
        let view = &views(placement)[0];
        let gui = f32::from(u16::try_from(gui_scale(800, 600)).unwrap_or(1));
        let (w, h) =
            measure_mod_widget(&FontAtlas::fallback(), &view.lines, view.scale, &view.style);
        assert!((view.position[0] + w - 800.0 / gui).abs() < 0.01);
        assert!((view.position[1] + h - 600.0 / gui).abs() < 0.01);
    }

    #[test]
    fn the_options_of_the_mod_reach_the_view() {
        use umicraft_builtin_mods::SettingValue;

        let mut registry = ModRegistry::new(mods::all()).expect("distinct ids");
        for (key, value) in [
            ("background_color", SettingValue::Text("#ff0000".into())),
            ("background_opacity", SettingValue::Number(0.5)),
            ("padding", SettingValue::Number(4.0)),
            ("shadow", SettingValue::Text("off".into())),
        ] {
            registry
                .set_setting("fps", key, &value)
                .expect("fps option");
        }
        registry.update(&ModContext::default());
        let view = &mod_widget_views(
            registry.hud_widgets().map(item_of),
            &FontAtlas::fallback(),
            (800, 600),
            &ModIcons::default(),
        )[0];
        assert_eq!(view.style.background, Some([1.0, 0.0, 0.0, 0.5]));
        assert_eq!(view.style.padding, 4.0);
        assert_eq!(view.style.shadow, ShadowStyle::Off);
    }

    #[test]
    fn the_invert_option_swaps_the_side_of_the_pictures() {
        use umicraft_builtin_mods::SettingValue;

        let mut registry = ModRegistry::new(mods::all()).expect("distinct ids");
        registry
            .set_enabled("armor_status", true)
            .expect("the mod exists");
        let left_half = HudPlacement::at(Anchor::TopLeft, 0.0, 0.0);
        registry
            .set_placement("armor_status", left_half)
            .expect("the mod has a widget");
        let mirrored = |registry: &ModRegistry| {
            let views = mod_widget_views(
                registry
                    .hud_widgets()
                    .filter(|w| w.id.as_str() == "armor_status")
                    .map(item_of),
                &FontAtlas::fallback(),
                (800, 600),
                &ModIcons::default(),
            );
            views.first().map(|view| view.mirrored)
        };
        let context = ModContext {
            held: Some(umicraft_builtin_mods::ArmorPiece {
                name: "diamond_sword",
                damage: 0,
                max_damage: 1561,
            }),
            ..ModContext::default()
        };
        registry.update(&context);
        assert_eq!(mirrored(&registry), Some(false), "left half, icon first");
        registry
            .set_setting("armor_status", "invert", &SettingValue::Bool(true))
            .expect("the option exists");
        registry.update(&context);
        assert_eq!(mirrored(&registry), Some(true), "inverted: text then icon");
    }
}
