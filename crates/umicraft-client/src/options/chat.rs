// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat options: which messages arrive and how they are drawn.

use serde::{Deserialize, Serialize};
use umicraft_render::HudState;

/// Which chat messages the server sends (`EnumChatVisibility`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatVisibility {
    /// Every message.
    #[default]
    Full,
    /// Only the replies to the player's commands.
    Commands,
    /// No message.
    Hidden,
}

impl ChatVisibility {
    /// The byte of the `Client Settings` packet.
    #[must_use]
    pub const fn wire(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Commands => 1,
            Self::Hidden => 2,
        }
    }

    /// The next value when the player clicks the button.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Full => Self::Commands,
            Self::Commands => Self::Hidden,
            Self::Hidden => Self::Full,
        }
    }
}

/// The chat options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatOptions {
    /// Which messages the server sends.
    pub visibility: ChatVisibility,
    /// Whether the colours and styles of the chat are shown.
    pub colors: bool,
    /// `chatLinks`: whether links in the chat are recognised.
    pub links: bool,
    /// `chatLinksPrompt`: whether a click on a link asks first.
    pub links_prompt: bool,
    /// `chatOpacity`, `0.0..=1.0`.
    pub opacity: f32,
    /// `chatScale`, `0.0..=1.0`.
    pub scale: f32,
    /// `chatHeightFocused`, `0.0..=1.0`.
    pub height_focused: f32,
    /// `chatHeightUnfocused`, `0.0..=1.0`.
    pub height_unfocused: f32,
    /// `chatWidth`, `0.0..=1.0`.
    pub width: f32,
    /// `reducedDebugInfo`.
    pub reduced_debug: bool,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            visibility: ChatVisibility::Full,
            colors: true,
            links: true,
            links_prompt: false,
            opacity: 1.0,
            scale: 1.0,
            height_focused: 1.0,
            height_unfocused: 0.444_444_45,
            width: 1.0,
            reduced_debug: false,
        }
    }
}

impl ChatOptions {
    /// Applies the options to the chat of a HUD snapshot: hidden chat shows
    /// no message, and with the colours off every message is plain white
    /// text (`GameSettings.chatColours`).
    pub fn apply(&self, hud: &mut HudState) {
        hud.chat_style = umicraft_render::ChatStyle {
            opacity: self.opacity,
            scale: self.scale,
            width: self.width,
            height_focused: self.height_focused,
            height_unfocused: self.height_unfocused,
        };
        if self.visibility == ChatVisibility::Hidden {
            hud.chat.clear();
        }
        if !self.colors {
            for run in hud.chat.iter_mut().flat_map(|line| line.runs.iter_mut()) {
                run.color = [255, 255, 255];
                run.bold = false;
                run.italic = false;
                run.underlined = false;
                run.strikethrough = false;
                run.obfuscated = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_visibility_cycles_and_maps_to_the_wire_byte() {
        let mut visibility = ChatVisibility::Full;
        let mut wires = Vec::new();
        for _ in 0..4 {
            wires.push(visibility.wire());
            visibility = visibility.next();
        }
        assert_eq!(wires, [0, 1, 2, 0]);
    }

    fn hud_with_a_red_bold_message() -> HudState {
        use std::time::Instant;
        use umicraft_render::{ChatLineView, ChatRunView};
        HudState {
            chat: vec![ChatLineView {
                runs: vec![ChatRunView {
                    text: "hi".to_string(),
                    color: [255, 85, 85],
                    bold: true,
                    italic: true,
                    underlined: true,
                    strikethrough: true,
                    obfuscated: true,
                    ..ChatRunView::default()
                }],
                received: Instant::now(),
            }],
            ..HudState::default()
        }
    }

    #[test]
    fn hidden_chat_shows_no_message() {
        let mut hud = hud_with_a_red_bold_message();
        let options = ChatOptions {
            visibility: ChatVisibility::Hidden,
            ..ChatOptions::default()
        };
        options.apply(&mut hud);
        assert!(hud.chat.is_empty());
    }

    #[test]
    fn colours_off_makes_every_message_plain_white() {
        let mut hud = hud_with_a_red_bold_message();
        let options = ChatOptions {
            colors: false,
            ..ChatOptions::default()
        };
        options.apply(&mut hud);
        let run = &hud.chat[0].runs[0];
        assert_eq!(run.color, [255, 255, 255]);
        assert!(!run.bold && !run.italic && !run.underlined);
        assert!(!run.strikethrough && !run.obfuscated);
        assert_eq!(run.text, "hi", "the text itself is kept");
    }

    #[test]
    fn the_default_chat_options_change_nothing() {
        let mut hud = hud_with_a_red_bold_message();
        let before = hud.clone();
        ChatOptions::default().apply(&mut hud);
        assert_eq!(hud, before);
    }
}
