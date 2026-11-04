// SPDX-License-Identifier: MIT OR Apache-2.0

//! The controls page (`GuiControls`): mouse options and the key list.

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, TextStyle, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W, KEY_ROW, RED, YELLOW};
use super::{on_off, option_label, OptionsScreen, Outcome};
use crate::options::{key_label, Action, Options};

/// The language key of the name of an action (`KeyBinding` description).
fn action_key(action: Action) -> &'static str {
    match action {
        Action::Forward => "key.forward",
        Action::Back => "key.back",
        Action::Left => "key.left",
        Action::Right => "key.right",
        Action::Jump => "key.jump",
        Action::Sneak => "key.sneak",
        Action::Sprint => "key.sprint",
        Action::Inventory => "key.inventory",
        Action::Chat => "key.chat",
        Action::Command => "key.command",
        Action::PlayerList => "key.playerlist",
        Action::Hotbar1 => "key.hotbar.1",
        Action::Hotbar2 => "key.hotbar.2",
        Action::Hotbar3 => "key.hotbar.3",
        Action::Hotbar4 => "key.hotbar.4",
        Action::Hotbar5 => "key.hotbar.5",
        Action::Hotbar6 => "key.hotbar.6",
        Action::Hotbar7 => "key.hotbar.7",
        Action::Hotbar8 => "key.hotbar.8",
        Action::Hotbar9 => "key.hotbar.9",
    }
}

/// One entry of the key list, in the order `GuiKeyBindingList` sorts them
/// (categories, then descriptions, both alphabetically).
#[derive(Debug, Clone, Copy)]
enum KeyEntry {
    /// A category heading, by language key.
    Category(&'static str),
    Key(Action),
}

const KEY_ENTRIES: [KeyEntry; 23] = [
    KeyEntry::Category("key.categories.inventory"),
    KeyEntry::Key(Action::Hotbar1),
    KeyEntry::Key(Action::Hotbar2),
    KeyEntry::Key(Action::Hotbar3),
    KeyEntry::Key(Action::Hotbar4),
    KeyEntry::Key(Action::Hotbar5),
    KeyEntry::Key(Action::Hotbar6),
    KeyEntry::Key(Action::Hotbar7),
    KeyEntry::Key(Action::Hotbar8),
    KeyEntry::Key(Action::Hotbar9),
    KeyEntry::Key(Action::Inventory),
    KeyEntry::Category("key.categories.movement"),
    KeyEntry::Key(Action::Jump),
    KeyEntry::Key(Action::Sneak),
    KeyEntry::Key(Action::Sprint),
    KeyEntry::Key(Action::Left),
    KeyEntry::Key(Action::Right),
    KeyEntry::Key(Action::Back),
    KeyEntry::Key(Action::Forward),
    KeyEntry::Category("key.categories.multiplayer"),
    KeyEntry::Key(Action::PlayerList),
    KeyEntry::Key(Action::Chat),
    KeyEntry::Key(Action::Command),
];

impl OptionsScreen {
    /// `GuiControls`.
    pub(super) fn show_controls(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (63.0, gui.height - 32.0);
        let cx = gui.cx();
        let content = KEY_ENTRIES.len() as f32 * KEY_ROW;
        self.scroll_list(ui, content, bottom - top, KEY_ROW);
        Self::list_background(ui, gui, top, bottom);

        let label_width = Action::ALL
            .iter()
            .map(|action| atlas.measure(lang.tr(action_key(*action)), 1.0))
            .fold(0.0_f32, f32::max);
        let x = cx - 110.0 + 2.0;
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for (index, entry) in KEY_ENTRIES.iter().enumerate() {
            let y = top + 4.0 - self.scroll + index as f32 * KEY_ROW;
            match *entry {
                KeyEntry::Category(key) => {
                    let name = lang.tr(key);
                    let width = atlas.measure(name, 1.0);
                    let at = [
                        ((cx - width * 0.5) * gui.scale).round(),
                        ((y + KEY_ROW - 9.0 - 1.0) * gui.scale).round(),
                    ];
                    ui.draw()
                        .text(at, name, TextStyle::new(gui.scale, [1.0; 4]));
                }
                KeyEntry::Key(action) => {
                    let inside = y >= top && y + BUTTON_H <= bottom;
                    self.key_row(
                        ui,
                        atlas,
                        options,
                        gui,
                        lang,
                        action,
                        (x, y),
                        label_width,
                        inside,
                    );
                }
            }
        }
        ui.pop_clip();
        Self::scrollbar(
            ui,
            gui,
            cx + 124.0 + 32.0,
            (top, bottom),
            content,
            self.scroll,
        );

        // The mouse settings above the list.
        let invert = gui.rect(cx - 155.0, 18.0, HALF_W, BUTTON_H);
        let label = option_label(
            lang,
            "options.invertMouse",
            on_off(lang, options.controls.invert_mouse),
        );
        if self.button(ui, atlas, "invert", invert, &label, true, None) {
            options.controls.invert_mouse = !options.controls.invert_mouse;
            outcome.changed = true;
        }
        let sensitivity = gui.rect(cx + 5.0, 18.0, HALF_W, BUTTON_H);
        let label = sensitivity_label(lang, options.controls.mouse_sensitivity);
        if let Some(value) = self.slider(
            ui,
            atlas,
            "sens",
            sensitivity,
            &label,
            options.controls.mouse_sensitivity,
        ) {
            options.controls.mouse_sensitivity = value;
            outcome.changed = true;
        }
        let touch = gui.rect(cx - 155.0, 42.0, HALF_W, BUTTON_H);
        self.button(
            ui,
            atlas,
            "touch",
            touch,
            &option_label(lang, "options.touchscreen", on_off(lang, false)),
            true,
            None,
        );

        Self::title(ui, atlas, gui, lang.tr("controls.title"), 8.0);

        let all_default = Action::ALL
            .iter()
            .all(|action| options.keys.get(*action) == action.default_key());
        let done = gui.rect(cx - 155.0, gui.height - 29.0, HALF_W, BUTTON_H);
        self.back(ui, atlas, done, lang);
        let reset = gui.rect(cx - 155.0 + 160.0, gui.height - 29.0, HALF_W, BUTTON_H);
        if self.button(
            ui,
            atlas,
            "reset",
            reset,
            lang.tr("controls.resetAll"),
            !all_default,
            None,
        ) {
            options.keys.reset();
            self.capturing = None;
            outcome.changed = true;
        }
    }

    /// One `GuiKeyBindingList.KeyEntry`: label, key button, reset button.
    #[allow(clippy::too_many_arguments)]
    fn key_row(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        action: Action,
        (x, y): (f32, f32),
        label_width: f32,
        inside: bool,
    ) {
        let name = lang.tr(action_key(action));
        let at = [
            ((x + 90.0 - label_width) * gui.scale).round(),
            ((y + KEY_ROW / 2.0 - 4.0) * gui.scale).round(),
        ];
        ui.draw()
            .text(at, name, TextStyle::new(gui.scale, [1.0; 4]));

        let waiting = self.capturing == Some(action);
        let conflicted = !options.keys.conflicts(action).is_empty();
        let key = key_label(options.keys.get(action));
        let (label, tint) = if waiting {
            (format!("> {key} <"), Some(YELLOW))
        } else if conflicted {
            (key, Some(RED))
        } else {
            (key, None)
        };
        let change = gui.rect(x + 105.0, y, 75.0, BUTTON_H);
        let id = format!("key:{}", action.id());
        if self.button(ui, atlas, &id, change, &label, inside, tint) {
            self.capturing = Some(action);
        }
        let reset = gui.rect(x + 190.0, y, 50.0, BUTTON_H);
        let id = format!("keyreset:{}", action.id());
        let changed = options.keys.get(action) != action.default_key();
        if self.button(
            ui,
            atlas,
            &id,
            reset,
            lang.tr("controls.reset"),
            inside && changed,
            None,
        ) {
            options.keys.set(action, action.default_key());
            self.capturing = None;
        }
    }
}

/// `GameSettings.getKeyBinding(SENSITIVITY)`.
pub(super) fn sensitivity_label(lang: &Lang, value: f32) -> String {
    let value = if value == 0.0 {
        lang.tr("options.sensitivity.min").to_string()
    } else if value == 1.0 {
        lang.tr("options.sensitivity.max").to_string()
    } else {
        format!("{}%", (value * 200.0) as i32)
    };
    option_label(lang, "options.sensitivity", &value)
}
