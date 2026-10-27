// SPDX-License-Identifier: MIT OR Apache-2.0

//! The options screen: video, controls with rebindable keys, chat, and the
//! tabs that wait for later milestones (M10, UI-3).
//!
//! [`OptionsScreen`] follows the pattern of the other screens: it draws
//! through a [`Ui`] and a [`FontAtlas`], reads no file, clock or network, and
//! edits the [`Options`] it is given. The window saves the options and tells
//! the server about the changes when the screen closes. A key rebinding is a
//! two-step flow: a click on the key button starts the capture, and the next
//! key press is handed to [`OptionsScreen::capture_key`] by the window.

use std::collections::HashMap;

use umicraft_ui::geom::Layout;
use umicraft_ui::widgets::{button, panel, slider, SliderState};
use umicraft_ui::{Color, FontAtlas, Rect, TextStyle, Ui};
use winit::keyboard::KeyCode;

use crate::options::{key_label, Action, Options, FOV, MAX_FPS, RENDER_DISTANCE, UNLIMITED_FPS};

/// Height of one row, in pixels.
const ROW: f32 = 26.0;
/// Gap between two rows, in pixels.
const GAP: f32 = 4.0;
/// The widest the panel gets, in pixels.
const MAX_WIDTH: f32 = 580.0;
/// Scale of the title.
const TITLE_SCALE: f32 = 3.0;
/// Scale of the labels.
const TEXT_SCALE: f32 = 2.0;
/// The backdrop behind the panel.
const BACKDROP: Color = [0.0, 0.0, 0.0, 0.55];
/// The fill of the panel.
const PANEL: Color = [0.05, 0.06, 0.09, 0.94];
/// The colour of titles.
const TITLE: Color = [0.98, 0.98, 1.0, 1.0];
/// The colour of notes.
const NOTE: Color = [0.55, 0.58, 0.66, 1.0];
/// The colour of a binding that shares its key with another action.
const CONFLICT: Color = [1.0, 0.33, 0.33, 1.0];

/// One tab of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Field of view, render distance and frame-rate cap.
    Video,
    /// Mouse and key bindings.
    Controls,
    /// Chat visibility and colours.
    Chat,
    /// Volumes (M11).
    Audio,
    /// The mod manager (M15).
    Mods,
    /// The account manager (M12).
    Accounts,
}

impl Tab {
    /// Every tab, left to right.
    pub const ALL: [Self; 6] = [
        Self::Video,
        Self::Controls,
        Self::Chat,
        Self::Audio,
        Self::Mods,
        Self::Accounts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Video => "VIDEO",
            Self::Controls => "CONTROLS",
            Self::Chat => "CHAT",
            Self::Audio => "AUDIO",
            Self::Mods => "MODS",
            Self::Accounts => "ACCOUNTS",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Video => "tab:video",
            Self::Controls => "tab:controls",
            Self::Chat => "tab:chat",
            Self::Audio => "tab:audio",
            Self::Mods => "tab:mods",
            Self::Accounts => "tab:accounts",
        }
    }

    /// What a tab without a backend says.
    fn waiting_note(self) -> Option<&'static str> {
        match self {
            Self::Audio => Some("AUDIO ARRIVES WITH MILESTONE M11."),
            Self::Mods => Some("THE MOD MANAGER ARRIVES WITH MILESTONE M15."),
            Self::Accounts => Some("THE ACCOUNT MANAGER ARRIVES WITH MILESTONE M12."),
            _ => None,
        }
    }
}

/// What one frame of the screen did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// An option changed in this frame.
    pub changed: bool,
    /// The player pressed Done.
    pub done: bool,
}

/// The state of the options screen.
#[derive(Debug, Clone)]
pub struct OptionsScreen {
    tab: Tab,
    capturing: Option<Action>,
    fov: SliderState,
    distance: SliderState,
    fps: SliderState,
    sensitivity: SliderState,
    /// The rectangle of every widget in the last frame, by id.
    rects: HashMap<String, Rect>,
}

impl Default for OptionsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionsScreen {
    /// Creates the screen on its first tab.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tab: Tab::Video,
            capturing: None,
            fov: SliderState::default(),
            distance: SliderState::default(),
            fps: SliderState::default(),
            sensitivity: SliderState::default(),
            rects: HashMap::new(),
        }
    }

    /// The tab that is shown.
    #[must_use]
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// The action waiting for a key, if a rebinding is under way.
    #[must_use]
    pub fn capturing(&self) -> Option<Action> {
        self.capturing
    }

    /// Gives up a rebinding without changing the key.
    pub fn cancel_capture(&mut self) {
        self.capturing = None;
    }

    /// The rectangle of a widget in the last frame (`"done"`, `"tab:chat"`,
    /// `"key:forward"`, `"fov"`, …), so a driver can click it.
    #[must_use]
    pub fn rect_of(&self, id: &str) -> Option<Rect> {
        self.rects.get(id).copied()
    }

    /// Binds `key` to the action that is waiting for one. A key the client
    /// keeps for itself is refused and the capture goes on. Returns whether
    /// the binding changed.
    pub fn capture_key(&mut self, options: &mut Options, key: KeyCode) -> bool {
        let Some(action) = self.capturing else {
            return false;
        };
        if options.keys.set(action, key) {
            self.capturing = None;
            true
        } else {
            false
        }
    }

    /// Draws one frame of the screen and edits `options`.
    ///
    /// `ui` must be between [`Ui::begin_frame`] and [`Ui::end_frame`].
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas, options: &mut Options) -> Outcome {
        self.rects.clear();
        let viewport = ui.viewport();
        ui.draw().rect(viewport, BACKDROP);

        let width = (viewport.width - 32.0).clamp(200.0, MAX_WIDTH);
        let height = (viewport.height - 32.0).max(120.0);
        let outer = Rect::new(
            viewport.x + (viewport.width - width) * 0.5,
            viewport.y + (viewport.height - height) * 0.5,
            width,
            height,
        );
        let content = panel(ui, outer, PANEL);
        let mut outcome = Outcome::default();

        ui.push_id("options");
        let mut layout = Layout::vertical(content).with_spacing(GAP);
        let title = layout.next_fixed(atlas.line_height(TITLE_SCALE));
        ui.draw().text(
            [title.x, title.y],
            "OPTIONS",
            TextStyle::new(TITLE_SCALE, TITLE),
        );

        let tabs = layout.next_fixed(ROW);
        self.show_tabs(ui, atlas, tabs);

        let done = Rect::new(
            content.x + (content.width - 200.0).max(0.0) * 0.5,
            content.bottom() - ROW,
            200.0_f32.min(content.width),
            ROW,
        );
        // `remaining` starts right under the tabs, without the row gap.
        let body = layout.remaining();
        let top = body.y + GAP;
        let body = Rect::new(body.x, top, body.width, (done.y - GAP - top).max(0.0));
        match self.tab {
            Tab::Video => self.show_video(ui, atlas, options, body, &mut outcome),
            Tab::Controls => self.show_controls(ui, atlas, options, body, &mut outcome),
            Tab::Chat => self.show_chat(ui, atlas, options, body, &mut outcome),
            other => {
                if let Some(note) = other.waiting_note() {
                    ui.draw()
                        .text([body.x, body.y], note, TextStyle::new(TEXT_SCALE, NOTE));
                }
            }
        }

        let done_id = ui.id("done");
        self.rects.insert("done".to_string(), done);
        if button(ui, done_id, done, atlas, "DONE").clicked {
            outcome.done = true;
        }
        ui.pop_id();
        outcome
    }

    fn show_tabs(&mut self, ui: &mut Ui, atlas: &FontAtlas, row: Rect) {
        let each = row.width / Tab::ALL.len() as f32;
        for (index, tab) in Tab::ALL.into_iter().enumerate() {
            let rect = Rect::new(row.x + index as f32 * each, row.y, each - GAP, row.height);
            self.rects.insert(tab.id().to_string(), rect);
            let label = if tab == self.tab {
                format!("[{}]", tab.label())
            } else {
                tab.label().to_string()
            };
            let id = ui.id(tab.id());
            if button(ui, id, rect, atlas, &label).clicked {
                self.tab = tab;
                self.capturing = None;
            }
        }
    }

    fn show_video(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        body: Rect,
        outcome: &mut Outcome,
    ) {
        let mut layout = Layout::vertical(body).with_spacing(GAP);

        let fraction = (options.video.fov - FOV.start()) / (FOV.end() - FOV.start());
        let rect = layout.next_fixed(ROW);
        self.rects.insert("fov".to_string(), rect);
        let id = ui.id("fov");
        let label = format!("FOV: {}", options.video.fov.round() as i32);
        let (value, changed) = slider(ui, id, rect, atlas, &label, fraction, &mut self.fov);
        if changed {
            options.video.fov = (FOV.start() + value * (FOV.end() - FOV.start())).round();
            outcome.changed = true;
        }

        let span = f32::from(RENDER_DISTANCE.end() - RENDER_DISTANCE.start());
        let fraction = f32::from(options.video.render_distance - RENDER_DISTANCE.start()) / span;
        let rect = layout.next_fixed(ROW);
        self.rects.insert("distance".to_string(), rect);
        let id = ui.id("distance");
        let label = format!("RENDER DISTANCE: {} CHUNKS", options.video.render_distance);
        let (value, changed) = slider(ui, id, rect, atlas, &label, fraction, &mut self.distance);
        if changed {
            options.video.render_distance = RENDER_DISTANCE.start() + (value * span).round() as u8;
            outcome.changed = true;
        }

        let span = (MAX_FPS.end() - MAX_FPS.start()) as f32;
        let fraction = (options.video.max_fps - MAX_FPS.start()) as f32 / span;
        let rect = layout.next_fixed(ROW);
        self.rects.insert("fps".to_string(), rect);
        let id = ui.id("fps");
        let label = if options.video.max_fps >= UNLIMITED_FPS {
            "MAX FPS: UNLIMITED".to_string()
        } else {
            format!("MAX FPS: {}", options.video.max_fps)
        };
        let (value, changed) = slider(ui, id, rect, atlas, &label, fraction, &mut self.fps);
        if changed {
            options.video.max_fps = MAX_FPS.start() + (value * span).round() as u32;
            outcome.changed = true;
        }

        let note = layout.next_fixed(atlas.line_height(TEXT_SCALE));
        ui.draw().text(
            [note.x, note.y],
            "THE SERVER LEARNS THE RENDER DISTANCE WHEN YOU CLOSE THIS SCREEN.",
            TextStyle::new(TEXT_SCALE, NOTE),
        );
    }

    fn show_controls(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        body: Rect,
        outcome: &mut Outcome,
    ) {
        let mut layout = Layout::vertical(body).with_spacing(GAP);

        let rect = layout.next_fixed(ROW);
        self.rects.insert("sens".to_string(), rect);
        let id = ui.id("sens");
        let label = format!(
            "SENSITIVITY: {}%",
            (options.controls.mouse_sensitivity * 200.0).round() as i32
        );
        let (value, changed) = slider(
            ui,
            id,
            rect,
            atlas,
            &label,
            options.controls.mouse_sensitivity,
            &mut self.sensitivity,
        );
        if changed {
            options.controls.mouse_sensitivity = value;
            outcome.changed = true;
        }

        let row = layout.next_fixed(ROW);
        let half = (row.width - GAP) * 0.5;
        let invert = Rect::new(row.x, row.y, half, row.height);
        self.rects.insert("invert".to_string(), invert);
        let id = ui.id("invert");
        let label = format!(
            "INVERT MOUSE: {}",
            if options.controls.invert_mouse {
                "ON"
            } else {
                "OFF"
            }
        );
        if button(ui, id, invert, atlas, &label).clicked {
            options.controls.invert_mouse = !options.controls.invert_mouse;
            outcome.changed = true;
        }
        let reset = Rect::new(row.x + half + GAP, row.y, half, row.height);
        self.rects.insert("reset".to_string(), reset);
        let id = ui.id("reset");
        if button(ui, id, reset, atlas, "RESET KEYS").clicked {
            options.keys.reset();
            self.capturing = None;
            outcome.changed = true;
        }

        // The key buttons fill two columns, top to bottom.
        let rows = Action::ALL.len().div_ceil(2);
        for line in 0..rows {
            let row = layout.next_fixed(ROW);
            for column in 0..2 {
                let Some(action) = Action::ALL.get(column * rows + line).copied() else {
                    continue;
                };
                let rect = Rect::new(
                    row.x + column as f32 * (half + GAP),
                    row.y,
                    half,
                    row.height,
                );
                self.show_key(ui, atlas, options, action, rect);
            }
        }
    }

    fn show_key(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &Options,
        action: Action,
        rect: Rect,
    ) {
        let key_id = format!("key:{}", action.id());
        self.rects.insert(key_id.clone(), rect);
        let waiting = self.capturing == Some(action);
        let conflicted = !options.keys.conflicts(action).is_empty();
        let key = if waiting {
            "PRESS A KEY".to_string()
        } else {
            key_label(options.keys.get(action))
        };
        let label = format!("{}: {key}", action.label());
        let id = ui.id(&key_id);
        if button(ui, id, rect, atlas, &label).clicked {
            self.capturing = Some(action);
        }
        if conflicted && !waiting {
            // A red bar on the left edge flags two actions on one key.
            ui.draw()
                .rect(Rect::new(rect.x, rect.y, 4.0, rect.height), CONFLICT);
        }
    }

    fn show_chat(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        body: Rect,
        outcome: &mut Outcome,
    ) {
        let mut layout = Layout::vertical(body).with_spacing(GAP);

        let rect = layout.next_fixed(ROW);
        self.rects.insert("visibility".to_string(), rect);
        let id = ui.id("visibility");
        let label = format!(
            "CHAT: {}",
            match options.chat.visibility {
                crate::options::ChatVisibility::Full => "SHOWN",
                crate::options::ChatVisibility::Commands => "COMMANDS ONLY",
                crate::options::ChatVisibility::Hidden => "HIDDEN",
            }
        );
        if button(ui, id, rect, atlas, &label).clicked {
            options.chat.visibility = options.chat.visibility.next();
            outcome.changed = true;
        }

        let rect = layout.next_fixed(ROW);
        self.rects.insert("colors".to_string(), rect);
        let id = ui.id("colors");
        let label = format!("COLORS: {}", if options.chat.colors { "ON" } else { "OFF" });
        if button(ui, id, rect, atlas, &label).clicked {
            options.chat.colors = !options.chat.colors;
            outcome.changed = true;
        }

        let note = layout.next_fixed(atlas.line_height(TEXT_SCALE));
        ui.draw().text(
            [note.x, note.y],
            "THE SERVER LEARNS THESE WHEN YOU CLOSE THIS SCREEN.",
            TextStyle::new(TEXT_SCALE, NOTE),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_ui::{InputEvent, PointerButton, UiInput};

    const VIEWPORT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 720.0,
    };

    fn frame(
        screen: &mut OptionsScreen,
        options: &mut Options,
        events: Vec<InputEvent>,
    ) -> Outcome {
        let mut ui = Ui::new(VIEWPORT);
        ui.begin_frame(VIEWPORT, UiInput::new(events));
        let outcome = screen.show(&mut ui, &FontAtlas::fallback(), options);
        ui.end_frame();
        outcome
    }

    fn click(rect: Rect) -> Vec<InputEvent> {
        let position = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        vec![
            InputEvent::PointerMoved { position },
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            },
            InputEvent::PointerReleased {
                position,
                button: PointerButton::Left,
            },
        ]
    }

    /// Draws a frame to lay the widgets out, then clicks `id` in the next.
    fn click_widget(screen: &mut OptionsScreen, options: &mut Options, id: &str) -> Outcome {
        frame(screen, options, vec![]);
        let rect = screen
            .rect_of(id)
            .unwrap_or_else(|| panic!("no widget {id}"));
        frame(screen, options, click(rect))
    }

    #[test]
    fn the_screen_opens_on_the_video_tab_with_a_done_button() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        let outcome = frame(&mut screen, &mut options, vec![]);
        assert_eq!(outcome, Outcome::default());
        assert_eq!(screen.tab(), Tab::Video);
        assert!(screen.rect_of("fov").is_some());
        assert!(screen.rect_of("done").is_some());
    }

    #[test]
    fn done_is_reported() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        let outcome = click_widget(&mut screen, &mut options, "done");
        assert!(outcome.done);
    }

    #[test]
    fn a_tab_click_switches_the_content() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "tab:controls");
        assert_eq!(screen.tab(), Tab::Controls);
        frame(&mut screen, &mut options, vec![]);
        assert!(screen.rect_of("key:forward").is_some());
        assert!(screen.rect_of("fov").is_none());
        click_widget(&mut screen, &mut options, "tab:chat");
        assert_eq!(screen.tab(), Tab::Chat);
    }

    #[test]
    fn the_video_sliders_edit_the_options_within_their_ranges() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        frame(&mut screen, &mut options, vec![]);
        let fov = screen.rect_of("fov").unwrap();
        // The far right end of the bar is the maximum.
        let events = vec![InputEvent::PointerPressed {
            position: [fov.right() - 1.0, fov.y + 2.0],
            button: PointerButton::Left,
        }];
        let outcome = frame(&mut screen, &mut options, events);
        assert!(outcome.changed);
        assert_eq!(options.video.fov, 110.0);

        let distance = screen.rect_of("distance").unwrap();
        let events = vec![InputEvent::PointerPressed {
            position: [distance.x + 1.0, distance.y + 2.0],
            button: PointerButton::Left,
        }];
        frame(&mut screen, &mut options, events);
        assert_eq!(options.video.render_distance, 2);
    }

    #[test]
    fn the_frame_rate_slider_reaches_unlimited() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        frame(&mut screen, &mut options, vec![]);
        let fps = screen.rect_of("fps").unwrap();
        let events = vec![InputEvent::PointerPressed {
            position: [fps.right() - 1.0, fps.y + 2.0],
            button: PointerButton::Left,
        }];
        frame(&mut screen, &mut options, events);
        assert_eq!(options.video.max_fps, UNLIMITED_FPS);
    }

    #[test]
    fn invert_mouse_and_the_sensitivity_slider_edit_the_controls() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "tab:controls");
        let outcome = click_widget(&mut screen, &mut options, "invert");
        assert!(outcome.changed);
        assert!(options.controls.invert_mouse);

        let sens = screen.rect_of("sens").unwrap();
        let events = vec![InputEvent::PointerPressed {
            position: [sens.x + 1.0, sens.y + 2.0],
            button: PointerButton::Left,
        }];
        frame(&mut screen, &mut options, events);
        assert_eq!(options.controls.mouse_sensitivity, 0.0);
    }

    #[test]
    fn rebinding_a_key_is_a_click_then_a_key_press() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "tab:controls");
        click_widget(&mut screen, &mut options, "key:forward");
        assert_eq!(screen.capturing(), Some(Action::Forward));

        assert!(screen.capture_key(&mut options, KeyCode::ArrowUp));
        assert_eq!(options.keys.get(Action::Forward), KeyCode::ArrowUp);
        assert_eq!(screen.capturing(), None);
        // Nothing waits any more, so a later key changes nothing.
        assert!(!screen.capture_key(&mut options, KeyCode::KeyZ));
    }

    #[test]
    fn a_reserved_key_is_refused_and_the_capture_goes_on() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "tab:controls");
        click_widget(&mut screen, &mut options, "key:jump");
        assert!(!screen.capture_key(&mut options, KeyCode::F3));
        assert_eq!(screen.capturing(), Some(Action::Jump));
        assert_eq!(options.keys.get(Action::Jump), KeyCode::Space);
        screen.cancel_capture();
        assert_eq!(screen.capturing(), None);
    }

    #[test]
    fn reset_keys_restores_the_defaults() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        options.keys.set(Action::Forward, KeyCode::ArrowUp);
        click_widget(&mut screen, &mut options, "tab:controls");
        let outcome = click_widget(&mut screen, &mut options, "reset");
        assert!(outcome.changed);
        assert_eq!(options.keys, crate::options::KeyBindings::default());
    }

    #[test]
    fn the_chat_buttons_cycle_the_visibility_and_toggle_the_colours() {
        use crate::options::ChatVisibility;
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "tab:chat");
        click_widget(&mut screen, &mut options, "visibility");
        assert_eq!(options.chat.visibility, ChatVisibility::Commands);
        click_widget(&mut screen, &mut options, "visibility");
        assert_eq!(options.chat.visibility, ChatVisibility::Hidden);
        click_widget(&mut screen, &mut options, "colors");
        assert!(!options.chat.colors);
    }

    #[test]
    fn the_tabs_that_wait_for_a_milestone_say_so_and_change_nothing() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        for tab in ["tab:audio", "tab:mods", "tab:accounts"] {
            click_widget(&mut screen, &mut options, tab);
            assert_eq!(frame(&mut screen, &mut options, vec![]), Outcome::default());
        }
        assert_eq!(options, Options::default());
    }

    #[test]
    fn changing_tab_gives_up_a_pending_capture() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "tab:controls");
        click_widget(&mut screen, &mut options, "key:chat");
        assert!(screen.capturing().is_some());
        click_widget(&mut screen, &mut options, "tab:video");
        assert_eq!(screen.capturing(), None);
    }

    #[test]
    fn the_panel_fits_a_small_window() {
        let small = Rect::new(0.0, 0.0, 320.0, 240.0);
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        let mut ui = Ui::new(small);
        ui.begin_frame(small, UiInput::default());
        screen.show(&mut ui, &FontAtlas::fallback(), &mut options);
        ui.end_frame();
        let done = screen.rect_of("done").unwrap();
        assert!(done.x >= 0.0 && done.right() <= small.width);
        assert!(done.y >= 0.0 && done.bottom() <= small.height);
    }
}
