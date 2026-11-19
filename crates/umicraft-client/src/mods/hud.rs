// SPDX-License-Identifier: MIT OR Apache-2.0

//! Turns the widgets of the built-in mods into what the HUD renderer draws
//! (ADR-0065): measures each one, resolves its placement on the screen and hands
//! over a [`ModWidgetView`].

use umicraft_builtin_mods::{HudStyle, HudWidget, Shadow};
use umicraft_render::{
    gui_scale, measure_mod_widget_view, ModCell, ModLabel, ModLine, ModTick, ModWidgetView,
    ShadowStyle, WidgetStyle,
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

/// The views of `widgets` on a window of `display` pixels.
///
/// The placement is resolved in GUI units (window size divided by the GUI scale),
/// so a widget keeps its place when the window or the GUI scale changes. A widget
/// with nothing to draw yet (first frame) is skipped. A widget whose centre is on
/// the right half of the screen is mirrored, so its pictures stay at the edge; the
/// `invert` option of the mod swaps that.
#[must_use]
pub fn mod_widget_views<'a>(
    widgets: impl Iterator<Item = HudWidget<'a>>,
    font: &FontAtlas,
    display: (u32, u32),
    icons: &ModIcons<'_>,
) -> Vec<ModWidgetView> {
    let scale = gui_scale(display.0, display.1) as f32;
    let screen = (display.0 as f32 / scale, display.1 as f32 / scale);
    widgets
        .filter(|widget| !widget.content.is_empty())
        .map(|widget| {
            let lines: Vec<ModLine> = widget
                .content
                .lines()
                .iter()
                .map(|line| ModLine {
                    icon: icons.resolve(line.icon),
                    detail: line.detail.clone(),
                    detail_color: line.detail_color,
                    ..ModLine::new(line.text.as_str(), line.color)
                })
                .collect();
            let cells: Vec<ModCell> = widget
                .content
                .cells()
                .iter()
                .map(|cell| ModCell {
                    rect: cell.rect,
                    pressed: cell.pressed,
                    color: cell.color,
                    text: cell.text.clone(),
                })
                .collect();
            let labels = widget
                .content
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
            let ticks = widget
                .content
                .ticks()
                .iter()
                .map(|tick| ModTick {
                    rect: tick.rect,
                    color: tick.color,
                    alpha: tick.alpha,
                })
                .collect();
            let mut view = ModWidgetView {
                lines,
                cells,
                labels,
                ticks,
                reserve: widget.content.reserve,
                style: widget_style(&widget.content.style),
                ..ModWidgetView::default()
            };
            // Measured at scale 1: `HudPlacement::rect` applies the widget scale.
            let rect = widget
                .placement
                .rect(measure_mod_widget_view(font, &view), screen);
            view.mirrored = (rect.x + rect.w / 2.0 > screen.0 / 2.0) != widget.content.invert;
            view.position = [rect.x, rect.y];
            view.scale = widget.placement.scale;
            view
        })
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
            registry.hud_widgets(),
            &FontAtlas::fallback(),
            (800, 600),
            &ModIcons::default(),
        )
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
            registry.hud_widgets(),
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
                    .filter(|w| w.id.as_str() == "armor_status"),
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
