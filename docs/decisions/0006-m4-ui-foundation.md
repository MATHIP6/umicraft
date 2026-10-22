# ADR-0006 — M4: an immediate-mode UI crate and the frozen M4 contract

- Status: Accepted
- Date: 2026-10-21
- Deciders: project owner

## Context

M3 is complete: the world and the player live on a game thread, the render
thread only consumes snapshots, and the windowed, screenshot and headless
paths all exist. M4 is the next milestone (spec §12.1): an **interface
foundation** made of an immediate-mode widget layer, a real 1.8 font with the
built-in fallback, and a debug screen that exercises a button, a text field
and a scrollable list in both the windowed and the headless path.

M4 is also the point where open decision §13.3 (GUI approach) has to be
settled for the in-game layer. The current overlay is already immediate-mode:
it rebuilds a vertex list every frame from plain data. The new layer must not
force the render thread to wait for a second UI runtime, and it must be
testable without a GPU so that the acceptance suite can drive it in CI.

The work is split across several crates, so spec §14.11 applies: the public
types, the file ownership and the acceptance criteria are frozen in this ADR
before the workstreams start. Two contributors must never edit the same file.

## Options

### GUI approach

1. **egui for the menus, custom immediate mode for the in-game HUD.**
   Rejected for M4. It introduces two UI stacks, a second render path and a
   large dependency for screens that are still simple. The spec allows egui
   for menus; that option stays open for M12 if the main menu outgrows the
   custom layer, but M4 must prove the custom layer first.
2. **Custom immediate-mode layer over the existing overlay.** *Chosen.* It
   matches the existing debug overlay, keeps the hot path allocation-light,
   is unit-testable without a GPU, and gives mods a plain draw list to extend
   later.
3. **Retained-mode widget tree.** Rejected for M4. It adds reconciliation and
   synchronization state for a debug screen and a future HUD that are rebuilt
   every frame anyway.

### Crate placement

1. **Inside `umicraft-render`.** Rejected: layout would only be testable with
   a GPU, and widget code would couple to wgpu resources.
2. **Inside `umicraft-client`.** Rejected: the layout and focus rules are not
   client-specific, and the headless path would still depend on the window
   module.
3. **A pure `umicraft-ui` crate.** *Chosen.* It owns layout, focus, widgets,
   text metrics and the draw list; the renderer owns textures and vertices;
   the client owns window input and wiring.

## Decision

### The crate

- Create `umicraft-ui` as a pure library: no GPU, window, network, filesystem
  or game dependency. Its only external dependency is `png` (already used by
  the client), for decoding the user-supplied 1.8 font atlas.
- `umicraft-render` and `umicraft-client` depend on `umicraft-ui`, never the
  other way around.
- The UI frame is built on the **render thread**, after the game frame has
  been received and before the GPU submission. No new thread, no shared
  mutable state with the game, network or mesh threads.
- The headless and screenshot paths drive the same `umicraft-ui` code with a
  scripted input queue; they do not reimplement widgets.

### Boundaries

- `DrawList` is the only output the renderer sees: solid rectangles, text
  runs, and clip scopes. The renderer does not know what a button is.
- `FontAtlas` is plain data: an R8 coverage bitmap, one glyph record per
  character and the line height. The renderer uploads it once and draws
  quads; the UI crate parses and measures.
- Input is the small backend-agnostic `UiInput` vocabulary. The client turns
  winit events (including `KeyEvent.text`/IME) into it; the headless driver
  builds it directly.
- Geometry is in **physical pixels with a top-left origin**, the same space
  as `Overlay::rect` and `Overlay::text`. DPI scaling is applied by the
  client when it builds the input and the viewport.

### Fonts and the clean-room rules

- The real 1.8 font is the user's
  `assets/minecraft/textures/font/ascii.png`, read at runtime from the
  client jar (spec M4 and AST-1/2). It is never copied into the repository,
  a test fixture or a release (spec §3 rules 3 and 4).
- The built-in 3×5 fallback is original work and the only font committed to
  the repository. It is used before the assets are installed and when the
  user has no client jar.
- Tests use a synthetic PNG/atlas generated in the test; a test against a
  real client jar is ignored by default or gated on an environment variable.
- Any non-obvious 1.8 font metric is cited in `docs/sources.md` before the
  workstream is merged (ADR-0001, spec §14.6). The MCP tree may be consulted
  as a reference under ADR-0001.

### Acceptance screen

- The debug screen is a pure function of its state and `UiInput`; it lives in
  `umicraft-ui::debug_screen`, not in the client.
- The windowed client opens it with **F2**.
- The headless client exposes `--ui-demo`: it runs a scripted sequence (focus
  the field, type, Backspace, move the caret, click the button, scroll the
  list) and prints a report that CI can assert.
- The screenshot path includes the same screen when `--ui-demo` is combined
  with `--screenshot`, so a visual artifact can be attached to the milestone.

### Build and merge rules

- One feature branch, one worktree and one `target/` per workstream:
  `.worktrees/m4-<name>` and `feat/m4-<name>`.
- Concurrent worktrees MUST NOT share `CARGO_TARGET_DIR` (spec §14.12).
  During development each agent runs targeted checks; the lead runs the full
  workspace gates at integration.
- At most three agents run at the same time, to keep the builds and the
  review queue manageable.
- No agent merges to `main`. The lead integrates in the order
  **W1 → W2 → W3 → W4 → W5**.
- Shared files are lead-only: root `Cargo.toml`, `Cargo.lock`, CI workflows,
  `docs/**`, `README.md` and `UMICRAFT_SPEC.md`. A workstream that needs a
  shared-file change stops and reports instead of editing it.
- An interface change is a stop-the-line event: the agent reports it, the
  lead updates this ADR and notifies the other workstreams before the change
  lands.

## Consequences

- The repository gains one pure crate and two dependency edges
  (`umicraft-render → umicraft-ui`, `umicraft-client → umicraft-ui`).
- Layout, focus, editing and text measurement become unit-testable without a
  window or a GPU. The acceptance suite can run the debug screen headless.
- The renderer gains one UI pass and one font-atlas upload; the existing HUD
  overlay stays on the same pipeline, so M3 screenshots keep working.
- The real font is only available after the user has installed the assets or
  pointed the client at an official installation; the fallback is mandatory,
  not a test-only path.
- The UI crate must stay free of `umicraft-render`, `umicraft-client`,
  `umicraft-game`, `umicraft-net` and `umicraft-protocol` dependencies. The
  workspace lints (`missing_docs`, `unsafe_code = deny`, clippy `all`) apply.

---

# Appendix A — ownership map

| Workstream | Owns | Must not touch |
|---|---|---|
| **W0 — lead** | Root `Cargo.toml`, `Cargo.lock`, `.github/workflows/**`, `docs/**`, `README.md`, `UMICRAFT_SPEC.md`, `crates/umicraft-ui/Cargo.toml` | Feature source files |
| **W1 — UI core** | `crates/umicraft-ui/src/{lib.rs,geom.rs,id.rs,input.rs,draw.rs,context}.rs`, `crates/umicraft-ui/tests/**` except `text.rs`/`widgets.rs` | Everything else |
| **W2 — text & font** | `crates/umicraft-ui/src/text.rs` (or `src/text/**`), `crates/umicraft-ui/tests/text.rs` | Everything else |
| **W3 — widgets** | `crates/umicraft-ui/src/widgets/**`, `crates/umicraft-ui/src/debug_screen.rs`, `crates/umicraft-ui/tests/widgets.rs` | Everything else |
| **W4 — render pass** | `crates/umicraft-render/src/{overlay.rs,renderer.rs,lib.rs}`, new `crates/umicraft-render/src/ui_pass.rs`, `crates/umicraft-render/Cargo.toml`, `crates/umicraft-render/tests/**` | Everything else |
| **W5 — client & acceptance** | `crates/umicraft-client/**`, `crates/umicraft-testkit/**`, `crates/umicraft-client/Cargo.toml` | Everything else |

Rules:

- No two workstreams edit the same file.
- `crates/umicraft-ui/src/lib.rs` already declares every module; W1 owns it.
  W2/W3 do not add modules without the lead updating the skeleton first.
- W4 adds `umicraft-ui.workspace = true` to the render manifest; W5 does the
  same for the client manifest. The root workspace dependency is already
  declared by W0.
- Tests belong to the workstream that owns the code they exercise.
  Cross-crate acceptance tests live in W5.

# Appendix B — frozen public surface

The exact bodies are filled by the workstreams. Names, ownership and
semantics below are frozen; parameter types may be refined if the refinement
does not force another workstream to change its files.

```rust
// geometry, IDs, input, draw list, text atlas, context
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
pub struct WidgetId(u64);
pub enum InputEvent { /* pointer, wheel, key, text */ }
pub struct UiInput { pub events: Vec<InputEvent>, pub modifiers: Modifiers }
pub struct DrawList { /* commands: Rect, Text, PushClip, PopClip */ }
pub struct FontAtlas { /* R8 pixels + glyphs + line height + white UV */ }

pub struct Ui { /* per-frame immediate-mode state */ }
impl Ui {
    pub fn new(viewport: Rect) -> Self;
    pub fn begin_frame(&mut self, viewport: Rect, input: UiInput);
    pub fn end_frame(&mut self) -> UiOutput;
    pub fn viewport(&self) -> Rect;
    pub fn input(&self) -> &UiInput;
    pub fn draw(&mut self) -> &mut DrawList;
    pub fn push_id(&mut self, label: &str);
    pub fn pop_id(&mut self);
    pub fn id(&self, label: &str) -> WidgetId;
    pub fn focus(&self) -> Option<WidgetId>;
    pub fn set_focus(&mut self, focus: Option<WidgetId>);
    pub fn is_hovered(&self, rect: Rect) -> bool;
    pub fn response(&mut self, id: WidgetId, rect: Rect) -> UiResponse;
}

pub struct UiResponse {
    pub id: WidgetId,
    pub hovered: bool,
    pub clicked: bool,
    pub changed: bool,
    pub focused: bool,
}

pub struct UiOutput {
    pub draw_list: DrawList,
    pub responses: Vec<UiResponse>,
    pub focus: Option<WidgetId>,
}
```

Widget contract (W3; exact state types are W3's):

```rust
pub fn panel(ui: &mut Ui, rect: Rect, color: Color);
pub fn button(ui: &mut Ui, id: WidgetId, rect: Rect, label: &str) -> UiResponse;
pub fn text_field(ui: &mut Ui, id: WidgetId, rect: Rect, state: &mut TextFieldState) -> UiResponse;
pub fn list(ui: &mut Ui, id: WidgetId, rect: Rect, items: &[String], state: &mut ListState) -> UiResponse;
```

`debug_screen` must expose a pure screen driven by `&mut Ui`; it must not read
files, time, the network or global input. W3 defines its state type and its
`show` method.

Invariants:

- `Ui` is not shared across threads; it is owned by the render thread or the
  headless driver.
- `begin_frame`/`end_frame` reuse the draw and response allocations.
- `DrawList` is ordered; every `PushClip` has a matching `PopClip`.
- The UI crate has no `unwrap()`/`expect()` on external data and no `unsafe`.
- Text measurement and caret indices are in `char` boundaries, never bytes.
- The fallback font is original; the real font is loaded at runtime only.

# Appendix C — M4 acceptance map

| Spec M4 criterion | Owner |
|---|---|
| A debug screen with a list, a text field and a button | W3, W5 |
| The text field edits text with a visible caret | W3 |
| Layout maths unit-tested | W1 |
| Usable from the windowed and the headless path | W5, same `debug_screen` |
| Real 1.8 font + built-in fallback | W2, W5 |
| `cargo deny` green, no dependency outside the allow list | W0 / CI |
| No Mojang asset in the repository | W2 / W0 / CI |

# Appendix D — command and handoff rules

Each agent receives a card from `docs/tasks/m4/`. The final report contains:

1. the commit SHA(s) on the feature branch;
2. the files changed, checked against Appendix A;
3. the exact commands run and their result;
4. any deviation from this ADR and any open question.

The lead merges in order, runs the full gates, updates this ADR's status and
the M4 sections of `README.md`/`docs/threading.md`, and only then creates the
next milestone's worktrees.
