// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book and sign screens: drawing, buttons and keys around the state
//! machines of [`umicraft_game::editors`] (MCP `GuiScreenBook`, `GuiEditSign`).
//!
//! Deviations from the reference, recorded in `docs/parity.md`: the sign is a
//! flat board instead of the 3D sign model, and the page arrows are plain
//! hit areas drawn from `book.png`.

use umicraft_assets::GuiAtlas;
use umicraft_game::editors::book::{BookEditor, PAGE_WIDTH};
use umicraft_game::editors::sign::SignEditor;
use umicraft_game::editors::wrap::wrap;
use umicraft_ui::widgets::button_enabled;
use umicraft_ui::{gui_scale, FontAtlas, Rect, TextStyle, Ui};

use crate::game::{GameEvent, TextScreenRequest};
use crate::screen::Screen;

/// Size of the book texture in GUI pixels.
const BOOK_SIZE: f32 = 192.0;
/// Top of the book, from the top of the screen.
const BOOK_TOP: f32 = 2.0;
/// Left edge and top edge of the page text, from the book's corner.
const PAGE_TEXT: (f32, f32) = (36.0, 34.0);
/// Height of a text line.
const LINE_HEIGHT: f32 = 9.0;
/// Size of a page arrow and where the arrows sit.
const ARROW: (f32, f32) = (23.0, 13.0);
const NEXT_AT: (f32, f32) = (120.0, 156.0);
const PREVIOUS_AT: (f32, f32) = (38.0, 156.0);
/// Row of `book.png` where the arrows start, and the distance between them.
const ARROW_V: u32 = 192;
const ARROW_V_STEP: u32 = 13;
/// Width and height of the buttons under the book and of the sign's button.
const BUTTON: (f32, f32) = (98.0, 20.0);
const DONE_WIDE: f32 = 200.0;
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const GREY: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
const WHITE: [f32; 4] = [1.0; 4];
/// Planks of the flat sign.
const BOARD: [f32; 4] = [0.55, 0.42, 0.25, 1.0];
const BOARD_EDGE: [f32; 4] = [0.30, 0.22, 0.12, 1.0];

/// A key the text screens act on, already independent of the windowing
/// library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key<'a> {
    /// Typed characters.
    Text(&'a str),
    /// Backspace.
    Backspace,
    /// Enter.
    Enter,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Escape.
    Escape,
}

/// How the screen ended; the event is sent to the game thread.
#[derive(Debug)]
pub(crate) struct Done(pub Option<GameEvent>);

#[derive(Debug)]
struct SignScreen {
    position: (i32, i32, i32),
    editor: SignEditor,
}

#[derive(Debug)]
struct BookScreen {
    editor: BookEditor,
    author: String,
}

/// The open text screen, if any.
#[derive(Debug, Default)]
pub(crate) struct TextScreens {
    sign: Option<SignScreen>,
    book: Option<BookScreen>,
}

impl TextScreens {
    /// Opens the screen a request asks for and returns the [`Screen`] to show;
    /// `None` when the request cannot be shown (a stack that is no book).
    pub(crate) fn open(&mut self, request: TextScreenRequest) -> Option<Screen> {
        *self = Self::default();
        match request {
            TextScreenRequest::Sign { position, lines } => {
                self.sign = Some(SignScreen {
                    position,
                    editor: SignEditor::new(lines),
                });
                Some(Screen::Sign)
            }
            TextScreenRequest::Book { stack, author } => {
                self.book = Some(BookScreen {
                    editor: BookEditor::open(&stack)?,
                    author,
                });
                Some(Screen::Book)
            }
        }
    }

    /// Applies a key. `measure` gives the width of a text in GUI pixels.
    /// Returns `Some` when the key ends the screen.
    pub(crate) fn key(&mut self, key: Key<'_>, measure: &dyn Fn(&str) -> f32) -> Option<Done> {
        if let Some(sign) = self.sign.as_mut() {
            return sign.key(key, measure);
        }
        self.book.as_mut()?.key(key, measure)
    }

    /// Draws the open screen and returns `Some` when a button ended it.
    pub(crate) fn draw(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        viewport: Rect,
        gui_atlas: Option<&GuiAtlas>,
        caret_visible: bool,
    ) -> Option<Done> {
        let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
        let view = View {
            ui,
            atlas,
            viewport,
            scale,
        };
        if let Some(sign) = self.sign.as_mut() {
            return sign.draw(view, caret_visible);
        }
        self.book.as_mut()?.draw(view, gui_atlas, caret_visible)
    }
}

/// What every draw function needs.
struct View<'a> {
    ui: &'a mut Ui,
    atlas: &'a FontAtlas,
    viewport: Rect,
    scale: f32,
}

impl View<'_> {
    /// A rectangle in GUI pixels from the top-left of the screen.
    fn rect(&self, x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(
            self.viewport.x + x * self.scale,
            self.viewport.y + y * self.scale,
            width * self.scale,
            height * self.scale,
        )
    }

    fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
        let position = [
            self.viewport.x + x * self.scale,
            self.viewport.y + y * self.scale,
        ];
        self.ui
            .draw()
            .text(position, text, TextStyle::new(self.scale, color));
    }

    fn width(&self, text: &str) -> f32 {
        self.atlas.measure(text, 1.0)
    }

    fn button(&mut self, id: &str, rect: Rect, label: &str, enabled: bool) -> bool {
        let id = self.ui.id(id);
        button_enabled(self.ui, id, rect, self.atlas, label, enabled).clicked
    }

    /// The screen width in GUI pixels.
    fn gui_width(&self) -> f32 {
        self.viewport.width / self.scale
    }
}

impl SignScreen {
    fn key(&mut self, key: Key<'_>, measure: &dyn Fn(&str) -> f32) -> Option<Done> {
        match key {
            Key::Text(text) => self.editor.insert(text, measure),
            Key::Backspace => self.editor.backspace(),
            Key::Up => self.editor.up(),
            Key::Down => self.editor.down(),
            Key::Enter => self.editor.down(),
            Key::Escape => return Some(self.finish()),
        }
        None
    }

    /// `GuiEditSign.onGuiClosed` sends the lines however the screen ends.
    fn finish(&self) -> Done {
        Done(Some(GameEvent::SignEdited {
            position: self.position,
            lines: self.editor.json_lines(),
        }))
    }

    fn draw(&mut self, mut view: View<'_>, caret_visible: bool) -> Option<Done> {
        view.ui.draw().rect(view.viewport, [0.0, 0.0, 0.0, 0.55]);
        let centre = view.gui_width() / 2.0;
        let title = "Edit sign message:";
        let title_x = centre - view.width(title) / 2.0;
        view.text(title_x, 40.0, title, WHITE);

        let board = view.rect(centre - 60.0, 70.0, 120.0, 60.0);
        view.ui.draw().rect(board, BOARD_EDGE);
        let inner = view.rect(centre - 58.0, 72.0, 116.0, 56.0);
        view.ui.draw().rect(inner, BOARD);
        for (index, line) in self.editor.lines().iter().enumerate() {
            let selected = index == self.editor.line();
            let shown = if selected && caret_visible {
                format!("> {line} <")
            } else {
                line.clone()
            };
            let x = centre - view.width(&shown) / 2.0;
            view.text(x, 76.0 + index as f32 * 10.0, &shown, BLACK);
        }

        let done = view.rect(centre - DONE_WIDE / 2.0, 150.0, DONE_WIDE, BUTTON.1);
        view.button("sign_done", done, "Done", true)
            .then(|| self.finish())
    }
}

impl BookScreen {
    fn key(&mut self, key: Key<'_>, measure: &dyn Fn(&str) -> f32) -> Option<Done> {
        match key {
            Key::Text(text) => self.editor.insert(text, measure),
            Key::Backspace => self.editor.backspace(),
            Key::Enter if self.editor.is_signing() => {
                if !self.editor.title().trim().is_empty() {
                    return Some(self.finish(true));
                }
            }
            Key::Enter => self.editor.newline(measure),
            Key::Up | Key::Down => {}
            Key::Escape => return Some(Done(None)),
        }
        None
    }

    /// The packet of a finished book (`sendBookToServer`), if any.
    fn finish(&self, publish: bool) -> Done {
        match self.editor.packet(publish, &self.author) {
            Ok(packet) => Done(packet.map(GameEvent::BookEdited)),
            Err(error) => {
                tracing::warn!(%error, "book not sent");
                Done(None)
            }
        }
    }

    fn draw(
        &mut self,
        mut view: View<'_>,
        gui_atlas: Option<&GuiAtlas>,
        caret_visible: bool,
    ) -> Option<Done> {
        let left = ((view.gui_width() - BOOK_SIZE) / 2.0).floor();
        let region = gui_atlas.and_then(|atlas| atlas.region("book"));
        let book = view.rect(left, BOOK_TOP, BOOK_SIZE, BOOK_SIZE);
        match region {
            Some(region) => {
                let src = region.sub_region(0, 0, BOOK_SIZE as u32, BOOK_SIZE as u32);
                view.ui.draw().image(src, book, WHITE);
            }
            None => view.ui.draw().rect(book, [0.85, 0.78, 0.6, 1.0]),
        }
        let origin = (left, BOOK_TOP);
        if self.editor.is_signing() {
            self.draw_signing(&mut view, origin, caret_visible);
        } else {
            self.draw_page(&mut view, origin, caret_visible);
            if let Some(done) = self.draw_arrows(&mut view, region, origin) {
                return Some(done);
            }
        }
        self.draw_buttons(&mut view)
    }

    fn draw_page(&self, view: &mut View<'_>, origin: (f32, f32), caret_visible: bool) {
        let indicator = format!(
            "Page {} of {}",
            self.editor.current() + 1,
            self.editor.page_count()
        );
        let indicator_x = origin.0 + BOOK_SIZE - 44.0 - view.width(&indicator);
        view.text(indicator_x, origin.1 + 16.0, &indicator, BLACK);

        let mut text = self.editor.page().to_owned();
        if self.editor.is_editable() && caret_visible {
            text.push('_');
        }
        let measure = |s: &str| view.atlas.measure(s, 1.0);
        let lines = wrap(&text, PAGE_WIDTH, &measure);
        for (index, line) in lines.iter().enumerate() {
            let y = origin.1 + PAGE_TEXT.1 + index as f32 * LINE_HEIGHT;
            view.text(origin.0 + PAGE_TEXT.0, y, line, BLACK);
        }
    }

    fn draw_signing(&self, view: &mut View<'_>, origin: (f32, f32), caret_visible: bool) {
        let centred =
            |view: &View<'_>, text: &str| origin.0 + (BOOK_SIZE - view.width(text)) / 2.0 + 0.0;
        let prompt = "Enter Book Title:";
        view.text(centred(view, prompt), origin.1 + 34.0, prompt, BLACK);
        let mut title = self.editor.title().to_owned();
        if caret_visible {
            title.push('_');
        }
        view.text(centred(view, &title), origin.1 + 50.0, &title, BLACK);
        let by = format!("by {}", self.author);
        view.text(centred(view, &by), origin.1 + 60.0, &by, GREY);
        let note = "Note! When you sign the book, it will no longer be editable.";
        let measure = |s: &str| view.atlas.measure(s, 1.0);
        for (index, line) in wrap(note, PAGE_WIDTH, &measure).iter().enumerate() {
            let y = origin.1 + 82.0 + index as f32 * LINE_HEIGHT;
            view.text(origin.0 + PAGE_TEXT.0, y, line, BLACK);
        }
    }

    fn draw_arrows(
        &mut self,
        view: &mut View<'_>,
        region: Option<&umicraft_assets::GuiRegion>,
        origin: (f32, f32),
    ) -> Option<Done> {
        let arrows = [
            ("book_next", NEXT_AT, 0, self.editor.has_next()),
            ("book_previous", PREVIOUS_AT, 1, self.editor.has_previous()),
        ];
        for (id, at, row, visible) in arrows {
            if !visible {
                continue;
            }
            let rect = view.rect(
                origin.0 + at.0,
                origin.1 + at.1 - BOOK_TOP,
                ARROW.0,
                ARROW.1,
            );
            let widget = view.ui.id(id);
            let response = view.ui.response(widget, rect);
            if let Some(region) = region {
                let u = if response.hovered { ARROW.0 as u32 } else { 0 };
                let v = ARROW_V + row * ARROW_V_STEP;
                let src = region.sub_region(u, v, ARROW.0 as u32, ARROW.1 as u32);
                view.ui.draw().image(src, rect, WHITE);
            }
            if response.clicked {
                if row == 0 {
                    self.editor.next_page();
                } else {
                    self.editor.previous_page();
                }
            }
        }
        None
    }

    fn draw_buttons(&mut self, view: &mut View<'_>) -> Option<Done> {
        let centre = view.gui_width() / 2.0;
        let top = 4.0 + BOOK_SIZE;
        if self.editor.is_signing() {
            let sign = view.rect(centre - 100.0, top, BUTTON.0, BUTTON.1);
            let enabled = !self.editor.title().trim().is_empty();
            if view.button("book_sign", sign, "Sign", enabled) {
                return Some(self.finish(true));
            }
            let cancel = view.rect(centre + 2.0, top, BUTTON.0, BUTTON.1);
            if view.button("book_cancel", cancel, "Cancel", true) {
                self.editor.cancel_signing();
            }
        } else if self.editor.is_editable() {
            let sign = view.rect(centre - 100.0, top, BUTTON.0, BUTTON.1);
            if view.button("book_begin_sign", sign, "Sign", true) {
                self.editor.begin_signing();
            }
            let done = view.rect(centre + 2.0, top, BUTTON.0, BUTTON.1);
            if view.button("book_done", done, "Done", true) {
                return Some(self.finish(false));
            }
        } else {
            let done = view.rect(centre - DONE_WIDE / 2.0, top, DONE_WIDE, BUTTON.1);
            if view.button("book_done", done, "Done", true) {
                return Some(Done(None));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::ItemStack;

    fn chars(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn book_request(id: i16) -> TextScreenRequest {
        TextScreenRequest::Book {
            stack: ItemStack {
                item_id: id,
                stack_size: 1,
                damage: 0,
                nbt: None,
            },
            author: "Steve".into(),
        }
    }

    #[test]
    fn requests_open_the_matching_screen() {
        let mut screens = TextScreens::default();
        let sign = TextScreenRequest::Sign {
            position: (1, 2, 3),
            lines: Default::default(),
        };
        assert_eq!(screens.open(sign), Some(Screen::Sign));
        assert_eq!(screens.open(book_request(386)), Some(Screen::Book));
        assert!(screens.sign.is_none(), "one screen at a time");
        assert_eq!(screens.open(book_request(276)), None);
    }

    #[test]
    fn a_sign_always_sends_its_lines_when_it_ends() {
        let mut screens = TextScreens::default();
        screens.open(TextScreenRequest::Sign {
            position: (1, 2, 3),
            lines: Default::default(),
        });
        assert!(screens.key(Key::Text("hi"), &chars).is_none());
        let Some(Done(Some(GameEvent::SignEdited { position, lines }))) =
            screens.key(Key::Escape, &chars)
        else {
            panic!("escape sends the sign");
        };
        assert_eq!(position, (1, 2, 3));
        assert_eq!(lines[0], "\"hi\"");
    }

    #[test]
    fn enter_moves_down_a_sign_line() {
        let mut screens = TextScreens::default();
        screens.open(TextScreenRequest::Sign {
            position: (0, 0, 0),
            lines: Default::default(),
        });
        screens.key(Key::Text("a"), &chars);
        screens.key(Key::Enter, &chars);
        screens.key(Key::Text("b"), &chars);
        let Some(Done(Some(GameEvent::SignEdited { lines, .. }))) =
            screens.key(Key::Escape, &chars)
        else {
            panic!("sign");
        };
        assert_eq!(&lines[..2], ["\"a\"", "\"b\""]);
    }

    #[test]
    fn a_book_escape_sends_nothing_and_enter_adds_a_line() {
        let mut screens = TextScreens::default();
        screens.open(book_request(386));
        screens.key(Key::Text("a"), &chars);
        screens.key(Key::Enter, &chars);
        screens.key(Key::Text("b"), &chars);
        assert_eq!(screens.book.as_ref().unwrap().editor.page(), "a\nb");
        let done = screens.key(Key::Escape, &chars).unwrap();
        assert!(done.0.is_none());
    }

    #[test]
    fn enter_in_the_title_signs_the_book_only_with_a_title() {
        let mut screens = TextScreens::default();
        screens.open(book_request(386));
        screens.key(Key::Text("page"), &chars);
        screens.book.as_mut().unwrap().editor.begin_signing();
        assert!(screens.key(Key::Enter, &chars).is_none(), "no title yet");
        screens.key(Key::Text("Title"), &chars);
        let Some(Done(Some(GameEvent::BookEdited(packet)))) = screens.key(Key::Enter, &chars)
        else {
            panic!("sign packet");
        };
        assert_eq!(packet.channel, "MC|BSign");
    }
}
