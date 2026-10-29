// SPDX-License-Identifier: MIT OR Apache-2.0

//! The options screens, laid out like Minecraft 1.8.8 (M10, UI-3).
//!
//! The pages replicate `GuiOptions`, `GuiVideoSettings`, `GuiControls` and
//! `ScreenChatOptions`: every coordinate below is in GUI units (the
//! `ScaledResolution` of the window) and multiplied by the GUI scale when
//! drawn. A vanilla option the client has no backend for yet is drawn in its
//! place, enabled but inert, with the vanilla default value.
//!
//! [`OptionsScreen`] draws through a [`Ui`] and a [`FontAtlas`], reads no
//! file, clock or network, and edits the [`Options`] it is given. The window
//! saves the options and tells the server about the changes when the screen
//! closes. A key rebinding is a two-step flow: a click on the key button
//! starts the capture, and the next key press is handed to
//! [`OptionsScreen::capture_key`] by the window.

use std::collections::HashMap;

use umicraft_ui::game_menu::draw_default_background;
use umicraft_ui::widgets::{button_tinted, gui_scale, slider, SliderState};
use umicraft_ui::{Color, FontAtlas, InputEvent, Rect, TextStyle, Ui};
use winit::keyboard::KeyCode;

use crate::options::{
    key_label, Action, ChatVisibility, Options, FOV, MAX_FPS, RENDER_DISTANCE, UNLIMITED_FPS,
};

/// Height of a vanilla button, in GUI units.
const BUTTON_H: f32 = 20.0;
/// Width of a half-width button, in GUI units.
const HALF_W: f32 = 150.0;
/// Height of one row of the video list, in GUI units.
const VIDEO_ROW: f32 = 25.0;
/// Height of one row of the key list, in GUI units.
const KEY_ROW: f32 = 20.0;
/// `EnumChatFormatting.RED`.
const RED: Color = [1.0, 0.333, 0.333, 1.0];
/// `EnumChatFormatting.YELLOW`.
const YELLOW: Color = [1.0, 1.0, 0.333, 1.0];
/// The shadow of white text (`0xFFFFFF >> 2 & 0xFCFCFC`).
const SHADOW: Color = [0.247, 0.247, 0.247, 1.0];
/// The tint of the dirt background behind a list (`0x202020`).
const LIST_BACKGROUND: Color = [0.125, 0.125, 0.125, 1.0];
/// The tint of the bars above and below a list (`0x404040`).
const LIST_BAR: Color = [0.25, 0.25, 0.25, 1.0];

/// One page of the options screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// `GuiOptions`: the hub.
    Main,
    /// `GuiVideoSettings`.
    Video,
    /// `GuiControls`.
    Controls,
    /// `ScreenChatOptions`.
    Chat,
}

/// What one frame of the screen did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// An option changed in this frame.
    pub changed: bool,
    /// The player pressed Done on the hub page.
    pub done: bool,
}

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

/// The geometry of a frame: GUI units and their scale.
#[derive(Debug, Clone, Copy)]
struct Gui {
    scale: f32,
    width: f32,
    height: f32,
}

impl Gui {
    fn new(viewport: Rect) -> Self {
        let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
        Self {
            scale,
            width: (viewport.width / scale).ceil(),
            height: (viewport.height / scale).ceil(),
        }
    }

    /// A rectangle given in GUI units, in pixels.
    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            x * self.scale,
            y * self.scale,
            w * self.scale,
            h * self.scale,
        )
    }

    /// Half the width, as vanilla's integer division gives it.
    fn cx(self) -> f32 {
        (self.width / 2.0).floor()
    }
}

/// The state of the options screens.
#[derive(Debug, Clone)]
pub struct OptionsScreen {
    page: Page,
    capturing: Option<Action>,
    /// Scroll offset of the list of the page, in GUI units.
    scroll: f32,
    sliders: HashMap<String, SliderState>,
    /// The rectangle of every widget in the last frame, by id.
    rects: HashMap<String, Rect>,
}

impl Default for OptionsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionsScreen {
    /// Creates the screen on the hub page.
    #[must_use]
    pub fn new() -> Self {
        Self {
            page: Page::Main,
            capturing: None,
            scroll: 0.0,
            sliders: HashMap::new(),
            rects: HashMap::new(),
        }
    }

    /// The page that is shown.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
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

    /// Goes back to the hub page, as when the screen is opened again.
    pub fn reset(&mut self) {
        self.page = Page::Main;
        self.capturing = None;
        self.scroll = 0.0;
    }

    /// The rectangle of a widget in the last frame (`"done"`, `"nav:video"`,
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
        let gui = Gui::new(ui.viewport());
        let mut outcome = Outcome::default();
        draw_default_background(ui);
        ui.push_id("options");
        match self.page {
            Page::Main => self.show_main(ui, atlas, options, gui, &mut outcome),
            Page::Video => self.show_video(ui, atlas, options, gui, &mut outcome),
            Page::Controls => self.show_controls(ui, atlas, options, gui, &mut outcome),
            Page::Chat => self.show_chat(ui, atlas, options, gui, &mut outcome),
        }
        ui.pop_id();
        outcome
    }

    /// Draws a button and reports whether it was clicked.
    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        label: &str,
        enabled: bool,
        tint: Option<Color>,
    ) -> bool {
        self.rects.insert(id.to_string(), rect);
        let widget = ui.id(id);
        button_tinted(ui, widget, rect, atlas, label, enabled, tint).clicked
    }

    /// Draws a slider and returns its new fraction when it moved.
    fn slider(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        label: &str,
        fraction: f32,
    ) -> Option<f32> {
        self.rects.insert(id.to_string(), rect);
        let widget = ui.id(id);
        let mut state = self.sliders.remove(id).unwrap_or_default();
        let (value, changed) = slider(ui, widget, rect, atlas, label, fraction, &mut state);
        self.sliders.insert(id.to_string(), state);
        changed.then_some(value)
    }

    /// `drawCenteredString` in white with the vanilla shadow.
    fn title(ui: &mut Ui, atlas: &FontAtlas, gui: Gui, text: &str, y: f32) {
        let width = atlas.measure(text, gui.scale);
        let x = (gui.cx() * gui.scale - width * 0.5).round();
        let y = (y * gui.scale).round();
        ui.draw().text(
            [x + gui.scale, y + gui.scale],
            text,
            TextStyle::new(gui.scale, SHADOW),
        );
        ui.draw()
            .text([x, y], text, TextStyle::new(gui.scale, [1.0; 4]));
    }

    /// `GuiOptions`.
    fn show_main(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, "Options", 15.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        let left = cx - 155.0;
        let right = cx + 5.0;

        let fov = options.video.fov;
        let fraction = (fov - FOV.start()) / (FOV.end() - FOV.start());
        let rect = gui.rect(left, h6 - 12.0, HALF_W, BUTTON_H);
        if let Some(value) = self.slider(ui, atlas, "fov", rect, &fov_label(fov), fraction) {
            options.video.fov = (FOV.start() + value * (FOV.end() - FOV.start())).round();
            outcome.changed = true;
        }
        // The difficulty belongs to the server (`isSingleplayer()` is false).
        let difficulty = gui.rect(right, h6 - 12.0, HALF_W, BUTTON_H);
        self.button(
            ui,
            atlas,
            "difficulty",
            difficulty,
            "Difficulty: Normal",
            false,
            None,
        );

        // (id, label, y offset, x, enabled): the disabled ones wait for
        // a backend of their own.
        let entries: [(&str, &str, f32, f32, bool); 10] = [
            ("skin", "Skin Customization...", 48.0, left, true),
            ("shaders", "Super Secret Settings...", 48.0, right, true),
            ("sounds", "Music & Sounds...", 72.0, left, true),
            ("stream", "Broadcast Settings...", 72.0, right, true),
            ("nav:video", "Video Settings...", 96.0, left, true),
            ("nav:controls", "Controls...", 96.0, right, true),
            ("language", "Language...", 120.0, left, true),
            ("nav:chat", "Chat Settings...", 120.0, right, true),
            ("resourcepacks", "Resource Packs...", 144.0, left, true),
            ("snooper", "Snooper Settings...", 144.0, right, true),
        ];
        for (id, label, row, x, enabled) in entries {
            let rect = gui.rect(x, h6 + row - 6.0, HALF_W, BUTTON_H);
            if self.button(ui, atlas, id, rect, label, enabled, None) {
                let page = match id {
                    "nav:video" => Page::Video,
                    "nav:controls" => Page::Controls,
                    "nav:chat" => Page::Chat,
                    // Vanilla opens a screen the client does not have yet.
                    _ => continue,
                };
                self.page = page;
                self.scroll = 0.0;
            }
        }

        let done = gui.rect(cx - 100.0, h6 + 168.0, 200.0, BUTTON_H);
        if self.button(ui, atlas, "done", done, "Done", true, None) {
            outcome.done = true;
        }
    }

    /// The bottom "Done" of the sub-pages: back to the hub.
    fn back(&mut self, ui: &mut Ui, atlas: &FontAtlas, rect: Rect) {
        if self.button(ui, atlas, "done", rect, "Done", true, None) {
            self.page = Page::Main;
            self.capturing = None;
            self.scroll = 0.0;
        }
    }

    /// Draws the dirt-less list background: the middle tint, the bars above
    /// and below and the 4-unit shadows of `GuiSlot.overlayBackground`.
    fn list_background(ui: &mut Ui, gui: Gui, top: f32, bottom: f32) {
        let width = gui.width * gui.scale;
        ui.draw()
            .rect(gui.rect(0.0, top, gui.width, bottom - top), LIST_BACKGROUND);
        ui.draw()
            .rect(Rect::new(0.0, 0.0, width, top * gui.scale), LIST_BAR);
        ui.draw().rect(
            Rect::new(
                0.0,
                bottom * gui.scale,
                width,
                (gui.height - bottom) * gui.scale,
            ),
            LIST_BAR,
        );
        for step in 0..4 {
            let alpha = 1.0 - step as f32 / 4.0;
            let color = [0.0, 0.0, 0.0, alpha];
            ui.draw()
                .rect(gui.rect(0.0, top + step as f32, gui.width, 1.0), color);
            ui.draw().rect(
                gui.rect(0.0, bottom - 1.0 - step as f32, gui.width, 1.0),
                color,
            );
        }
    }

    /// Applies the wheel to the scroll offset.
    fn scroll_list(&mut self, ui: &Ui, content: f32, visible: f32, row: f32) {
        let max = (content - (visible - 4.0)).max(0.0);
        for event in &ui.input().events {
            if let InputEvent::Wheel { delta } = event {
                self.scroll += delta * row / 2.0;
            }
        }
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// The vanilla scrollbar of a `GuiSlot`.
    fn scrollbar(
        ui: &mut Ui,
        gui: Gui,
        x: f32,
        (top, bottom): (f32, f32),
        content: f32,
        scroll: f32,
    ) {
        let visible = bottom - top;
        let max = (content - (visible - 4.0)).max(0.0);
        if max <= 0.0 {
            return;
        }
        let thumb = (visible * visible / content).clamp(32.0, visible - 8.0);
        let y = (scroll * (visible - thumb) / max + top).max(top);
        ui.draw()
            .rect(gui.rect(x, top, 6.0, visible), [0.0, 0.0, 0.0, 1.0]);
        ui.draw()
            .rect(gui.rect(x, y, 6.0, thumb), [0.5, 0.5, 0.5, 1.0]);
        ui.draw()
            .rect(gui.rect(x, y, 5.0, thumb - 1.0), [0.75, 0.75, 0.75, 1.0]);
    }

    /// `GuiVideoSettings`.
    fn show_video(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (32.0, gui.height - 32.0);
        let cx = gui.cx();
        let rows = 8;
        let content = rows as f32 * VIDEO_ROW;
        self.scroll_list(ui, content, bottom - top, VIDEO_ROW);
        Self::list_background(ui, gui, top, bottom);

        // The list is clipped to its area; a row is only clickable while it
        // lies inside.
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for index in 0..rows * 2 {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = top + 4.0 - self.scroll + (index / 2) as f32 * VIDEO_ROW;
            let rect = gui.rect(x, y, HALF_W, BUTTON_H);
            let inside = y >= top && y + BUTTON_H <= bottom;
            self.video_option(ui, atlas, options, index, rect, inside, outcome);
        }
        ui.pop_clip();
        Self::scrollbar(ui, gui, cx + 156.0, (top, bottom), content, self.scroll);

        Self::title(ui, atlas, gui, "Video Settings", 5.0);
        let done = gui.rect(cx - 100.0, gui.height - 27.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done);
    }

    /// One entry of `GuiVideoSettings.videoOptions`.
    #[allow(clippy::too_many_arguments)]
    fn video_option(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        index: usize,
        rect: Rect,
        inside: bool,
        outcome: &mut Outcome,
    ) {
        match index {
            1 => {
                let span = f32::from(RENDER_DISTANCE.end() - RENDER_DISTANCE.start());
                let distance = options.video.render_distance;
                let fraction = f32::from(distance - RENDER_DISTANCE.start()) / span;
                let label = format!("Render Distance: {distance} chunks");
                if inside {
                    if let Some(value) = self.slider(ui, atlas, "distance", rect, &label, fraction)
                    {
                        options.video.render_distance =
                            RENDER_DISTANCE.start() + (value * span).round() as u8;
                        outcome.changed = true;
                    }
                } else {
                    self.rects.insert("distance".to_string(), rect);
                }
            }
            3 => {
                let span = (MAX_FPS.end() - MAX_FPS.start()) as f32;
                let fps = options.video.max_fps;
                let fraction = (fps - MAX_FPS.start()) as f32 / span;
                let label = if fps >= UNLIMITED_FPS {
                    "Max Framerate: Unlimited".to_string()
                } else {
                    format!("Max Framerate: {fps} fps")
                };
                if inside {
                    if let Some(value) = self.slider(ui, atlas, "fps", rect, &label, fraction) {
                        // `GameSettings.snapToStep`: steps of 10.
                        let steps = (value * span / 10.0).round() as u32;
                        options.video.max_fps = MAX_FPS.start() + steps * 10;
                        outcome.changed = true;
                    }
                } else {
                    self.rects.insert("fps".to_string(), rect);
                }
            }
            _ => {
                let id = format!("video:{index}");
                self.button(ui, atlas, &id, rect, VIDEO_DISABLED[index], true, None);
            }
        }
    }

    /// `GuiControls`.
    fn show_controls(
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

    /// `ScreenChatOptions`.
    fn show_chat(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        outcome: &mut Outcome,
    ) {
        Self::title(ui, atlas, gui, "Chat Settings", 20.0);
        let cx = gui.cx();
        let h6 = (gui.height / 6.0).floor();
        for (index, entry) in CHAT_ENTRIES.iter().enumerate() {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = h6 + 24.0 * (index / 2) as f32;
            let rect = gui.rect(x, y, HALF_W, BUTTON_H);
            match *entry {
                "Chat" => {
                    let label = format!(
                        "Chat: {}",
                        match options.chat.visibility {
                            ChatVisibility::Full => "Shown",
                            ChatVisibility::Commands => "Commands Only",
                            ChatVisibility::Hidden => "Hidden",
                        }
                    );
                    if self.button(ui, atlas, "visibility", rect, &label, true, None) {
                        options.chat.visibility = options.chat.visibility.next();
                        outcome.changed = true;
                    }
                }
                "Colors" => {
                    let label =
                        format!("Colors: {}", if options.chat.colors { "ON" } else { "OFF" });
                    if self.button(ui, atlas, "colors", rect, &label, true, None) {
                        options.chat.colors = !options.chat.colors;
                        outcome.changed = true;
                    }
                }
                label => {
                    let id = format!("chat:{index}");
                    self.button(ui, atlas, &id, rect, label, true, None);
                }
            }
        }
        let done = gui.rect(cx - 100.0, h6 + 120.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done);
    }
}

/// `GameSettings.getKeyBinding(FOV)`.
fn fov_label(fov: f32) -> String {
    if fov == 70.0 {
        "FOV: Normal".to_string()
    } else if fov == 110.0 {
        "FOV: Quake Pro".to_string()
    } else {
        format!("FOV: {}", fov as i32)
    }
}

/// `GameSettings.getKeyBinding(SENSITIVITY)`.
fn sensitivity_label(value: f32) -> String {
    if value == 0.0 {
        "Sensitivity: *yawn*".to_string()
    } else if value == 1.0 {
        "Sensitivity: HYPERSPEED!!!".to_string()
    } else {
        format!("Sensitivity: {}%", (value * 200.0) as i32)
    }
}

/// `GuiVideoSettings.videoOptions` without a backend: the label with the
/// vanilla default, in the vanilla order (1 and 3 are live sliders).
const VIDEO_DISABLED: [&str; 16] = [
    "Graphics: Fancy",
    "Render Distance",
    "Smooth Lighting: Maximum",
    "Max Framerate",
    "3D Anaglyph: OFF",
    "View Bobbing: ON",
    "GUI Scale: Auto",
    "Brightness: Moody",
    "Clouds: Fancy",
    "Particles: All",
    "Fullscreen: OFF",
    "Use VSync: ON",
    "Mipmap Levels: 4",
    "Allow Block Alternatives: ON",
    "Use VBOs: OFF",
    "Entity Shadows: ON",
];

/// `ScreenChatOptions.field_146399_a`; "Chat" and "Colors" are live.
const CHAT_ENTRIES: [&str; 10] = [
    "Chat",
    "Colors",
    "Web Links: ON",
    "Opacity: 100%",
    "Prompt on Links: OFF",
    "Scale: 100%",
    "Focused Height: 180px",
    "Unfocused Height: 20px",
    "Width: 320px",
    "Reduced Debug Info: OFF",
];

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_ui::{PointerButton, UiInput};

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

    /// GUI scale 3 at 1280x720: the window is 427x240 GUI units.
    fn gui_rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x * 3.0, y * 3.0, w * 3.0, h * 3.0)
    }

    #[test]
    fn the_hub_page_matches_gui_options() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        frame(&mut screen, &mut options, vec![]);
        // width 427 -> cx 213; height 240 -> h/6 = 40.
        assert_eq!(
            screen.rect_of("fov"),
            Some(gui_rect(58.0, 28.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("difficulty"),
            Some(gui_rect(218.0, 28.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("nav:video"),
            Some(gui_rect(58.0, 130.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("nav:controls"),
            Some(gui_rect(218.0, 130.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("nav:chat"),
            Some(gui_rect(218.0, 154.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("done"),
            Some(gui_rect(113.0, 208.0, 200.0, 20.0))
        );
    }

    #[test]
    fn done_on_the_hub_closes_and_done_on_a_page_goes_back() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "nav:chat");
        assert_eq!(screen.page(), Page::Chat);
        let outcome = click_widget(&mut screen, &mut options, "done");
        assert!(!outcome.done);
        assert_eq!(screen.page(), Page::Main);
        assert!(click_widget(&mut screen, &mut options, "done").done);
    }

    #[test]
    fn a_disabled_hub_button_does_nothing() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "sounds");
        assert_eq!(screen.page(), Page::Main);
    }

    #[test]
    fn the_fov_slider_edits_within_its_range_and_names_the_ends() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        frame(&mut screen, &mut options, vec![]);
        let fov = screen.rect_of("fov").unwrap();
        let events = vec![InputEvent::PointerPressed {
            position: [fov.x + fov.width, fov.y + 5.0],
            button: PointerButton::Left,
        }];
        let outcome = frame(&mut screen, &mut options, events);
        assert!(outcome.changed);
        assert_eq!(options.video.fov, 110.0);
        assert_eq!(fov_label(110.0), "FOV: Quake Pro");
        assert_eq!(fov_label(70.0), "FOV: Normal");
        assert_eq!(fov_label(90.0), "FOV: 90");
    }

    #[test]
    fn the_video_page_lists_the_sixteen_options_in_vanilla_order() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "nav:video");
        frame(&mut screen, &mut options, vec![]);
        // Row 0 starts at top + 4 = 36; Render Distance is the right column.
        assert_eq!(
            screen.rect_of("video:0"),
            Some(gui_rect(58.0, 36.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("distance"),
            Some(gui_rect(218.0, 36.0, 150.0, 20.0))
        );
        assert_eq!(
            screen.rect_of("done"),
            Some(gui_rect(113.0, 213.0, 200.0, 20.0))
        );
    }

    #[test]
    fn the_wheel_scrolls_the_video_list() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "nav:video");
        frame(&mut screen, &mut options, vec![]);
        let before = screen.rect_of("video:0").unwrap();
        frame(
            &mut screen,
            &mut options,
            vec![InputEvent::Wheel { delta: 1.0 }],
        );
        let after = screen.rect_of("video:0").unwrap();
        assert!(after.y < before.y, "the row moved up");
    }

    #[test]
    fn the_controls_page_rebinds_a_key_and_resets_it() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "nav:controls");
        assert_eq!(screen.page(), Page::Controls);
        // Scroll to the Movement keys, then click Jump.
        frame(
            &mut screen,
            &mut options,
            vec![InputEvent::Wheel { delta: 16.0 }],
        );
        click_widget(&mut screen, &mut options, "key:jump");
        assert_eq!(screen.capturing(), Some(Action::Jump));
        assert!(screen.capture_key(&mut options, KeyCode::KeyJ));
        assert_eq!(options.keys.get(Action::Jump), KeyCode::KeyJ);
        click_widget(&mut screen, &mut options, "keyreset:jump");
        assert_eq!(options.keys.get(Action::Jump), KeyCode::Space);
    }

    #[test]
    fn reset_keys_is_disabled_until_a_key_changes() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "nav:controls");
        let outcome = click_widget(&mut screen, &mut options, "reset");
        assert!(!outcome.changed);
        options.keys.set(Action::Chat, KeyCode::KeyC);
        let outcome = click_widget(&mut screen, &mut options, "reset");
        assert!(outcome.changed);
        assert_eq!(options.keys.get(Action::Chat), KeyCode::KeyT);
    }

    #[test]
    fn the_chat_page_cycles_visibility_and_toggles_colors() {
        let mut screen = OptionsScreen::new();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, "nav:chat");
        let before = options.chat.visibility;
        let outcome = click_widget(&mut screen, &mut options, "visibility");
        assert!(outcome.changed);
        assert_ne!(options.chat.visibility, before);
        let colors = options.chat.colors;
        click_widget(&mut screen, &mut options, "colors");
        assert_eq!(options.chat.colors, !colors);
        // Vanilla places the right column 160 units after the left.
        let gap = screen.rect_of("colors").unwrap().x - screen.rect_of("visibility").unwrap().x;
        assert_eq!(gap, 160.0 * 3.0);
    }

    #[test]
    fn labels_follow_game_settings() {
        assert_eq!(sensitivity_label(0.0), "Sensitivity: *yawn*");
        assert_eq!(sensitivity_label(1.0), "Sensitivity: HYPERSPEED!!!");
        assert_eq!(sensitivity_label(0.5), "Sensitivity: 100%");
    }
}
