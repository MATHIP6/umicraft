// SPDX-License-Identifier: MIT OR Apache-2.0

//! The boss bar at the top of the screen (`GuiIngame.renderBossHealth`): the
//! 182x5 empty bar of `icons.png` at `(width / 2 - 91, 12)`, the full bar
//! over it up to `(int)(scale * 183)` pixels, and the name with a shadow ten
//! units above.

use umicraft_ui::{DrawList, FontAtlas, Rect, SpriteSheet};

use super::runs::{draw_runs, width};
use crate::hud::{BossView, HudState};

/// The width of the bar texture.
const BAR_WIDTH: i32 = 182;
/// The top of the bar.
const BAR_Y: f32 = 12.0;

/// The pixels of the full bar for a health fraction: `(int)(scale * 183)`,
/// limited to the texture.
pub(super) fn filled_pixels(health: f32) -> u32 {
    ((health * (BAR_WIDTH + 1) as f32) as i32).clamp(0, BAR_WIDTH) as u32
}

/// Draws the boss bar and its name.
pub(super) fn draw_boss_bar(
    list: &mut DrawList,
    state: &HudState,
    boss: &BossView,
    (gw, _): (f32, f32),
    sf: f32,
    (font, icons): (&FontAtlas, &SpriteSheet),
) {
    let centre = (gw as i32 / 2) as f32;
    let left = centre - (BAR_WIDTH / 2) as f32;
    let bar = Rect::new(left * sf, BAR_Y * sf, BAR_WIDTH as f32 * sf, 5.0 * sf);
    list.image(icons.sub(0, 74, BAR_WIDTH as u32, 5), bar, [1.0; 4]);
    let fill = filled_pixels(boss.health);
    if fill > 0 {
        let rect = Rect::new(left * sf, BAR_Y * sf, fill as f32 * sf, 5.0 * sf);
        list.image(icons.sub(0, 79, fill, 5), rect, [1.0; 4]);
    }
    let name_x = centre - (width(font, &boss.name) as i32 / 2) as f32;
    let at = [name_x * sf, (BAR_Y - 10.0) * sf];
    draw_runs(list, font, state, at, &boss.name, sf, 1.0, true);
}

#[cfg(test)]
mod tests {
    use super::super::runs::run;
    use super::*;
    use umicraft_ui::DrawCommand;

    #[test]
    fn the_fill_is_the_health_times_183_in_whole_pixels() {
        assert_eq!(filled_pixels(1.0), 182, "limited to the texture");
        assert_eq!(filled_pixels(0.5), 91);
        assert_eq!(filled_pixels(0.0), 0);
        assert_eq!(filled_pixels(-0.3), 0);
        assert_eq!(filled_pixels(2.0), 182);
    }

    #[test]
    fn the_bar_is_centred_with_the_name_ten_units_above_it() {
        let mut list = DrawList::new();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let boss = BossView {
            name: vec![run("Wither")],
            health: 0.5,
        };
        let font = FontAtlas::fallback();
        draw_boss_bar(
            &mut list,
            &HudState::default(),
            &boss,
            (320.0, 240.0),
            1.0,
            (&font, &sheet),
        );
        let images: Vec<Rect> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Image { dst, .. } => Some(*dst),
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 2);
        assert_eq!(
            (images[0].x, images[0].y, images[0].width),
            (69.0, 12.0, 182.0)
        );
        assert_eq!(images[1].width, 91.0);
        let name_y = list.commands().iter().find_map(|command| match command {
            DrawCommand::Text { position, .. } => Some(position[1]),
            _ => None,
        });
        assert_eq!(name_y, Some(2.0 + 1.0), "the shadow is drawn first");
    }
}
