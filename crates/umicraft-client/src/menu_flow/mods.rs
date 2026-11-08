// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods menu inside the menu flow: its blur and its frame. The design is
//! in `umicraft_ui::menus::mods`; the keys and the shortcut that open it are in
//! `window/mods_menu.rs`, which also applies the changes the player makes.

use umicraft_core::lang::Lang;
use umicraft_ui::menus::ModsOutcome;
use umicraft_ui::{FontAtlas, Ui};

use super::{MenuCommand, MenuContext, MenuFlow};
use crate::hud_editor_view::snap;
use crate::screen::Screen;

impl MenuFlow {
    /// How blurred the game behind the mods menu is, `0.0` to `1.0`: it fades
    /// in while the menu opens.
    #[must_use]
    pub fn mods_blur(&self) -> f32 {
        self.mods.blur_strength(self.partial_tick())
    }

    /// Whether Escape steps back inside the mods menu (settings to list, list
    /// to landing page) instead of closing it.
    #[must_use]
    pub fn mods_can_go_back(&self) -> bool {
        self.mods.can_go_back()
    }

    /// The mods menu over the game. Changes (a mod switched, a setting, a HUD
    /// widget moved) leave as a command; the cross of the landing page closes
    /// the menu.
    pub(super) fn show_mods(
        &mut self,
        ui: &mut Ui,
        (atlas, lang): (&FontAtlas, &Lang),
        context: &MenuContext<'_>,
    ) -> Option<MenuCommand> {
        let partial_tick = self.partial_tick();
        let fonts = (atlas, context.brand_font);
        let shown = self
            .mods
            .show(ui, fonts, lang, context.mods, (&snap, partial_tick));
        match shown? {
            ModsOutcome::Close => Some(MenuCommand::GoTo(Screen::None)),
            change => Some(MenuCommand::Mods(change)),
        }
    }
}

#[cfg(test)]
mod tests {
    use umicraft_ui::menus::{
        landing_layout, mods_layout, HudEditorOutcome, HudEditorView, HudWidgetView, ModsView,
        TitleStyle,
    };
    use umicraft_ui::mod_widget::{ModLine, ModWidgetView};
    use umicraft_ui::{InputEvent, PointerButton, Rect};

    use super::super::tests::{flow_with, frame_core};
    use super::*;

    const VIEWPORT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 720.0,
    };

    fn mods_with_a_widget() -> ModsView {
        ModsView {
            hud: HudEditorView {
                widgets: vec![HudWidgetView {
                    id: "fps".into(),
                    name: "FPS".into(),
                    widget: ModWidgetView {
                        lines: vec![ModLine::new("60 FPS", 0xFFFFFF)],
                        position: [100.0, 100.0],
                        scale: 1.0,
                    },
                }],
                screen: [427.0, 240.0],
                scale_range: (0.5, 3.0),
            },
            ..ModsView::default()
        }
    }

    fn middle(rect: Rect) -> [f32; 2] {
        [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
    }

    fn click(at: [f32; 2]) -> Vec<InputEvent> {
        vec![
            InputEvent::PointerMoved { position: at },
            InputEvent::PointerPressed {
                position: at,
                button: PointerButton::Left,
            },
            InputEvent::PointerReleased {
                position: at,
                button: PointerButton::Left,
            },
        ]
    }

    fn frame(
        flow: &mut super::MenuFlow,
        events: Vec<InputEvent>,
        mods: &ModsView,
    ) -> Option<MenuCommand> {
        frame_core(flow, Screen::Mods, events, (&[], TitleStyle::Classic), mods).command
    }

    #[test]
    fn the_cross_of_the_landing_closes_the_menu() {
        let (mut flow, _) = flow_with("mods-close", &[]);
        let cross = middle(landing_layout(VIEWPORT).close);
        let command = frame(&mut flow, click(cross), &ModsView::default());
        assert_eq!(command, Some(MenuCommand::GoTo(Screen::None)));
    }

    #[test]
    fn the_mods_button_opens_the_list_and_the_menu_can_go_back() {
        let (mut flow, _) = flow_with("mods-list", &[]);
        assert!(!flow.mods_can_go_back());
        let button = middle(landing_layout(VIEWPORT).mods);
        assert_eq!(frame(&mut flow, click(button), &ModsView::default()), None);
        assert!(flow.mods_can_go_back());
        // The cross of the panel is not the menu's close.
        let cross = middle(mods_layout(VIEWPORT).close);
        assert_eq!(frame(&mut flow, click(cross), &ModsView::default()), None);
        assert!(!flow.mods_can_go_back());
    }

    #[test]
    fn dropping_a_widget_on_the_landing_becomes_a_change_to_apply() {
        let (mut flow, _) = flow_with("mods-drop", &[]);
        let widget_at = [(100.0 + 8.0) * 3.0, (100.0 + 5.0) * 3.0];
        let to = [widget_at[0] + 90.0, widget_at[1] + 60.0];
        let events = vec![
            InputEvent::PointerMoved {
                position: widget_at,
            },
            InputEvent::PointerPressed {
                position: widget_at,
                button: PointerButton::Left,
            },
            InputEvent::PointerMoved { position: to },
            InputEvent::PointerReleased {
                position: to,
                button: PointerButton::Left,
            },
        ];
        let command = frame(&mut flow, events, &mods_with_a_widget());
        let Some(MenuCommand::Mods(ModsOutcome::Hud(HudEditorOutcome::Placed {
            id, rect, ..
        }))) = command
        else {
            panic!("expected a placement, got {command:?}");
        };
        assert_eq!(id, "fps");
        assert_eq!((rect.x, rect.y), (130.0, 120.0));
    }
}
