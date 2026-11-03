// SPDX-License-Identifier: MIT OR Apache-2.0

//! The name field of the anvil screen (MCP `GuiRepair`).
//!
//! A pure state machine: the window feeds it edits and the stack in the first
//! slot, it answers with the name to send in `MC|ItemName`. Like the
//! reference, the field shows the display name of the item and sends an empty
//! name while the text is the item's untouched default name.

use umicraft_game::chat::ChatInput;
use umicraft_game::inventory::payload::ANVIL_NAME_LIMIT;
use umicraft_game::inventory::tooltip::tooltip;
use umicraft_protocol::packets::play::ItemStack;

use crate::tooltip_draw::resolve;

/// One edit of the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edit<'a> {
    /// Typed characters.
    Insert(&'a str),
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Caret left.
    Left,
    /// Caret right.
    Right,
    /// Caret to the start.
    Home,
    /// Caret to the end.
    End,
}

/// What the screen draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldView {
    /// The text in the field.
    pub text: String,
    /// The caret position in characters.
    pub caret: usize,
    /// Whether the field takes input (an item sits in the first slot).
    pub enabled: bool,
}

/// The state of the field.
#[derive(Debug, Default)]
pub(crate) struct AnvilField {
    input: Option<ChatInput>,
    /// The stack of the first slot the text was last taken from; the outer
    /// option is `None` until the first [`AnvilField::sync`].
    source: Option<Option<ItemStack>>,
}

impl AnvilField {
    /// Follows the first anvil slot. When its stack changed, the text becomes
    /// the display name of the new stack (or empty); the returned name is the
    /// one to send, as `GuiRepair.sendSlotContents` does when a stack is
    /// present.
    pub(crate) fn sync(&mut self, slot: Option<&ItemStack>) -> Option<String> {
        if self
            .source
            .as_ref()
            .is_some_and(|seen| seen.as_ref() == slot)
        {
            return None;
        }
        self.source = Some(slot.cloned());
        let text = slot.map(display_name).unwrap_or_default();
        self.input = Some(ChatInput::new(&text));
        slot.map(|stack| outgoing_name(stack, &text))
    }

    /// Applies `edit`; returns the name to send when the text changed.
    pub(crate) fn edit(&mut self, edit: Edit<'_>) -> Option<String> {
        let stack = self.source.as_ref()?.as_ref()?;
        let input = self.input.as_mut()?;
        let before = input.text();
        match edit {
            Edit::Insert(text) => {
                input.insert(text);
                if input.len() > ANVIL_NAME_LIMIT {
                    let kept: String = input.text().chars().take(ANVIL_NAME_LIMIT).collect();
                    input.set_text(&kept);
                }
            }
            Edit::Backspace => input.backspace(false),
            Edit::Delete => input.delete(false),
            Edit::Left => input.left(false),
            Edit::Right => input.right(false),
            Edit::Home => input.home(),
            Edit::End => input.end(),
        }
        let after = input.text();
        (after != before).then(|| outgoing_name(stack, &after))
    }

    /// What to draw.
    pub(crate) fn view(&self) -> FieldView {
        let enabled = matches!(self.source, Some(Some(_)));
        match &self.input {
            Some(input) => FieldView {
                text: input.text(),
                caret: input.caret(),
                enabled,
            },
            None => FieldView {
                text: String::new(),
                caret: 0,
                enabled,
            },
        }
    }
}

/// The name an item shows: its custom name or its translated name.
fn display_name(stack: &ItemStack) -> String {
    tooltip(stack)
        .first()
        .map(|line| resolve(&line.text))
        .unwrap_or_default()
}

/// `GuiRepair.renameItem`: an item without a custom name whose field still
/// reads its default name is sent with an empty name, so the server does not
/// rename it.
fn outgoing_name(stack: &ItemStack, text: &str) -> String {
    let unnamed = ItemStack {
        nbt: None,
        ..stack.clone()
    };
    let has_custom_name = display_name(stack) != display_name(&unnamed);
    if !has_custom_name && text == display_name(stack) {
        String::new()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sword() -> ItemStack {
        ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        }
    }

    #[test]
    fn an_empty_slot_disables_the_field_and_sends_nothing() {
        let mut field = AnvilField::default();
        assert_eq!(field.sync(None), None);
        assert!(!field.view().enabled);
        assert_eq!(field.edit(Edit::Insert("a")), None);
    }

    #[test]
    fn a_new_item_fills_the_field_with_its_name_and_sends_an_empty_rename() {
        let mut field = AnvilField::default();
        let sent = field.sync(Some(&sword()));
        assert_eq!(sent, Some(String::new()));
        let view = field.view();
        assert!(view.enabled);
        assert!(!view.text.is_empty(), "the translated name");
        assert_eq!(view.caret, view.text.chars().count());
    }

    #[test]
    fn the_same_stack_does_not_reset_the_text() {
        let mut field = AnvilField::default();
        field.sync(Some(&sword()));
        field.edit(Edit::Insert("!"));
        let typed = field.view().text;
        assert_eq!(field.sync(Some(&sword())), None);
        assert_eq!(field.view().text, typed);
    }

    #[test]
    fn typing_sends_the_new_name() {
        let mut field = AnvilField::default();
        field.sync(Some(&sword()));
        let default = field.view().text;
        assert_eq!(field.edit(Edit::Insert("!")), Some(format!("{default}!")));
        // Removing it again returns to the untouched default: empty rename.
        assert_eq!(field.edit(Edit::Backspace), Some(String::new()));
    }

    #[test]
    fn moving_the_caret_sends_nothing() {
        let mut field = AnvilField::default();
        field.sync(Some(&sword()));
        assert_eq!(field.edit(Edit::Left), None);
        assert_eq!(field.edit(Edit::Home), None);
    }

    #[test]
    fn the_name_is_capped_at_the_field_limit() {
        let mut field = AnvilField::default();
        field.sync(Some(&sword()));
        field.edit(Edit::Home);
        field.edit(Edit::Insert(&"x".repeat(100)));
        assert_eq!(field.view().text.chars().count(), ANVIL_NAME_LIMIT);
    }
}
