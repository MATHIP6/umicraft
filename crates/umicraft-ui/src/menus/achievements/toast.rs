// SPDX-License-Identifier: MIT OR Apache-2.0

//! The "Achievement get!" toast (MCP `GuiAchievement`): a 160 by 32 plaque
//! that slides down from the top-right corner, stays and slides back up
//! within three seconds.
//!
//! The slide is `y = -36 * (1 - 4 * t')^4` with `t'` the progress folded at
//! the middle (`updateAchievementWindow`), so the plaque spends most of the
//! time fully shown and moves quickly at both ends.

use std::time::Duration;

use super::model::AchievementArt;
use crate::context::Ui;
use crate::draw::{Color, TextStyle};
use crate::menus::common::Gui;
use crate::text::FontAtlas;
use crate::widgets::{draw_item, SlotItem};

/// How long the toast lasts.
pub const TOAST_DURATION: Duration = Duration::from_secs(3);
/// The plaque size in GUI units.
const SIZE: [f32; 2] = [160.0, 32.0];
/// The title colour (`-256`, yellow).
const TITLE: Color = [1.0, 1.0, 0.0, 1.0];
/// How far the plaque hides above the screen edge at the ends.
const HIDDEN: f32 = 36.0;

/// What the toast shows.
#[derive(Debug, Clone)]
pub struct Toast<'a> {
    /// The line above the name (`achievement.get`).
    pub title: &'a str,
    /// The translated achievement name.
    pub name: &'a str,
    /// The achievement icon.
    pub icon: SlotItem,
    /// The time since the toast started.
    pub elapsed: Duration,
}

/// The vertical offset of the plaque in GUI units, `None` once the toast is
/// over. The plaque is at `y = offset` and `x = width - 160`.
#[must_use]
pub fn toast_offset(elapsed: Duration) -> Option<f32> {
    let progress = elapsed.as_secs_f32() / TOAST_DURATION.as_secs_f32();
    if !(0.0..=1.0).contains(&progress) {
        return None;
    }
    let mut folded = progress * 2.0;
    if folded > 1.0 {
        folded = 2.0 - folded;
    }
    let slide = (1.0 - folded * 4.0).max(0.0);
    let slide = slide * slide;
    Some(-((slide * slide * HIDDEN).trunc()))
}

/// Draws the toast over whatever the screen shows; nothing without the
/// achievement texture or after [`TOAST_DURATION`].
pub fn draw_toast(ui: &mut Ui, atlas: &FontAtlas, toast: &Toast<'_>, art: Option<&AchievementArt>) {
    let (Some(offset), Some(art)) = (toast_offset(toast.elapsed), art) else {
        return;
    };
    let gui = Gui::new(ui.viewport());
    let x = gui.width - SIZE[0];
    let plaque = gui.rect(x, offset, SIZE[0], SIZE[1]);
    ui.draw()
        .image(art.background.sub(96, 202, 160, 32), plaque, [1.0; 4]);
    let text = |ui: &mut Ui, text: &str, dy: f32, color: Color| {
        let at = gui.rect(x + 30.0, offset + dy, 0.0, 0.0);
        ui.draw()
            .text([at.x, at.y], text, TextStyle::new(gui.scale, color));
    };
    text(ui, toast.title, 7.0, TITLE);
    text(ui, toast.name, 18.0, [1.0; 4]);
    let icon = gui.rect(x + 8.0, offset + 8.0, 16.0, 16.0);
    draw_item(ui.draw(), atlas, icon, &toast.icon);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: f32) -> Option<f32> {
        toast_offset(Duration::from_secs_f32(seconds))
    }

    #[test]
    fn the_plaque_starts_hidden_stays_then_hides_again() {
        assert_eq!(at(0.0), Some(-36.0));
        assert_eq!(at(1.5), Some(0.0));
        assert_eq!(at(3.0), Some(-36.0));
        assert_eq!(at(3.1), None);
    }

    #[test]
    fn it_slides_in_faster_than_linear() {
        // A quarter of the way down after 0.25 of the first half.
        let early = at(0.1).unwrap_or(0.0);
        let later = at(0.3).unwrap_or(0.0);
        assert!(early < later && later <= 0.0, "{early} {later}");
        assert!(later > -36.0 / 2.0, "the slide ends before the middle");
    }

    #[test]
    fn the_plaque_is_drawn_at_the_top_right_with_its_texts() {
        use crate::draw::{DrawCommand, SpriteSheet};
        use crate::input::UiInput;
        use crate::menus::common::testing::frame;
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let art = AchievementArt {
            background: sheet,
            tiles: [sheet; 8],
        };
        let toast = Toast {
            title: "Achievement get!",
            name: "Getting Wood",
            icon: SlotItem {
                key: 1,
                count: 1,
                damage: 0,
                max_durability: 0,
                glint: false,
            },
            elapsed: Duration::from_millis(1_500),
        };
        let commands = frame(UiInput::default(), |ui| {
            draw_toast(ui, &FontAtlas::fallback(), &toast, Some(&art));
            ui.draw().commands().to_vec()
        });
        // scale 3, width 427: x = (427 - 160) * 3, fully shown at y = 0
        let plaque = commands.iter().find_map(|c| match c {
            DrawCommand::Image { dst, .. } => Some(*dst),
            _ => None,
        });
        assert_eq!(
            plaque.map(|r| (r.x, r.y, r.width)),
            Some((801.0, 0.0, 480.0))
        );
        let texts: Vec<_> = commands
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["Achievement get!", "Getting Wood"]);
    }

    #[test]
    fn the_slide_is_symmetric() {
        assert_eq!(at(0.4), at(2.6));
    }
}
