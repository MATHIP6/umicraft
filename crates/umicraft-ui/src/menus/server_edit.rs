// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Add/Edit Server screen (MCP `GuiScreenAddServer`) and the Direct
//! Connect screen (`GuiScreenServerList`).
//!
//! Add/Edit, GUI units: title at `y = 17`; name label at `y = 53` and field
//! `(cx - 100, 66)` 200x20; address label at `y = 94` and field
//! `(cx - 100, 106)`; Done at `(cx - 100, height / 4 + 114)` and Cancel at
//! `height / 4 + 138`. Direct Connect: title at `y = 20`, label at `y = 100`,
//! field `(cx - 100, 116)`, Join at `height / 4 + 108`, Cancel at
//! `height / 4 + 132`. Vanilla limits the address to 128 characters and
//! enables the first button only when the address (and for Add/Edit the
//! name) is not empty; Umicraft asks the caller's `is_valid` for the address.
//! Add/Edit also has the `Server Resource Packs: <mode>` button at
//! `(cx - 100, height / 4 + 72)`, which cycles [`ResourcePackMode`].

use umicraft_core::lang::Lang;

use super::background::draw_menu_background;
use crate::context::Ui;
use crate::geom::Rect;
use crate::input::Key;
use crate::text::FontAtlas;
use crate::widgets::{text_field_scaled, TextFieldState};

use super::common::{escape_pressed, key_pressed, Gui, TextPlacement, LABEL_GRAY};
use super::pack_mode::ResourcePackMode;

/// The address length limit of `GuiTextField.setMaxStringLength(128)`.
const MAX_ADDRESS_CHARS: usize = 128;

/// Which of the two screens is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// A new entry: name and address, title `addServer.title`.
    Add,
    /// An existing entry: same screen, prefilled.
    Edit,
    /// A one-off address: no name, title `selectServer.direct`.
    DirectConnect,
}

/// What the player did on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEditOutcome {
    /// The first button: save (Add/Edit) or join (Direct Connect).
    /// `name` is empty in Direct Connect mode.
    Done {
        /// The server name.
        name: String,
        /// The server address.
        address: String,
        /// The resource pack mode the button was left on.
        pack_mode: ResourcePackMode,
    },
    /// Cancel or Escape.
    Cancel,
}

/// The rectangles of the screen for a mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerEditLayout {
    /// The name field (a zero rectangle in Direct Connect mode).
    pub name: Rect,
    /// The address field.
    pub address: Rect,
    /// The server resource packs button (a zero rectangle in Direct Connect
    /// mode).
    pub packs: Rect,
    /// The Done / Join button.
    pub done: Rect,
    /// The Cancel button.
    pub cancel: Rect,
}

/// Computes the vanilla layout of `mode` for `viewport`.
#[must_use]
pub fn server_edit_layout(viewport: Rect, mode: EditMode) -> ServerEditLayout {
    let gui = Gui::new(viewport);
    let (x, quarter) = (gui.cx() - 100.0, gui.h_div(4.0));
    if mode == EditMode::DirectConnect {
        return ServerEditLayout {
            name: Rect::ZERO,
            address: gui.rect(x, 116.0, 200.0, 20.0),
            packs: Rect::ZERO,
            done: gui.rect(x, quarter + 108.0, 200.0, 20.0),
            cancel: gui.rect(x, quarter + 132.0, 200.0, 20.0),
        };
    }
    ServerEditLayout {
        name: gui.rect(x, 66.0, 200.0, 20.0),
        address: gui.rect(x, 106.0, 200.0, 20.0),
        packs: gui.rect(x, quarter + 72.0, 200.0, 20.0),
        done: gui.rect(x, quarter + 114.0, 200.0, 20.0),
        cancel: gui.rect(x, quarter + 138.0, 200.0, 20.0),
    }
}

/// The state of the edit screen: the mode, the two fields and the validator.
#[derive(Debug, Clone)]
pub struct ServerEditScreen {
    mode: EditMode,
    name: TextFieldState,
    address: TextFieldState,
    pack_mode: ResourcePackMode,
    is_valid: fn(&str) -> bool,
    focused: bool,
}

impl ServerEditScreen {
    /// A screen for a new server, with the default `name` (vanilla uses
    /// `selectServer.defaultName`; the caller resolves it).
    #[must_use]
    pub fn add(name: &str, is_valid: fn(&str) -> bool) -> Self {
        Self::build(EditMode::Add, name, "", is_valid)
    }

    /// A screen editing an existing entry.
    #[must_use]
    pub fn edit(name: &str, address: &str, is_valid: fn(&str) -> bool) -> Self {
        Self::build(EditMode::Edit, name, address, is_valid)
    }

    /// A Direct Connect screen prefilled with the last address.
    #[must_use]
    pub fn direct_connect(last_address: &str, is_valid: fn(&str) -> bool) -> Self {
        Self::build(EditMode::DirectConnect, "", last_address, is_valid)
    }

    fn build(mode: EditMode, name: &str, address: &str, is_valid: fn(&str) -> bool) -> Self {
        Self {
            mode,
            name: TextFieldState::with_text(name),
            address: TextFieldState::with_text(address),
            pack_mode: ResourcePackMode::default(),
            is_valid,
            focused: false,
        }
    }

    /// The screen with the resource pack button set to `mode` (an entry
    /// being edited keeps its own).
    #[must_use]
    pub fn with_pack_mode(mut self, mode: ResourcePackMode) -> Self {
        self.pack_mode = mode;
        self
    }

    /// The mode of this screen.
    #[must_use]
    pub fn mode(&self) -> EditMode {
        self.mode
    }

    /// Whether the first button is enabled: the address is valid and, except
    /// in Direct Connect, the name is not empty.
    #[must_use]
    pub fn can_submit(&self) -> bool {
        let named = self.mode == EditMode::DirectConnect || !self.name.is_empty();
        named && (self.is_valid)(self.address.text())
    }

    /// Advances the caret blink of both fields by one game tick.
    pub fn tick(&mut self) {
        self.name.tick();
        self.address.tick();
    }

    /// Draws one frame and returns the player's choice.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
    ) -> Option<ServerEditOutcome> {
        draw_menu_background(ui);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = server_edit_layout(viewport, self.mode);
        self.draw_labels(ui, atlas, lang, &gui);

        ui.push_id("server-edit");
        let (name_id, address_id) = (ui.id("name"), ui.id("address"));
        if !self.focused {
            self.focused = true;
            let first = if self.mode == EditMode::DirectConnect {
                address_id
            } else {
                name_id
            };
            ui.set_focus(Some(first));
        }
        if self.mode != EditMode::DirectConnect {
            text_field_scaled(ui, name_id, layout.name, atlas, &mut self.name, gui.scale);
        }
        text_field_scaled(
            ui,
            address_id,
            layout.address,
            atlas,
            &mut self.address,
            gui.scale,
        );
        self.limit_address();

        if self.mode != EditMode::DirectConnect {
            let packs_id = ui.id("packs");
            let label = format!(
                "{}: {}",
                lang.tr("addServer.resourcePack"),
                lang.tr(self.pack_mode.lang_key())
            );
            if gui.button(ui, packs_id, layout.packs, atlas, &label, true) {
                self.pack_mode = self.pack_mode.next();
            }
        }
        let (done_id, cancel_id) = (ui.id("done"), ui.id("cancel"));
        let done_key = if self.mode == EditMode::DirectConnect {
            "selectServer.select"
        } else {
            "addServer.add"
        };
        let enabled = self.can_submit();
        let done = gui.button(ui, done_id, layout.done, atlas, lang.tr(done_key), enabled);
        let cancel = gui.button(
            ui,
            cancel_id,
            layout.cancel,
            atlas,
            lang.tr("gui.cancel"),
            true,
        );
        ui.pop_id();

        if enabled && (done || key_pressed(ui, Key::Enter)) {
            return Some(ServerEditOutcome::Done {
                name: self.name.text().to_string(),
                address: self.address.text().to_string(),
                pack_mode: self.pack_mode,
            });
        }
        (cancel || escape_pressed(ui)).then_some(ServerEditOutcome::Cancel)
    }

    fn draw_labels(&self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang, gui: &Gui) {
        let x = gui.cx() - 100.0;
        let label = |ui: &mut Ui, key: &str, y: f32| {
            gui.text(
                ui,
                atlas,
                lang.tr(key),
                TextPlacement::left([x, y], LABEL_GRAY),
            );
        };
        if self.mode == EditMode::DirectConnect {
            gui.title(ui, atlas, lang.tr("selectServer.direct"), gui.cx(), 20.0);
            label(ui, "addServer.enterIp", 100.0);
        } else {
            gui.title(ui, atlas, lang.tr("addServer.title"), gui.cx(), 17.0);
            label(ui, "addServer.enterName", 53.0);
            label(ui, "addServer.enterIp", 94.0);
        }
    }

    fn limit_address(&mut self) {
        if self.address.text().chars().count() > MAX_ADDRESS_CHARS {
            let kept: String = self
                .address
                .text()
                .chars()
                .take(MAX_ADDRESS_CHARS)
                .collect();
            self.address.set_text(kept);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, UiInput};
    use crate::menus::common::testing::{click_at, frame, key, VIEWPORT};

    fn not_empty(address: &str) -> bool {
        !address.is_empty()
    }

    fn run(screen: &mut ServerEditScreen, input: UiInput) -> Option<ServerEditOutcome> {
        frame(input, |ui| {
            screen.show(ui, &FontAtlas::fallback(), &Lang::builtin())
        })
    }

    #[test]
    fn done_returns_the_fields_when_valid() {
        let mut screen = ServerEditScreen::edit("Home", "play.example.org", not_empty);
        let layout = server_edit_layout(VIEWPORT, EditMode::Edit);
        assert_eq!(
            run(&mut screen, click_at(layout.done)),
            Some(ServerEditOutcome::Done {
                name: "Home".into(),
                address: "play.example.org".into(),
                pack_mode: ResourcePackMode::Prompt,
            })
        );
    }

    #[test]
    fn done_is_disabled_for_an_invalid_address_or_empty_name() {
        let layout = server_edit_layout(VIEWPORT, EditMode::Add);
        let mut no_address = ServerEditScreen::add("Home", not_empty);
        assert!(!no_address.can_submit());
        assert_eq!(run(&mut no_address, click_at(layout.done)), None);
        let mut no_name = ServerEditScreen::edit("", "host", not_empty);
        assert_eq!(run(&mut no_name, click_at(layout.done)), None);
    }

    #[test]
    fn direct_connect_needs_no_name_and_enter_joins() {
        let mut screen = ServerEditScreen::direct_connect("host:25565", not_empty);
        assert_eq!(
            run(&mut screen, key(Key::Enter)),
            Some(ServerEditOutcome::Done {
                name: String::new(),
                address: "host:25565".into(),
                pack_mode: ResourcePackMode::Prompt,
            })
        );
    }

    #[test]
    fn the_pack_button_cycles_and_done_returns_the_mode() {
        let layout = server_edit_layout(VIEWPORT, EditMode::Edit);
        // scale 3, height / 4 = 60: the vanilla position (cx - 100, 60 + 72).
        assert_eq!(layout.packs, Rect::new(339.0, 132.0 * 3.0, 600.0, 60.0));
        let mut screen = ServerEditScreen::edit("Home", "host", not_empty)
            .with_pack_mode(ResourcePackMode::Enabled);
        assert_eq!(run(&mut screen, click_at(layout.packs)), None);
        assert_eq!(screen.pack_mode, ResourcePackMode::Disabled);
        assert_eq!(run(&mut screen, click_at(layout.packs)), None);
        let done = run(&mut screen, click_at(layout.done));
        assert!(matches!(
            done,
            Some(ServerEditOutcome::Done {
                pack_mode: ResourcePackMode::Prompt,
                ..
            })
        ));
    }

    #[test]
    fn the_pack_button_shows_the_mode_and_direct_connect_has_none() {
        let labels = |mode: EditMode| {
            let mut screen = match mode {
                EditMode::DirectConnect => ServerEditScreen::direct_connect("h", not_empty),
                _ => ServerEditScreen::edit("n", "h", not_empty),
            };
            frame(UiInput::default(), |ui| {
                screen.show(ui, &FontAtlas::fallback(), &Lang::builtin());
                ui.draw()
                    .commands()
                    .iter()
                    .filter_map(|c| match c {
                        crate::draw::DrawCommand::Text { text, .. } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
        };
        assert!(labels(EditMode::Edit)
            .iter()
            .any(|t| t == "Server Resource Packs: Prompt"));
        assert!(!labels(EditMode::DirectConnect)
            .iter()
            .any(|t| t.starts_with("Server Resource Packs")));
    }

    #[test]
    fn cancel_and_escape_cancel() {
        let layout = server_edit_layout(VIEWPORT, EditMode::Add);
        let mut screen = ServerEditScreen::add("n", not_empty);
        assert_eq!(
            run(&mut screen, click_at(layout.cancel)),
            Some(ServerEditOutcome::Cancel)
        );
        assert_eq!(
            run(&mut screen, key(Key::Escape)),
            Some(ServerEditOutcome::Cancel)
        );
    }

    #[test]
    fn typing_goes_to_the_focused_name_field_and_the_address_is_limited() {
        let mut screen = ServerEditScreen::add("", not_empty);
        let mut ui = Ui::new(VIEWPORT);
        for input in [
            UiInput::default(),
            UiInput::new(vec![InputEvent::Text("Abc".into())]),
        ] {
            ui.begin_frame(VIEWPORT, input);
            screen.show(&mut ui, &FontAtlas::fallback(), &Lang::builtin());
            ui.end_frame();
        }
        assert_eq!(screen.name.text(), "Abc");

        let mut long = ServerEditScreen::direct_connect(&"a".repeat(200), not_empty);
        run(&mut long, UiInput::default());
        assert_eq!(long.address.text().chars().count(), MAX_ADDRESS_CHARS);
    }

    #[test]
    fn the_layouts_are_the_vanilla_ones() {
        // scale 3, cx = 213, height / 4 = 60
        let add = server_edit_layout(VIEWPORT, EditMode::Add);
        assert_eq!(add.name, Rect::new(339.0, 198.0, 600.0, 60.0));
        assert_eq!(add.done.y, (60.0 + 114.0) * 3.0);
        let direct = server_edit_layout(VIEWPORT, EditMode::DirectConnect);
        assert_eq!(direct.address.y, 348.0);
        assert_eq!(direct.cancel.y, (60.0 + 132.0) * 3.0);
    }
}
