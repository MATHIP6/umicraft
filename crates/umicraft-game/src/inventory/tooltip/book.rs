// SPDX-License-Identifier: MIT OR Apache-2.0

//! The written book in a tooltip (MCP `ItemEditableBook`): its title as the
//! name, then the author and the generation.

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::nbt::Nbt;

/// Registry name of the written book.
pub(super) const WRITTEN_BOOK: &str = "written_book";

/// `getItemStackDisplayName`: the `title` tag when it is not empty.
pub(super) fn title(root: Option<&Nbt>) -> Option<&str> {
    root?
        .get("title")?
        .as_str()
        .filter(|title| !title.is_empty())
}

/// `addInformation`: `book.byAuthor` when there is an author, then
/// `book.generation.N` (0 for a missing generation), both grey, for any book
/// with a tag.
pub(super) fn lines(root: Option<&Nbt>) -> Vec<TooltipLine> {
    let Some(root) = root else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(author) = root
        .get("author")
        .and_then(Nbt::as_str)
        .filter(|a| !a.is_empty())
    {
        lines.push(TooltipLine::plain(
            TooltipText::Translation {
                key: "book.byAuthor".into(),
                args: vec![TooltipText::literal(author)],
            },
            TooltipColor::Gray,
        ));
    }
    let generation = root.get("generation").and_then(Nbt::as_int).unwrap_or(0);
    lines.push(TooltipLine::plain(
        TooltipText::key(format!("book.generation.{generation}")),
        TooltipColor::Gray,
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(entries: Vec<(&str, Nbt)>) -> Nbt {
        Nbt::Compound(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    #[test]
    fn a_signed_book_shows_its_author_and_generation() {
        let tag = book(vec![
            ("title", Nbt::String("Diary".into())),
            ("author", Nbt::String("Alex".into())),
            ("generation", Nbt::Int(1)),
        ]);
        assert_eq!(title(Some(&tag)), Some("Diary"));
        let lines = lines(Some(&tag));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].text, TooltipText::key("book.generation.1"));
    }

    #[test]
    fn no_author_no_title_and_no_tag_follow_the_reference() {
        let tag = book(vec![("title", Nbt::String(String::new()))]);
        assert_eq!(title(Some(&tag)), None);
        assert_eq!(
            lines(Some(&tag)),
            [TooltipLine::plain(
                TooltipText::key("book.generation.0"),
                TooltipColor::Gray
            )]
        );
        assert!(lines(None).is_empty());
    }
}
