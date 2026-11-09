// SPDX-License-Identifier: MIT OR Apache-2.0

//! Turns the widgets of the built-in mods into what the HUD renderer draws
//! (ADR-0065): measures each one, resolves its placement on the screen and hands
//! over a [`ModWidgetView`].

use umicraft_builtin_mods::{HudStyle, HudWidget, Shadow};
use umicraft_render::{
    gui_scale, measure_mod_widget, ModLine, ModWidgetView, ShadowStyle, WidgetStyle,
};
use umicraft_ui::FontAtlas;

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
/// with no line yet (first frame) is skipped.
#[must_use]
pub fn mod_widget_views<'a>(
    widgets: impl Iterator<Item = HudWidget<'a>>,
    font: &FontAtlas,
    display: (u32, u32),
) -> Vec<ModWidgetView> {
    let scale = gui_scale(display.0, display.1) as f32;
    let screen = (display.0 as f32 / scale, display.1 as f32 / scale);
    widgets
        .filter(|widget| !widget.content.lines().is_empty())
        .map(|widget| {
            let lines: Vec<ModLine> = widget
                .content
                .lines()
                .iter()
                .map(|line| ModLine::new(line.text.as_str(), line.color))
                .collect();
            let style = widget_style(&widget.content.style);
            // Measured at scale 1: `HudPlacement::rect` applies the widget scale.
            let size = measure_mod_widget(font, &lines, 1.0, &style);
            let rect = widget.placement.rect(size, screen);
            ModWidgetView {
                lines,
                position: [rect.x, rect.y],
                scale: widget.placement.scale,
                style,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use umicraft_builtin_mods::{mods, Anchor, HudPlacement, ModContext, ModRegistry};

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
        mod_widget_views(registry.hud_widgets(), &FontAtlas::fallback(), (800, 600))
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
        let view = &mod_widget_views(registry.hud_widgets(), &FontAtlas::fallback(), (800, 600))[0];
        assert_eq!(view.style.background, Some([1.0, 0.0, 0.0, 0.5]));
        assert_eq!(view.style.padding, 4.0);
        assert_eq!(view.style.shadow, ShadowStyle::Off);
    }
}
