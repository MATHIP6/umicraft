// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book screen, after `GuiScreenBook`: a book and quill is edited, a
//! written book is read.

use umicraft_protocol::packets::play::ItemStack;

use super::wrap::wrap;
use crate::chat::lang::LangTable;
use crate::chat::unformatted_text;
use crate::inventory::payload::{self, PayloadError};
use crate::nbt::Nbt;

/// Item id of the book and quill.
pub const WRITABLE_BOOK: i16 = 386;
/// Item id of the written book.
pub const WRITTEN_BOOK: i16 = 387;
/// Most pages a book takes (`GuiScreenBook.addNewPage`).
pub const MAX_PAGES: usize = 50;
/// Most characters on a page.
pub const PAGE_CHARS: usize = 256;
/// Longest title.
pub const TITLE_CHARS: usize = 16;
/// Width of the text of a page, in GUI pixels.
pub const PAGE_WIDTH: f32 = 118.0;
/// Most lines on a page (`splitStringWidth(...) <= 128` at 9 px per line).
pub const PAGE_LINES: usize = 14;

/// A packet to send: a plugin channel and its payload.
#[derive(Debug, Clone, PartialEq)]
pub struct BookPacket {
    /// `MC|BEdit` or `MC|BSign`.
    pub channel: &'static str,
    /// The book stack.
    pub data: Vec<u8>,
}

/// The state of an open book.
#[derive(Debug, Clone, PartialEq)]
pub struct BookEditor {
    stack: ItemStack,
    pages: Vec<String>,
    current: usize,
    unsigned: bool,
    modified: bool,
    signing: bool,
    title: String,
}

impl BookEditor {
    /// Opens `stack`, a book and quill or a written book; `None` for any
    /// other item. Pages of a written book are chat components and are read
    /// as plain text.
    #[must_use]
    pub fn open(stack: &ItemStack) -> Option<Self> {
        let unsigned = match stack.item_id {
            WRITABLE_BOOK => true,
            WRITTEN_BOOK => false,
            _ => return None,
        };
        let mut pages = read_pages(stack);
        if !unsigned {
            let lang = LangTable::default();
            pages = pages
                .into_iter()
                .map(|page| unformatted_text(&page, &lang))
                .collect();
        }
        if pages.is_empty() && unsigned {
            pages.push(String::new());
        }
        Some(Self {
            stack: stack.clone(),
            pages,
            current: 0,
            unsigned,
            modified: false,
            signing: false,
            title: String::new(),
        })
    }

    /// Whether the book can be edited.
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.unsigned
    }

    /// Whether the signing page (title entry) is shown.
    #[must_use]
    pub fn is_signing(&self) -> bool {
        self.signing
    }

    /// The title being typed.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The text of the current page.
    #[must_use]
    pub fn page(&self) -> &str {
        self.pages.get(self.current).map_or("", String::as_str)
    }

    /// The index of the current page.
    #[must_use]
    pub fn current(&self) -> usize {
        self.current
    }

    /// The number of pages (at least one).
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    /// Whether there is a page before the current one.
    #[must_use]
    pub fn has_previous(&self) -> bool {
        self.current > 0
    }

    /// Whether the next-page button shows: another page exists, or the book
    /// can grow one.
    #[must_use]
    pub fn has_next(&self) -> bool {
        self.current + 1 < self.page_count() || (self.unsigned && self.page_count() < MAX_PAGES)
    }

    /// Goes to the previous page.
    pub fn previous_page(&mut self) {
        self.current = self.current.saturating_sub(1);
    }

    /// Goes to the next page; an editable book adds one past its last page.
    pub fn next_page(&mut self) {
        if self.current + 1 < self.page_count() {
            self.current += 1;
        } else if self.unsigned && self.page_count() < MAX_PAGES {
            self.pages.push(String::new());
            self.modified = true;
            self.current += 1;
        }
    }

    /// Opens the signing page of an editable book.
    pub fn begin_signing(&mut self) {
        self.signing = self.unsigned;
    }

    /// Leaves the signing page.
    pub fn cancel_signing(&mut self) {
        self.signing = false;
    }

    /// Types `text` into the title while signing, else into the page.
    ///
    /// A page takes characters while it stays under [`PAGE_CHARS`] and its
    /// wrapped text under [`PAGE_LINES`] lines (`pageInsertIntoCurrent`).
    pub fn insert(&mut self, text: &str, measure: &dyn Fn(&str) -> f32) {
        if !self.unsigned {
            return;
        }
        for character in text.chars().filter(|c| !c.is_control()) {
            if self.signing {
                if self.title.chars().count() < TITLE_CHARS {
                    self.title.push(character);
                }
            } else {
                self.push_to_page(character, measure);
            }
        }
    }

    /// Starts a new line on the page.
    pub fn newline(&mut self, measure: &dyn Fn(&str) -> f32) {
        if self.unsigned && !self.signing {
            self.push_to_page('\n', measure);
        }
    }

    /// Removes the last character of the title or the page.
    pub fn backspace(&mut self) {
        if !self.unsigned {
            return;
        }
        if self.signing {
            self.title.pop();
        } else if let Some(page) = self.pages.get_mut(self.current) {
            if page.pop().is_some() {
                self.modified = true;
            }
        }
    }

    fn push_to_page(&mut self, character: char, measure: &dyn Fn(&str) -> f32) {
        let Some(page) = self.pages.get_mut(self.current) else {
            return;
        };
        if page.chars().count() >= PAGE_CHARS {
            return;
        }
        let mut grown = page.clone();
        grown.push(character);
        if wrap(&grown, PAGE_WIDTH, measure).len() <= PAGE_LINES {
            *page = grown;
            self.modified = true;
        }
    }

    /// The packet that ends the screen, if one is due.
    ///
    /// Only a modified book and quill sends anything: `MC|BEdit` with its
    /// pages, or with `publish` `MC|BSign` with the pages as chat components,
    /// the author and the title, as a written book
    /// (`GuiScreenBook.sendBookToServer`). Trailing empty pages are dropped.
    ///
    /// # Errors
    /// [`PayloadError`] when the stack cannot be encoded.
    pub fn packet(&self, publish: bool, author: &str) -> Result<Option<BookPacket>, PayloadError> {
        if !self.unsigned || !self.modified {
            return Ok(None);
        }
        let mut pages = self.pages.clone();
        while pages.len() > 1 && pages.last().is_some_and(String::is_empty) {
            pages.pop();
        }
        let mut entries = base_entries(&self.stack);
        let (channel, item_id) = if publish {
            pages = pages
                .into_iter()
                .map(|page| serde_json::Value::String(page).to_string())
                .collect();
            entries.push(("author".into(), Nbt::String(author.to_owned())));
            entries.push(("title".into(), Nbt::String(self.title.trim().to_owned())));
            (payload::BOOK_SIGN, WRITTEN_BOOK)
        } else {
            (payload::BOOK_EDIT, self.stack.item_id)
        };
        set_entry(
            &mut entries,
            "pages",
            Nbt::List(pages.into_iter().map(Nbt::String).collect()),
        );
        let stack = ItemStack {
            item_id,
            nbt: Nbt::Compound(entries).to_root_bytes(),
            ..self.stack.clone()
        };
        Ok(Some(BookPacket {
            channel,
            data: payload::book(&stack)?,
        }))
    }
}

/// The `pages` of a book stack, empty when it has none.
fn read_pages(stack: &ItemStack) -> Vec<String> {
    stack
        .nbt
        .as_deref()
        .and_then(|bytes| Nbt::parse(bytes).ok())
        .and_then(|tag| tag.get("pages")?.as_list().map(<[Nbt]>::to_vec))
        .map(|list| {
            list.iter()
                .filter_map(|page| page.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The tags of the stack other than `pages`, which the book keeps.
fn base_entries(stack: &ItemStack) -> Vec<(String, Nbt)> {
    match stack
        .nbt
        .as_deref()
        .and_then(|bytes| Nbt::parse(bytes).ok())
    {
        Some(Nbt::Compound(entries)) => entries
            .into_iter()
            .filter(|(name, _)| !matches!(name.as_str(), "pages" | "author" | "title"))
            .collect(),
        _ => Vec::new(),
    }
}

fn set_entry(entries: &mut Vec<(String, Nbt)>, name: &str, value: Nbt) {
    entries.retain(|(existing, _)| existing != name);
    entries.push((name.to_owned(), value));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn book(id: i16, pages: &[&str]) -> ItemStack {
        let list = Nbt::List(pages.iter().map(|p| Nbt::String((*p).into())).collect());
        ItemStack {
            item_id: id,
            stack_size: 1,
            damage: 0,
            nbt: Nbt::Compound(vec![("pages".into(), list)]).to_root_bytes(),
        }
    }

    fn sent_pages(packet: &BookPacket) -> Vec<String> {
        let stack = ItemStack::decode(&mut packet.data.as_slice()).unwrap();
        let tag = Nbt::parse(stack.nbt.as_deref().unwrap()).unwrap();
        tag.get("pages")
            .unwrap()
            .as_list()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn only_books_open() {
        assert!(BookEditor::open(&book(276, &[])).is_none());
        assert!(BookEditor::open(&book(WRITABLE_BOOK, &[])).is_some());
    }

    #[test]
    fn a_new_book_and_quill_has_one_empty_page() {
        let editor = BookEditor::open(&ItemStack {
            nbt: None,
            ..book(WRITABLE_BOOK, &[])
        })
        .unwrap();
        assert_eq!((editor.page_count(), editor.page()), (1, ""));
        assert!(editor.is_editable());
    }

    #[test]
    fn a_written_book_is_read_only_and_shows_plain_text() {
        let mut editor =
            BookEditor::open(&book(WRITTEN_BOOK, &["{\"text\":\"Once\"}", "plain"])).unwrap();
        assert_eq!(editor.page(), "Once");
        editor.insert("x", &chars);
        editor.backspace();
        assert_eq!(editor.page(), "Once");
        assert_eq!(editor.packet(false, "me").unwrap(), None);
        editor.next_page();
        assert_eq!(editor.page(), "plain");
        assert!(!editor.has_next());
    }

    #[test]
    fn typing_goes_to_the_page_until_it_is_full() {
        let mut editor = BookEditor::open(&book(WRITABLE_BOOK, &[""])).unwrap();
        editor.insert(&"a".repeat(300), &|_| 0.0);
        assert_eq!(editor.page().chars().count(), PAGE_CHARS);
    }

    #[test]
    fn a_page_stops_growing_at_fourteen_lines() {
        let mut editor = BookEditor::open(&book(WRITABLE_BOOK, &[""])).unwrap();
        for _ in 0..20 {
            editor.newline(&chars);
        }
        // 14 lines hold 13 newlines.
        assert_eq!(editor.page().matches('\n').count(), PAGE_LINES - 1);
    }

    #[test]
    fn next_adds_pages_up_to_fifty() {
        let mut editor = BookEditor::open(&book(WRITABLE_BOOK, &["one"])).unwrap();
        assert!(editor.has_next());
        for _ in 0..60 {
            editor.next_page();
        }
        assert_eq!(editor.page_count(), MAX_PAGES);
        assert!(!editor.has_next());
        editor.previous_page();
        assert_eq!(editor.current(), MAX_PAGES - 2);
    }

    #[test]
    fn an_untouched_book_sends_nothing() {
        let editor = BookEditor::open(&book(WRITABLE_BOOK, &["one"])).unwrap();
        assert_eq!(editor.packet(false, "me").unwrap(), None);
    }

    #[test]
    fn done_sends_the_pages_without_trailing_empty_ones() {
        let mut editor = BookEditor::open(&book(WRITABLE_BOOK, &["one"])).unwrap();
        editor.insert("!", &chars);
        editor.next_page();
        editor.next_page();
        let packet = editor.packet(false, "me").unwrap().unwrap();
        assert_eq!(packet.channel, payload::BOOK_EDIT);
        assert_eq!(sent_pages(&packet), ["one!"]);
    }

    #[test]
    fn signing_sends_a_written_book_with_json_pages_author_and_title() {
        let mut editor = BookEditor::open(&book(WRITABLE_BOOK, &["hi \"you\""])).unwrap();
        editor.insert("!", &chars);
        editor.begin_signing();
        assert!(editor.is_signing());
        editor.insert(&format!(" {}", "t".repeat(30)), &chars);
        assert_eq!(editor.title().chars().count(), TITLE_CHARS);
        let packet = editor.packet(true, "Steve").unwrap().unwrap();
        assert_eq!(packet.channel, payload::BOOK_SIGN);
        let stack = ItemStack::decode(&mut packet.data.as_slice()).unwrap();
        assert_eq!(stack.item_id, WRITTEN_BOOK);
        let tag = Nbt::parse(stack.nbt.as_deref().unwrap()).unwrap();
        assert_eq!(tag.get("author").unwrap().as_str(), Some("Steve"));
        assert_eq!(tag.get("title").unwrap().as_str(), Some("ttttttttttttttt"));
        assert_eq!(sent_pages(&packet), ["\"hi \\\"you\\\"!\""]);
    }

    #[test]
    fn signing_is_for_editable_books_and_can_be_cancelled() {
        let mut written = BookEditor::open(&book(WRITTEN_BOOK, &["x"])).unwrap();
        written.begin_signing();
        assert!(!written.is_signing());
        let mut editor = BookEditor::open(&book(WRITABLE_BOOK, &[""])).unwrap();
        editor.begin_signing();
        editor.cancel_signing();
        assert!(!editor.is_signing());
    }
}
