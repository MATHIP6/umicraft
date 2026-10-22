// SPDX-License-Identifier: MIT OR Apache-2.0

//! The M4 acceptance screen: a title, a button, a text field and a list.
//!
//! [`DebugScreen`] is a **pure** screen. It owns only its editing and selection
//! state and draws through a [`Ui`] and a [`FontAtlas`]; it reads no file, no
//! clock, no network and no global input. The same `show` call is used by the
//! windowed client (F2), by the headless `--ui-demo` script and by the
//! offscreen screenshot path, so the acceptance evidence exercises exactly the
//! code the player sees.
//!
//! The screen lays itself out from [`Ui::viewport`]: a dimmed backdrop, a
//! panel, a title, a full-width text field, a button and a list that fills the
//! rest of the panel. The rectangles of the three interactive widgets are
//! exposed after each frame so a driver can synthesise clicks without
//! duplicating the layout.
//!
//! The list content is original text about the M4 features; it contains no
//! Mojang asset or string.

use crate::context::Ui;
use crate::draw::TextStyle;
use crate::geom::{Layout, Rect};
use crate::text::FontAtlas;
use crate::widgets::{button, list, panel, text_field, ListState, TextFieldState};

use crate::widgets::style;

/// Distance between the viewport edge and the panel, in pixels.
const MARGIN: f32 = 24.0;
/// Scale of the screen title.
const TITLE_SCALE: f32 = 3.0;
/// Height of the text field row, in pixels.
const FIELD_HEIGHT: f32 = 32.0;
/// Height of the button row, in pixels.
const BUTTON_HEIGHT: f32 = 32.0;
/// Preferred button width; clamped to the panel content width.
const BUTTON_WIDTH: f32 = 200.0;
/// Gap between two stacked rows, in pixels.
const SPACING: f32 = 8.0;
/// Title of the screen.
const TITLE: &str = "UMICRAFT UI - M4 ACCEPTANCE";
/// Label of the demo button.
const BUTTON_LABEL: &str = "CLICK ME";

/// Original list content: one feature per row, enough rows to scroll in a
/// 720-pixel viewport.
const ITEMS: [&str; 32] = [
    "FOCUS: CLICK A WIDGET OR PRESS TAB",
    "TEXT FIELD: TYPE AND EDIT",
    "BACKSPACE AND DELETE WORK",
    "HOME AND END JUMP TO THE EDGES",
    "ARROW KEYS MOVE THE CARET",
    "LONG TEXT SCROLLS HORIZONTALLY",
    "THE CARET IS DRAWN WHEN FOCUSED",
    "BUTTON: HOVER, PRESS, RELEASE",
    "LIST: WHEEL TO SCROLL",
    "LIST: UP AND DOWN MOVE SELECTION",
    "ROWS ARE CLIPPED TO THE VIEWPORT",
    "THE SCROLLBAR SHOWS THE POSITION",
    "PANELS RETURN THEIR CONTENT RECT",
    "DRAW LISTS HAVE BALANCED CLIPS",
    "THE SAME SCREEN RUNS HEADLESS",
    "NO FILES, TIME OR NETWORK HERE",
    "FALLBACK FONT BEFORE ASSETS",
    "REAL 1.8 FONT WHEN INSTALLED",
    "M4: INTERFACE FOUNDATION",
    "UMICRAFT IS NOT AFFILIATED WITH MOJANG",
    "TAB MOVES FOCUS BETWEEN WIDGETS",
    "F2 TOGGLES THIS SCREEN IN THE WINDOW",
    "THE UI READS THE RENDER SNAPSHOT",
    "THE GAME THREAD KEEPS RUNNING",
    "THE FALLBACK FONT NEEDS NO ASSET",
    "THE SCROLLBAR IS A PASSIVE AFFORDANCE",
    "CLICKS FIRE ON THE PRIMARY PRESS",
    "THE FIRST OVERLAPPING WIDGET WINS",
    "TEXT METRICS COME FROM THE ATLAS",
    "THE SAME INPUT RUNS HEADLESS",
    "THE SCREENSHOT PATH REUSES THIS SCREEN",
    "NO GAME ASSET IS STORED IN THE REPOSITORY",
];

/// The state and the last layout of the M4 acceptance screen.
///
/// Create one screen, call [`Self::show`] once per UI frame, and read the
/// accessors afterwards. The state is independent of the renderer, so a
/// headless driver and a windowed client produce the same values for the same
/// input events.
#[derive(Debug, Clone)]
pub struct DebugScreen {
    field: TextFieldState,
    list: ListState,
    items: Vec<String>,
    clicks: u32,
    field_rect: Rect,
    button_rect: Rect,
    list_rect: Rect,
}

impl DebugScreen {
    /// Creates the screen with an empty field and an unselected list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            field: TextFieldState::new(),
            list: ListState::new(),
            items: ITEMS.iter().map(|item| (*item).to_owned()).collect(),
            clicks: 0,
            field_rect: Rect::ZERO,
            button_rect: Rect::ZERO,
            list_rect: Rect::ZERO,
        }
    }

    /// Draws one frame of the screen and updates its state.
    ///
    /// `ui` must be between [`Ui::begin_frame`] and [`Ui::end_frame`]. The
    /// function calls each widget exactly once; the focus, the clicks, the
    /// editing and the scrolling all come from the events of the current
    /// frame. It returns no value: the driver reads the accessors below after
    /// the call, which keeps the screen usable from code that cannot hold a
    /// borrow of the widgets.
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas) {
        let viewport = ui.viewport();
        ui.draw().rect(viewport, style::SCREEN_BACKDROP);

        let content = panel(ui, viewport.inset(MARGIN), style::PANEL);
        let title_height = atlas.line_height(TITLE_SCALE);
        let mut layout = Layout::vertical(content).with_spacing(SPACING);
        let title_rect = layout.next_fixed(title_height);
        let field_rect = layout.next_fixed(FIELD_HEIGHT);
        let button_rect = layout.next(BUTTON_WIDTH.min(content.width), BUTTON_HEIGHT);
        let list_rect = layout.next_remaining();

        self.field_rect = field_rect;
        self.button_rect = button_rect;
        self.list_rect = list_rect;

        // Scope every widget ID under the screen so two instances on the same
        // frame (a future comparison view) cannot collide, and so the IDs are
        // stable while the caller's own ID stack is stable.
        ui.push_id("debug-screen");
        let field_id = ui.id("field");
        let button_id = ui.id("button");
        let list_id = ui.id("list");

        ui.draw().text(
            [title_rect.x, title_rect.y],
            TITLE,
            TextStyle::new(TITLE_SCALE, style::TITLE),
        );

        text_field(ui, field_id, field_rect, atlas, &mut self.field);
        if button(ui, button_id, button_rect, atlas, BUTTON_LABEL).clicked {
            self.clicks = self.clicks.saturating_add(1);
        }
        list(ui, list_id, list_rect, &self.items, &mut self.list);

        ui.pop_id();
    }

    /// The text field content after the last [`Self::show`].
    #[must_use]
    pub fn text(&self) -> &str {
        self.field.text()
    }

    /// The caret as a `char` index in `0..=char_count` after the last
    /// [`Self::show`].
    #[must_use]
    pub fn caret(&self) -> usize {
        self.field.caret()
    }

    /// How many primary clicks the demo button received.
    #[must_use]
    pub fn click_count(&self) -> u32 {
        self.clicks
    }

    /// The selected list row, if any.
    #[must_use]
    pub fn selection(&self) -> Option<usize> {
        self.list.selected()
    }

    /// The list scroll offset in pixels.
    #[must_use]
    pub fn scroll_offset(&self) -> f32 {
        self.list.offset()
    }

    /// The rectangle of the text field in the last frame, or [`Rect::ZERO`]
    /// before the first [`Self::show`].
    ///
    /// A driver that synthesises pointer input can use it to click the field.
    #[must_use]
    pub fn field_rect(&self) -> Rect {
        self.field_rect
    }

    /// The rectangle of the demo button in the last frame, or
    /// [`Rect::ZERO`] before the first [`Self::show`].
    #[must_use]
    pub fn button_rect(&self) -> Rect {
        self.button_rect
    }

    /// The rectangle of the list in the last frame, or [`Rect::ZERO`] before
    /// the first [`Self::show`].
    #[must_use]
    pub fn list_rect(&self) -> Rect {
        self.list_rect
    }
}

impl Default for DebugScreen {
    fn default() -> Self {
        Self::new()
    }
}
