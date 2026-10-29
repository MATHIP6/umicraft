// SPDX-License-Identifier: MIT OR Apache-2.0

//! The server-less M4 acceptance run (`--ui-demo`).
//!
//! [`run_script`] drives the real [`DebugScreen`] through the real
//! [`umicraft_ui::UiInput`] and prints the state the screen reached after a
//! fixed sequence: focus the text field, type, delete a character, move the
//! caret, click the button, select a list row and scroll the list. CI can run
//! it with no GPU and no server, and the report is stable because the script
//! contains no clock or randomness.
//!
//! `--ui-demo --screenshot PATH` reuses the same final draw list, uploads the
//! loaded atlas and renders one offscreen frame, so the visual artifact and
//! the numeric report describe the same UI state.
//!
//! The screen content is original Umicraft text (the debug screen lives in
//! `umicraft-ui`); no Mojang asset is embedded here. The real font, when the
//! user has one, is read at runtime by [`crate::ui_font`].

use umicraft_render::{Camera, NoIcons, Overlay, Renderer};
use umicraft_ui::debug_screen::DebugScreen;
use umicraft_ui::widgets::ROW_HEIGHT;
use umicraft_ui::{DrawList, FontAtlas, InputEvent, PointerButton, Rect, Ui, UiInput};

use crate::args::Args;
use crate::error::ClientError;
use crate::screenshot::write_png;
use crate::ui_font::{self, FontSource};

/// The scripted text typed into the field before the edits.
const TYPED: &str = "HELLO";
/// The list row selected by the script (zero-based).
const SELECTED_ROW: usize = 2;
/// The wheel delta used by the script, in UI lines (positive scrolls down).
const SCROLL_LINES: f32 = 3.0;

/// The state reached by one scripted UI demo.
#[derive(Debug, Clone, PartialEq)]
pub struct DemoReport {
    /// Framebuffer size in physical pixels.
    pub viewport: (u32, u32),
    /// Text field content after the script.
    pub text: String,
    /// Caret position as a `char` index after the script.
    pub caret: usize,
    /// Number of primary clicks the demo button received.
    pub button_clicks: u32,
    /// Selected list row, if any.
    pub selection: Option<usize>,
    /// List scroll offset in pixels.
    pub scroll_offset: f32,
}

impl DemoReport {
    /// Formats the stable acceptance report.
    ///
    /// `source` is printed as `official` or `fallback`; it is supplied by the
    /// driver because the pure script does not care which atlas it was given.
    #[must_use]
    pub fn render(&self, source: FontSource) -> String {
        let selection = self
            .selection
            .map_or_else(|| "none".to_string(), |index| index.to_string());
        format!(
            "--- umicraft ui demo ---\n\
             font source       : {source}\n\
             viewport          : {}x{}\n\
             text              : {}\n\
             caret             : {}\n\
             button clicks     : {}\n\
             list selection    : {selection}\n\
             list scroll offset: {:.1}\n",
            self.viewport.0,
            self.viewport.1,
            self.text,
            self.caret,
            self.button_clicks,
            self.scroll_offset,
        )
    }
}

/// The report and the final draw list of one scripted run.
#[derive(Debug, Clone)]
pub struct DemoRun {
    /// The state the screen reached.
    pub report: DemoReport,
    /// The draw list of one final frame with the post-script state.
    pub draw_list: DrawList,
}

/// Runs the fixed acceptance sequence against the real debug screen.
///
/// The function is pure: the same viewport and atlas produce the same report
/// and draw list on every run, with no filesystem, network, GPU or clock
/// access. Widget rectangles are read back from [`DebugScreen`] after the
/// first frame, so the script does not duplicate the screen's layout.
#[must_use]
pub fn run_script(width: u32, height: u32, atlas: &FontAtlas) -> DemoRun {
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let mut ui = Ui::new(viewport);
    let mut screen = DebugScreen::new();

    // The first frame lays the screen out and exposes the widget rectangles.
    settle(&mut ui, &mut screen, atlas, UiInput::default());

    let field = screen.field_rect();
    settle(&mut ui, &mut screen, atlas, click(centre(field)));

    let mut typed = UiInput::default();
    typed.events.push(InputEvent::Text(TYPED.to_string()));
    settle(&mut ui, &mut screen, atlas, typed);

    settle(
        &mut ui,
        &mut screen,
        atlas,
        key(InputEvent::KeyPressed(umicraft_ui::Key::Backspace)),
    );
    settle(
        &mut ui,
        &mut screen,
        atlas,
        key(InputEvent::KeyPressed(umicraft_ui::Key::ArrowLeft)),
    );

    let button = screen.button_rect();
    settle(&mut ui, &mut screen, atlas, click(centre(button)));

    let list = screen.list_rect();
    let row = [
        list.x + 12.0,
        list.y + SELECTED_ROW as f32 * ROW_HEIGHT + ROW_HEIGHT * 0.5,
    ];
    settle(&mut ui, &mut screen, atlas, click(row));

    let mut wheel = UiInput::default();
    wheel
        .events
        .push(InputEvent::PointerMoved { position: row });
    wheel.events.push(InputEvent::Wheel {
        delta: SCROLL_LINES,
    });
    settle(&mut ui, &mut screen, atlas, wheel);

    // One last frame with no input draws the post-script state without
    // changing it, so the screenshot matches the report exactly.
    let viewport = ui.viewport();
    ui.begin_frame(viewport, UiInput::default());
    screen.show(&mut ui, atlas);
    let output = ui.end_frame();

    DemoRun {
        report: DemoReport {
            viewport: (width, height),
            text: screen.text().to_string(),
            caret: screen.caret(),
            button_clicks: screen.click_count(),
            selection: screen.selection(),
            scroll_offset: screen.scroll_offset(),
        },
        draw_list: output.draw_list,
    }
}

/// Runs one frame and recycles its allocation.
fn settle(ui: &mut Ui, screen: &mut DebugScreen, atlas: &FontAtlas, input: UiInput) {
    let viewport = ui.viewport();
    ui.begin_frame(viewport, input);
    screen.show(ui, atlas);
    let output = ui.end_frame();
    ui.recycle_output(output);
}

/// A complete primary click at `position`.
fn click(position: [f32; 2]) -> UiInput {
    UiInput::new(vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
        InputEvent::PointerReleased {
            position,
            button: PointerButton::Left,
        },
    ])
}

/// One key event.
fn key(event: InputEvent) -> UiInput {
    UiInput::new(vec![event])
}

/// The centre of `rect`.
fn centre(rect: Rect) -> [f32; 2] {
    [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
}

/// Runs the scripted demo and prints the report; writes the screenshot when
/// one was requested.
///
/// # Errors
/// Offscreen renderer creation, font-atlas upload, readback, or PNG output
/// failure when `--screenshot` is present.
pub fn run(args: &Args) -> Result<(), ClientError> {
    let font = ui_font::load();
    let Some(path) = args.screenshot.as_deref() else {
        let demo = run_script(args.width, args.height, &font.atlas);
        print!("{}", demo.report.render(font.source));
        return Ok(());
    };

    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let installed = ui_font::install(&mut renderer, &font.atlas)?;
    let source = if installed.used_fallback {
        FontSource::Fallback
    } else {
        font.source
    };
    let demo = run_script(args.width, args.height, &installed.atlas);
    print!("{}", demo.report.render(source));

    let camera = Camera::default();
    let overlay = Overlay::new(args.width, args.height);
    renderer.render_with_ui(&camera, &overlay, &demo.draw_list, &mut NoIcons)?;
    let image = renderer.read_pixels()?;
    write_png(path, &image.rgba, image.width, image.height)?;
    println!(
        "screenshot written: {} ({}x{})",
        path.display(),
        image.width,
        image.height
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scripted_sequence_reaches_the_expected_state() {
        let demo = run_script(1280, 720, &FontAtlas::fallback());
        assert_eq!(demo.report.viewport, (1280, 720));
        assert_eq!(demo.report.text, "HELL");
        assert_eq!(demo.report.caret, 3);
        assert_eq!(demo.report.button_clicks, 1);
        assert_eq!(demo.report.selection, Some(SELECTED_ROW));
        assert_eq!(demo.report.scroll_offset, SCROLL_LINES * ROW_HEIGHT);
        assert!(
            demo.report.scroll_offset > 0.0,
            "the 32-row list must scroll at 720p"
        );
        assert!(!demo.draw_list.is_empty(), "the final frame must draw");
    }

    #[test]
    fn the_report_text_is_stable() {
        let demo = run_script(1280, 720, &FontAtlas::fallback());
        let expected = "\
--- umicraft ui demo ---
font source       : fallback
viewport          : 1280x720
text              : HELL
caret             : 3
button clicks     : 1
list selection    : 2
list scroll offset: 72.0
";
        assert_eq!(demo.report.render(FontSource::Fallback), expected);
    }

    #[test]
    fn the_script_is_independent_of_the_atlas_metrics() {
        let fallback = run_script(1280, 720, &FontAtlas::fallback());
        let ui_fallback = umicraft_render::fallback_atlas();
        let other = run_script(1280, 720, &ui_fallback);
        assert_eq!(fallback.report, other.report);
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn the_ui_demo_screenshot_writes_a_png() {
        let path =
            std::env::temp_dir().join(format!("umicraft-ui-demo-{}.png", std::process::id()));
        let mut renderer = Renderer::new_offscreen(320, 180).expect("offscreen renderer");
        let atlas = FontAtlas::fallback();
        let installed = ui_font::install(&mut renderer, &atlas).expect("font atlas upload");
        let demo = run_script(320, 180, &installed.atlas);
        let camera = Camera::default();
        let overlay = Overlay::new(320, 180);
        renderer
            .render_with_ui(&camera, &overlay, &demo.draw_list, &mut NoIcons)
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        write_png(&path, &image.rgba, image.width, image.height).expect("png");

        let bytes = std::fs::read(&path).expect("screenshot exists");
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        std::fs::remove_file(&path).ok();
    }
}
