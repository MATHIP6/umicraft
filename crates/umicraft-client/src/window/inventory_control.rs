// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the inventory, container and text screens: winit input
//! becomes slot clicks, container actions and sign or book edits for the
//! game thread.
//!
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use std::time::Instant;

use umicraft_game::inventory::{drag_preview, DragPreview, WindowKind, WindowView};
use umicraft_ui::{Rect, ScreenLayout};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::{App, CARET_BLINK_MS};
use crate::anvil_field::Edit;
use crate::game::GameEvent;
use crate::inventory_input::{Button, ContainerAction, Context, InventoryAction};
use crate::inventory_screen::{screen_input, ScreenFacts};
use crate::inventory_winit;
use crate::options::Action;
use crate::screen::Screen;
use crate::text_screens::{Done, Key, TextScreens};

impl App {
    /// Handles a key press while the inventory or a container screen is open.
    pub(super) fn handle_inventory_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let anvil = self.anvil_open();
        // The anvil name field takes the inventory key and the digits as text.
        let closes =
            code == KeyCode::Escape || (!anvil && self.options.keys.get(Action::Inventory) == code);
        if event.state == ElementState::Pressed && anvil && !closes && self.anvil_key(&event) {
            return;
        }
        if event.state == ElementState::Pressed && !event.repeat && closes {
            self.close_inventory();
        }
        if event.state == ElementState::Pressed && !event.repeat {
            self.inventory_key(code);
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
        let _ = event_loop;
    }
    /// What the container screens need beyond the window: the player's level
    /// and game mode and, for an anvil, the name field (kept in step with the
    /// first slot, which sends the rename when its stack changed).
    pub(super) fn screen_facts(&mut self, view: &WindowView) -> ScreenFacts {
        let anvil = view
            .container
            .as_ref()
            .filter(|container| container.kind == WindowKind::Anvil);
        let name = anvil.map(|container| {
            let first = container.slots.first().and_then(Option::as_ref);
            let rename = self.anvil_field.sync(first);
            (rename, self.anvil_field.view())
        });
        if let Some((Some(rename), _)) = &name {
            self.send_container(ContainerAction::Rename(rename.clone()));
        }
        ScreenFacts {
            drag: None,
            xp_level: self.world.as_ref().map_or(0, |world| world.hud().level),
            creative: self
                .world
                .as_ref()
                .is_some_and(|world| world.status().gamemode == 1),
            anvil: name.map(|(_, view)| view),
            caret_visible: (self.started.elapsed().as_millis() / CARET_BLINK_MS).is_multiple_of(2),
        }
    }
    /// The facts of the open inventory screen; the default when none is open.
    pub(super) fn inventory_facts(&mut self) -> ScreenFacts {
        if self.screen() != Screen::Inventory {
            return ScreenFacts::default();
        }
        let view = self.world.as_ref().map(|world| world.window_view().clone());
        view.map(|view| self.screen_facts(&view))
            .unwrap_or_default()
    }
    /// Sends a container screen action to the game thread.
    pub(super) fn send_container(&self, action: ContainerAction) {
        self.send_event(GameEvent::Container(action));
    }
    /// Whether an anvil screen is open, whose name field takes the keys.
    pub(super) fn anvil_open(&self) -> bool {
        self.screen() == Screen::Inventory
            && self.world.as_ref().is_some_and(|world| {
                world
                    .window_view()
                    .container
                    .as_ref()
                    .is_some_and(|container| container.kind == WindowKind::Anvil)
            })
    }
    /// Feeds a key press to the anvil name field; returns whether it took it.
    pub(super) fn anvil_key(&mut self, event: &KeyEvent) -> bool {
        let PhysicalKey::Code(code) = event.physical_key else {
            return false;
        };
        let edit = match code {
            KeyCode::Backspace => Some(Edit::Backspace),
            KeyCode::Delete => Some(Edit::Delete),
            KeyCode::ArrowLeft => Some(Edit::Left),
            KeyCode::ArrowRight => Some(Edit::Right),
            KeyCode::Home => Some(Edit::Home),
            KeyCode::End => Some(Edit::End),
            _ => event
                .text
                .as_deref()
                .filter(|text| !self.ctrl_held && !text.chars().any(char::is_control))
                .map(Edit::Insert),
        };
        let Some(edit) = edit else {
            return false;
        };
        if let Some(rename) = self.anvil_field.edit(edit) {
            self.send_container(ContainerAction::Rename(rename));
        }
        true
    }
    /// Closes the inventory or container screen.
    pub(super) fn close_inventory(&mut self) {
        if matches!(self.screen(), Screen::Inventory | Screen::Creative) {
            self.send_event(GameEvent::CloseInventory);
        }
        self.inventory_input.reset();
        self.creative_input.reset();
        self.set_screen(Screen::None);
        self.ui_input.clear();
    }
    /// The layout and input context of the open inventory screen, if any.
    pub(super) fn inventory_screen(&self) -> Option<(ScreenLayout, Context)> {
        if self.screen() != Screen::Inventory {
            return None;
        }
        let size = self.window.as_ref()?.inner_size();
        let viewport = Rect::new(0.0, 0.0, size.width as f32, size.height as f32);
        let world = self.world.as_ref()?;
        Some(screen_input(
            viewport,
            world.window_view(),
            world.status().gamemode,
        ))
    }
    /// The stacks a drag split in progress will leave in the slots.
    pub(super) fn inventory_drag_preview(&self, view: &WindowView) -> Option<DragPreview> {
        let (slots, button) = self.inventory_input.drag_in_progress()?;
        let creative = self.world.as_ref()?.status().gamemode == 1;
        drag_preview(view, slots, button, creative)
    }
    /// Sends the slot interaction of the inventory input to the game thread.
    pub(super) fn send_inventory(&self, action: Option<InventoryAction>) {
        if let Some(action) = action {
            self.send_event(GameEvent::Inventory(action));
        }
    }
    /// Feeds a pointer move to the inventory input.
    pub(super) fn inventory_pointer(&mut self, position: PhysicalPosition<f64>) {
        if let Some((layout, context)) = self.inventory_screen() {
            let point = (position.x as f32, position.y as f32);
            self.inventory_input.pointer_moved(&layout, point, context);
        }
    }
    /// Feeds a mouse button to the inventory input.
    pub(super) fn inventory_button(&mut self, state: ElementState, button: MouseButton) {
        let (Some((layout, context)), Some(button)) =
            (self.inventory_screen(), inventory_winit::button(button))
        else {
            return;
        };
        if state == ElementState::Pressed && button == Button::Left {
            if let Some(hotspot) = layout.hotspot_at(self.pointer) {
                self.send_container(ContainerAction::Press(hotspot));
                return;
            }
        }
        let action = match state {
            ElementState::Pressed => {
                self.inventory_input
                    .button_pressed(&layout, button, context, Instant::now())
            }
            ElementState::Released => self
                .inventory_input
                .button_released(&layout, button, context),
        };
        self.send_inventory(action);
    }
    /// Feeds a key press (number keys, Q) to the inventory input.
    pub(super) fn inventory_key(&mut self, code: KeyCode) {
        let (Some((layout, context)), Some(key)) =
            (self.inventory_screen(), inventory_winit::key(code))
        else {
            return;
        };
        let action = self.inventory_input.key_pressed(&layout, key, context);
        self.send_inventory(action);
    }
    /// Opens the book or sign screen the game thread asked for.
    pub(super) fn open_text_screen(&mut self) {
        let Some(request) = self.world.as_mut().and_then(|w| w.take_text_request()) else {
            return;
        };
        if let Some(screen) = self.text_screens.open(request) {
            self.set_screen(screen);
        }
    }
    /// Ends the book or sign screen, sending what it produced.
    pub(super) fn finish_text_screen(&mut self, done: Done) {
        if let Some(event) = done.0 {
            self.send_event(event);
        }
        self.text_screens = TextScreens::default();
        self.set_screen(Screen::None);
    }
    /// Handles a key press while the book or sign screen is open.
    pub(super) fn handle_text_screen_key(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let text = event
            .text
            .as_deref()
            .filter(|_| !self.ctrl_held)
            .filter(|text| !text.chars().any(char::is_control));
        let key = match code {
            KeyCode::Escape => Key::Escape,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
            KeyCode::ArrowUp => Key::Up,
            KeyCode::ArrowDown => Key::Down,
            _ => match text {
                Some(text) => Key::Text(text),
                None => return,
            },
        };
        let Some(atlas) = self.ui_atlas.as_ref() else {
            return;
        };
        let measure = |text: &str| atlas.measure(text, 1.0);
        if let Some(done) = self.text_screens.key(key, &measure) {
            self.finish_text_screen(done);
        }
    }
}
