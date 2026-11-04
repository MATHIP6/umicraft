// SPDX-License-Identifier: MIT OR Apache-2.0

//! The controls page (`GuiControls`): mouse options and the key list.

use umicraft_ui::{FontAtlas, TextStyle, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W, KEY_ROW, RED, YELLOW};
use super::{OptionsScreen, Outcome};
use crate::options::{key_label, Action, Options};

/// The vanilla name of an action (`key.*` in `en_US.lang`).
fn action_name(action: Action) -> &'static str {
    match action {
        Action::Forward => "Walk Forward",
        Action::Back => "Walk Backwards",
        Action::Left => "Strafe Left",
        Action::Right => "Strafe Right",
        Action::Jump => "Jump",
        Action::Sneak => "Sneak",
        Action::Sprint => "Sprint",
        Action::Inventory => "Inventory",
        Action::Chat => "Open Chat",
        Action::Command => "Open Command",
        Action::PlayerList => "List Players",
        Action::Hotbar1 => "Hotbar Slot 1",
        Action::Hotbar2 => "Hotbar Slot 2",
        Action::Hotbar3 => "Hotbar Slot 3",
        Action::Hotbar4 => "Hotbar Slot 4",
        Action::Hotbar5 => "Hotbar Slot 5",
        Action::Hotbar6 => "Hotbar Slot 6",
        Action::Hotbar7 => "Hotbar Slot 7",
        Action::Hotbar8 => "Hotbar Slot 8",
        Action::Hotbar9 => "Hotbar Slot 9",
    }
}

/// One entry of the key list, in the order `GuiKeyBindingList` sorts them
/// (categories, then descriptions, both alphabetically).
#[derive(Debug, Clone, Copy)]
enum KeyEntry {
    Category(&'static str),
    Key(Action),
}

const KEY_ENTRIES: [KeyEntry; 23] = [
    KeyEntry::Category("Inventory"),
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
    KeyEntry::Category("Movement"),
    KeyEntry::Key(Action::Jump),
    KeyEntry::Key(Action::Sneak),
    KeyEntry::Key(Action::Sprint),
    KeyEntry::Key(Action::Left),
    KeyEntry::Key(Action::Right),
    KeyEntry::Key(Action::Back),
    KeyEntry::Key(Action::Forward),
    KeyEntry::Category("Multiplayer"),
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
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (63.0, gui.height - 32.0);
        let cx = gui.cx();
        let content = KEY_ENTRIES.len() as f32 * KEY_ROW;
        self.scroll_list(ui, content, bottom - top, KEY_ROW);
        Self::list_background(ui, gui, top, bottom);

        let label_width = Action::ALL
            .iter()
            .map(|action| atlas.measure(action_name(*action), 1.0))
            .fold(0.0_f32, f32::max);
        let x = cx - 110.0 + 2.0;
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for (index, entry) in KEY_ENTRIES.iter().enumerate() {
            let y = top + 4.0 - self.scroll + index as f32 * KEY_ROW;
            match *entry {
                KeyEntry::Category(name) => {
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
                    self.key_row(ui, atlas, options, gui, action, (x, y), label_width, inside);
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
        let label = format!(
            "Invert Mouse: {}",
            if options.controls.invert_mouse {
                "ON"
            } else {
                "OFF"
            }
        );
        if self.button(ui, atlas, "invert", invert, &label, true, None) {
            options.controls.invert_mouse = !options.controls.invert_mouse;
            outcome.changed = true;
        }
        let sensitivity = gui.rect(cx + 5.0, 18.0, HALF_W, BUTTON_H);
        let label = sensitivity_label(options.controls.mouse_sensitivity);
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
            "Touchscreen Mode: OFF",
            true,
            None,
        );

        Self::title(ui, atlas, gui, "Controls", 8.0);

        let all_default = Action::ALL
            .iter()
            .all(|action| options.keys.get(*action) == action.default_key());
        let done = gui.rect(cx - 155.0, gui.height - 29.0, HALF_W, BUTTON_H);
        self.back(ui, atlas, done);
        let reset = gui.rect(cx - 155.0 + 160.0, gui.height - 29.0, HALF_W, BUTTON_H);
        if self.button(ui, atlas, "reset", reset, "Reset Keys", !all_default, None) {
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
        action: Action,
        (x, y): (f32, f32),
        label_width: f32,
        inside: bool,
    ) {
        let name = action_name(action);
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
        if self.button(ui, atlas, &id, reset, "Reset", inside && changed, None) {
            options.keys.set(action, action.default_key());
            self.capturing = None;
        }
    }
}

/// `GameSettings.getKeyBinding(SENSITIVITY)`.
pub(super) fn sensitivity_label(value: f32) -> String {
    if value == 0.0 {
        "Sensitivity: *yawn*".to_string()
    } else if value == 1.0 {
        "Sensitivity: HYPERSPEED!!!".to_string()
    } else {
        format!("Sensitivity: {}%", (value * 200.0) as i32)
    }
}
