// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title, the subtitle and the action bar as the renderer draws them.
//!
//! The timers live in [`crate::title`]; this module turns them into styled
//! text and an opacity. Sources: MCP `NetHandlerPlayClient.handleTitle` (the
//! title and subtitle keep their styles: `getFormattedText`),
//! `handleChat` type 2 and `GuiIngame.setRecordPlaying(IChatComponent)` (the
//! action bar drops the JSON styles: `getUnformattedText`).

use crate::chat::{local, parse, unformatted_text, LangTable, Run};
use crate::title::{ActionBar, TitleState};

/// The title and subtitle to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleFrame {
    /// The title; empty when only a subtitle is displayed.
    pub title: Vec<Run>,
    /// The subtitle; empty when there is none.
    pub subtitle: Vec<Run>,
    /// The opacity in `0.0..=1.0`, a multiple of 1/255.
    pub alpha: f32,
}

/// The action bar message to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionBarFrame {
    /// The message with only the styles written as `§` codes in its text.
    pub text: Vec<Run>,
    /// The opacity in `0.0..=1.0`.
    pub alpha: f32,
}

/// The title to draw at `partial_tick`, or `None` while it is hidden or too
/// faint to see (`i2 > 8` in `GuiIngame.renderGameOverlay`).
#[must_use]
pub fn title_frame(
    state: &TitleState,
    translations: &LangTable,
    partial_tick: f32,
) -> Option<TitleFrame> {
    let alpha = state.alpha(partial_tick);
    if alpha <= 0.0 {
        return None;
    }
    let runs = |json: Option<&str>| {
        json.map(|json| parse(json, translations))
            .unwrap_or_default()
    };
    Some(TitleFrame {
        title: runs(state.title()),
        subtitle: runs(state.subtitle()),
        alpha,
    })
}

/// The action bar to draw at `partial_tick`, or `None` while it is hidden.
#[must_use]
pub fn action_bar_frame(
    bar: &ActionBar,
    translations: &LangTable,
    partial_tick: f32,
) -> Option<ActionBarFrame> {
    let json = bar.message()?;
    let alpha = bar.alpha(partial_tick);
    (alpha > 0.0).then(|| ActionBarFrame {
        text: local(&unformatted_text(json, translations)),
        alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::plain;
    use umicraft_protocol::packets::play::clientbound::Title;

    fn tr() -> LangTable {
        LangTable::builtin()
    }

    #[test]
    fn a_title_and_subtitle_keep_their_styles() {
        let mut state = TitleState::new();
        state.apply(&Title::Subtitle {
            json: r#"{"text":"sub","color":"gold"}"#.to_owned(),
        });
        state.apply(&Title::Title {
            json: r#"{"text":"Round 1","bold":true}"#.to_owned(),
        });
        for _ in 0..10 {
            state.tick();
        }
        let frame = title_frame(&state, &tr(), 0.0).expect("displayed");
        assert_eq!(plain(&frame.title), "Round 1");
        assert!(frame.title[0].style.bold);
        assert_eq!(frame.subtitle[0].style.color.map(|c| c.index()), Some(6));
        assert_eq!(frame.alpha, 1.0);
    }

    #[test]
    fn a_title_that_has_not_faded_in_yet_is_hidden() {
        let mut state = TitleState::new();
        state.apply(&Title::Title {
            json: r#"{"text":"x"}"#.to_owned(),
        });
        assert_eq!(
            title_frame(&state, &tr(), 0.0),
            None,
            "alpha is 0 at tick 0"
        );
    }

    #[test]
    fn a_subtitle_without_a_title_has_empty_title_runs() {
        let mut state = TitleState::new();
        state.apply(&Title::Subtitle {
            json: r#"{"text":"s"}"#.to_owned(),
        });
        state.apply(&Title::Title {
            json: r#"{"text":""}"#.to_owned(),
        });
        for _ in 0..10 {
            state.tick();
        }
        let frame = title_frame(&state, &tr(), 0.0).expect("displayed");
        assert!(frame.title.is_empty());
        assert_eq!(plain(&frame.subtitle), "s");
    }

    #[test]
    fn the_action_bar_drops_json_styles_but_obeys_section_codes() {
        let mut bar = ActionBar::new();
        bar.show(r#"{"text":"§aGo","color":"red","bold":true}"#);
        let frame = action_bar_frame(&bar, &tr(), 0.0).expect("displayed");
        assert_eq!(plain(&frame.text), "Go");
        assert_eq!(frame.text[0].style.color.map(|c| c.index()), Some(10));
        assert!(!frame.text[0].style.bold);
        assert_eq!(frame.alpha, 1.0);
    }

    #[test]
    fn the_action_bar_is_gone_after_sixty_ticks() {
        let mut bar = ActionBar::new();
        bar.show(r#"{"text":"x"}"#);
        for _ in 0..60 {
            bar.tick();
        }
        assert_eq!(action_bar_frame(&bar, &tr(), 0.0), None);
    }
}
