# ADR-0071 — Shared caret and selection for text inputs

Status: accepted (2026-11-11)

## Context

The UI text field (`TextFieldState`) had a selection with Control+A/C/X/V and
Shift movement. The chat line (`ChatInput`) had none: no selection, no copy or
paste, and Control+Shift+arrow did not extend anything. Both also carried their
own copy of the word-movement rule (`getNthWordFromPos`). `umicraft-ui` and
`umicraft-game` do not depend on each other.

## Decision

- `umicraft_core::text_edit` holds the two pieces both inputs need: `Selection`
  (a caret and its anchor, as `GuiTextField.cursorPosition` and
  `selectionEnd`) and `word_boundary`. Both crates already depend on core.
- Each input keeps its own buffer (`String` for the field, `Vec<char>` for the
  chat, which needs cheap insertion at the caret and the 100 character cap)
  and its own filtering rules. Only the caret logic is shared.
- `ChatInput` stays I/O-free. The window owns the system clipboard: it copies
  `selected_text`, writes what `cut` returns and inserts what it reads.
  Copying an empty selection does not touch the clipboard.
- `left`, `right`, `home` and `end` take an `extend` flag (Shift).

## Update: mouse, multi-click and undo (same day)

Umicraft additions beyond vanilla, shared by both inputs through
`umicraft_core::text_edit`:

- `ClickSeries` counts clicks on the same character boundary within 500 ms:
  one places the caret, two select a word, three the line. Dragging extends
  the selection until the button goes up or the focus leaves the field;
  Shift+click extends from the caret.
- `History` keeps up to 100 undo steps. A run of typed characters, or of
  single-character deletions, is one step; a paste, a cut, a word deletion or
  typing over a selection is a step of its own; moving the caret ends a run.
  Control+Z undoes, Control+Y and Control+Shift+Z redo. `set_text` clears it.
- The chat line is hit-tested by `umicraft_render::chat_input_hit`, which uses
  the layout `draw_chat` draws; a drag keeps following the pointer after it
  leaves the line.
- Control shortcuts follow the layout's letter (`ui_input::shortcut_code`):
  on an AZERTY keyboard Control+A is the key at the QWERTY `Q` position.
  Gameplay keys keep matching by position.

## Consequences

One implementation of the selection rules, covered by the core unit tests.
The chat view carries the selected range so the HUD draws the highlight.
