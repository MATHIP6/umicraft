// SPDX-License-Identifier: MIT OR Apache-2.0

//! The controls page (`GuiControls`): mouse options and the key list.

use std::borrow::Cow;

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, TextStyle, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W, KEY_ROW, RED, YELLOW};
use super::mod_keys::{mod_entries, ModKey};
use super::{on_off, option_label, OptionsScreen, Outcome};
use crate::options::{Action, Options};

/// What the key button of a mod binding shows while it is unbound. The reference
/// shows `NONE` for key code 0 (`Keyboard.getKeyName`) in every language.
const UNBOUND_LABEL: &str = "NONE";

/// One entry of the key list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyEntry<'a> {
    /// A category heading, by language key.
    Category(&'static str),
    Key(Action),
    /// The heading of a mod's bindings: the mod's name, literal text.
    ModCategory(&'a str),
    /// A binding a mod declared.
    ModKey(&'a ModKey),
}

/// The entries in the order `GuiKeyBindingList` shows them: `KeyBinding`
/// sorts by the translated category, then by the translated description, so
/// the order follows the language.
pub(super) fn key_entries(lang: &Lang) -> Vec<KeyEntry<'static>> {
    let mut actions = Action::ALL.to_vec();
    actions.sort_by(|a, b| {
        (lang.tr(a.category()), lang.tr(a.lang_key()))
            .cmp(&(lang.tr(b.category()), lang.tr(b.lang_key())))
    });
    let mut entries = Vec::with_capacity(actions.len() + 6);
    let mut last = None;
    for action in actions {
        if last != Some(action.category()) {
            last = Some(action.category());
            entries.push(KeyEntry::Category(action.category()));
        }
        entries.push(KeyEntry::Key(action));
    }
    entries
}

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
        // Taken out so the rows can borrow it while the page takes `&mut self`.
        let mod_keys = std::mem::take(&mut self.mod_keys);
        let mut entries = key_entries(lang);
        entries.extend(mod_entries(&mod_keys));
        let content = entries.len() as f32 * KEY_ROW;
        self.scroll_list(
            ui,
            gui,
            (cx + 124.0 + 32.0, top, bottom),
            (content, KEY_ROW),
        );
        Self::list_container(ui, gui, (0.0, gui.width), (top, bottom), self.scroll);

        let label_width = Action::ALL
            .iter()
            .map(|action| lang.tr(action.lang_key()))
            .chain(
                mod_keys
                    .iter()
                    .flat_map(|group| &group.keys)
                    .map(|key| key.label.as_str()),
            )
            .map(|label| atlas.measure(label, 1.0))
            .fold(0.0_f32, f32::max);
        let x = cx - 110.0 + 2.0;
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for (index, entry) in entries.iter().enumerate() {
            let y = top + 4.0 - self.scroll + index as f32 * KEY_ROW;
            let inside = y >= top && y + BUTTON_H <= bottom;
            match *entry {
                KeyEntry::Category(key) => Self::heading(ui, atlas, gui, lang.tr(key), (cx, y)),
                KeyEntry::ModCategory(name) => Self::heading(ui, atlas, gui, name, (cx, y)),
                KeyEntry::Key(action) => {
                    let waiting = self.capturing == Some(action);
                    let conflicted = !options.keys.conflicts(action).is_empty();
                    let row = KeyRow {
                        id: Cow::Borrowed(action.id()),
                        label: lang.tr(action.lang_key()),
                        key: options.keys.get(action).label(lang),
                        state: RowState::of(waiting, conflicted),
                        changed: options.keys.get(action) != action.default_binding(),
                    };
                    match self.key_row(ui, atlas, gui, lang, &row, (x, y), label_width, inside) {
                        RowClick::Rebind => {
                            self.cancel_capture();
                            self.capturing = Some(action);
                        }
                        RowClick::Reset => {
                            options.keys.set(action, action.default_binding());
                            self.cancel_capture();
                        }
                        RowClick::None => {}
                    }
                }
                KeyEntry::ModKey(key) => {
                    let conflicted = key
                        .binding()
                        .is_some_and(|binding| options.keys.actions_for(binding).next().is_some());
                    let row = KeyRow {
                        id: Cow::Owned(format!("mod:{}:{}", key.mod_id, key.key)),
                        label: &key.label,
                        key: key.binding().map_or_else(
                            || UNBOUND_LABEL.to_owned(),
                            |binding| binding.label(lang),
                        ),
                        state: RowState::of(self.is_mod_capture_of(key), conflicted),
                        changed: key.is_changed(),
                    };
                    match self.key_row(ui, atlas, gui, lang, &row, (x, y), label_width, inside) {
                        RowClick::Rebind => self.start_mod_capture(key),
                        RowClick::Reset => {
                            self.cancel_capture();
                            self.mod_key_changes.push(key.reset());
                        }
                        RowClick::None => {}
                    }
                }
            }
        }
        ui.pop_clip();
        Self::list_overlay(ui, gui, (0.0, gui.width), (top, bottom));
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

        let all_default = options.keys.is_default()
            && !mod_keys
                .iter()
                .flat_map(|g| &g.keys)
                .any(ModKey::is_changed);
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
            self.cancel_capture();
            self.mod_key_changes.extend(
                mod_keys
                    .iter()
                    .flat_map(|group| &group.keys)
                    .filter(|key| key.is_changed())
                    .map(ModKey::reset),
            );
            outcome.changed = true;
        }
        self.mod_keys = mod_keys;
    }

    /// A category heading of the key list, centred on `cx` in the row at `y`.
    fn heading(ui: &mut Ui, atlas: &FontAtlas, gui: Gui, name: &str, (cx, y): (f32, f32)) {
        let width = atlas.measure(name, 1.0);
        let at = [
            ((cx - width * 0.5) * gui.scale).round(),
            ((y + KEY_ROW - 9.0 - 1.0) * gui.scale).round(),
        ];
        ui.draw()
            .text(at, name, TextStyle::new(gui.scale, [1.0; 4]));
    }

    /// One `GuiKeyBindingList.KeyEntry`: label, key button, reset button. Draws
    /// the row of a built-in action and of a mod's binding alike and reports what
    /// was clicked.
    #[allow(clippy::too_many_arguments)]
    fn key_row(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        lang: &Lang,
        row: &KeyRow<'_>,
        (x, y): (f32, f32),
        label_width: f32,
        inside: bool,
    ) -> RowClick {
        let at = [
            ((x + 90.0 - label_width) * gui.scale).round(),
            ((y + KEY_ROW / 2.0 - 4.0) * gui.scale).round(),
        ];
        ui.draw()
            .text(at, row.label, TextStyle::new(gui.scale, [1.0; 4]));

        let (label, tint) = match row.state {
            RowState::Waiting => (format!("> {} <", row.key), Some(YELLOW)),
            RowState::Conflicted => (row.key.clone(), Some(RED)),
            RowState::Normal => (row.key.clone(), None),
        };
        let change = gui.rect(x + 105.0, y, 75.0, BUTTON_H);
        let id = format!("key:{}", row.id);
        if self.button(ui, atlas, &id, change, &label, inside, tint) {
            return RowClick::Rebind;
        }
        let reset = gui.rect(x + 190.0, y, 50.0, BUTTON_H);
        let id = format!("keyreset:{}", row.id);
        let clicked = self.button(
            ui,
            atlas,
            &id,
            reset,
            lang.tr("controls.reset"),
            inside && row.changed,
            None,
        );
        if clicked {
            RowClick::Reset
        } else {
            RowClick::None
        }
    }
}

/// How the key button of a row looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowState {
    /// Waiting for a key (yellow).
    Waiting,
    /// Bound to a key another binding uses too (red).
    Conflicted,
    Normal,
}

impl RowState {
    fn of(waiting: bool, conflicted: bool) -> Self {
        if waiting {
            Self::Waiting
        } else if conflicted {
            Self::Conflicted
        } else {
            Self::Normal
        }
    }
}

/// What a row shows, whatever it binds.
struct KeyRow<'a> {
    /// Identifies the row's buttons (`key:<id>`, `keyreset:<id>`).
    id: Cow<'a, str>,
    label: &'a str,
    /// The text of the key button.
    key: String,
    state: RowState,
    /// Whether the reset button has something to reset.
    changed: bool,
}

/// What the player clicked in a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowClick {
    None,
    Rebind,
    Reset,
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
