// SPDX-License-Identifier: MIT OR Apache-2.0

//! The HUD draw list, kept while the HUD does not change (perf 2, step B2).
//!
//! Building the HUD (the chat, the sidebar, the tab list, the hotbar) costs 0.1 to
//! 0.5 ms and its text changes at the 20 Hz of the game, not at the 700 Hz of the
//! frames. The window hands the HUD state of each frame to [`HudCache::draw`]; when it
//! equals the last one the cached commands are laid into the frame again. The clock
//! of the state is held still for [`REFRESH`], so the fade of the chat, which the
//! reference counts in ticks, follows at that pace; a change shows at once.

use std::time::{Duration, Instant};

use umicraft_render::{draw_hud_gui, HudSheets, HudState};
use umicraft_ui::{DrawList, FontAtlas};

/// How long the clock of a built HUD stands still: one game tick.
pub const REFRESH: Duration = Duration::from_millis(50);

/// The last built HUD and what it was built from.
#[derive(Debug, Default)]
pub struct HudCache {
    key: Option<(HudState, HudSheets, (u32, u32))>,
    list: DrawList,
}

impl HudCache {
    /// The clock to put in the HUD state of the frame at `now`: the one of the
    /// cached HUD while it is less than [`REFRESH`] old, so that an unchanged HUD
    /// compares equal, and `now` after that.
    #[must_use]
    pub fn clock(&self, now: Instant) -> Instant {
        match &self.key {
            Some((hud, _, _)) => hud
                .now
                .filter(|built| now.saturating_duration_since(*built) < REFRESH)
                .unwrap_or(now),
            None => now,
        }
    }

    /// Lays the HUD of `hud` into `target`: the cached commands when nothing
    /// changed, else a fresh build that becomes the cache. Returns whether the
    /// cache was used.
    pub fn draw(
        &mut self,
        target: &mut DrawList,
        hud: &HudState,
        viewport: (u32, u32),
        (font, sheets): (&FontAtlas, &HudSheets),
    ) -> bool {
        let unchanged = self.key.as_ref().is_some_and(|(key, key_sheets, size)| {
            *size == viewport && key_sheets == sheets && key == hud
        });
        if !unchanged {
            self.list.clear();
            draw_hud_gui(&mut self.list, hud, viewport, font, sheets);
            self.key = Some((hud.clone(), sheets.clone(), viewport));
        }
        target.extend_from(&self.list);
        unchanged
    }

    /// Forgets the cache: the next frame builds the HUD (the fonts or the sprite
    /// sheets changed).
    pub fn invalidate(&mut self) {
        self.key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(list: &DrawList) -> usize {
        list.commands().len()
    }

    #[test]
    fn an_unchanged_hud_is_laid_in_again_and_a_changed_one_is_built() {
        let font = FontAtlas::fallback();
        let sheets = HudSheets::default();
        let start = Instant::now();
        let mut cache = HudCache::default();
        let mut hud = HudState {
            now: Some(start),
            crosshair: true,
            ..HudState::default()
        };
        let mut first = DrawList::new();
        assert!(!cache.draw(&mut first, &hud, (800, 600), (&font, &sheets)));
        let mut second = DrawList::new();
        assert!(cache.draw(&mut second, &hud, (800, 600), (&font, &sheets)));
        assert_eq!(first, second, "the cached frame is the built one");
        hud.chat.push(umicraft_render::ChatLineView {
            runs: vec![umicraft_render::ChatRunView {
                text: "hello".to_owned(),
                color: [255; 3],
                bold: false,
                italic: false,
                underlined: false,
                strikethrough: false,
                obfuscated: false,
                click: None,
                hover: None,
                insertion: None,
            }],
            received: start,
        });
        let mut third = DrawList::new();
        assert!(!cache.draw(&mut third, &hud, (800, 600), (&font, &sheets)));
        assert_ne!(count(&third), count(&second), "a change shows at once");
    }

    #[test]
    fn a_new_window_size_or_an_invalidation_builds_again() {
        let font = FontAtlas::fallback();
        let sheets = HudSheets::default();
        let hud = HudState {
            now: Some(Instant::now()),
            ..HudState::default()
        };
        let mut cache = HudCache::default();
        let mut list = DrawList::new();
        cache.draw(&mut list, &hud, (800, 600), (&font, &sheets));
        assert!(!cache.draw(&mut list, &hud, (1024, 768), (&font, &sheets)));
        assert!(cache.draw(&mut list, &hud, (1024, 768), (&font, &sheets)));
        cache.invalidate();
        assert!(!cache.draw(&mut list, &hud, (1024, 768), (&font, &sheets)));
    }

    #[test]
    fn the_clock_stands_still_for_a_tick_then_moves_on() {
        let font = FontAtlas::fallback();
        let sheets = HudSheets::default();
        let start = Instant::now();
        let mut cache = HudCache::default();
        assert_eq!(cache.clock(start), start, "nothing cached yet");
        let hud = HudState {
            now: Some(start),
            ..HudState::default()
        };
        cache.draw(&mut DrawList::new(), &hud, (800, 600), (&font, &sheets));
        assert_eq!(cache.clock(start + Duration::from_millis(10)), start);
        let later = start + REFRESH + Duration::from_millis(1);
        assert_eq!(cache.clock(later), later);
    }
}
