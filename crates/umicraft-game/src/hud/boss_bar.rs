// SPDX-License-Identifier: MIT OR Apache-2.0

//! The boss bar at the top of the screen.
//!
//! 1.8 has no boss bar packet. A Wither or an Ender Dragon that is rendered
//! calls `BossStatus.setBossStatus`, which stores its health fraction
//! (`getHealth() / getMaxHealth()`), its formatted name and a countdown of 100.
//! `GuiIngame.renderBossHealth` draws the bar while the countdown is positive
//! and decrements it. Because the entity refreshes the status every frame it
//! is rendered, the bar disappears about 100 updates after the boss does.
//!
//! Deviation: the reference decrements once per rendered frame, so the delay
//! depends on the frame rate (1.7 s at 60 fps). Here it counts game ticks, 5 s
//! at 20 Hz, so it does not.

use crate::chat_component::{parse_legacy, Run, Translations};
use crate::entity::{BossInfo, DRAGON_NAME_KEY, WITHER_NAME_KEY};

/// The countdown set by `BossStatus.setBossStatus`.
pub const STATUS_BAR_TIME: i32 = 100;

/// A boss bar ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct BossBarFrame {
    /// The formatted boss name.
    pub name: Vec<Run>,
    /// The health as a fraction of the maximum; the renderer scales it to the
    /// 182 pixel bar with `(int)(scale * 183)`.
    pub health_scale: f32,
}

/// The state of `BossStatus`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BossBar {
    name: Option<String>,
    health_scale: f32,
    status_bar_time: i32,
}

impl BossBar {
    /// Creates a boss bar that shows nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the boss that is in the world this tick, if any
    /// (`BossStatus.setBossStatus`). The name is translated here so that a
    /// language change shows at the next refresh.
    pub fn observe(&mut self, boss: Option<&BossInfo>, translations: &Translations) {
        let Some(boss) = boss else { return };
        self.name = Some(boss_name(boss, translations));
        self.health_scale = boss.health_fraction;
        self.status_bar_time = STATUS_BAR_TIME;
    }

    /// Counts the display time down by one game tick.
    pub fn tick(&mut self) {
        if self.status_bar_time > 0 {
            self.status_bar_time -= 1;
        }
    }

    /// Whether a bar is displayed.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.name.is_some() && self.status_bar_time > 0
    }

    /// The bar to draw, while the countdown runs.
    #[must_use]
    pub fn frame(&self) -> Option<BossBarFrame> {
        let name = self.name.as_deref().filter(|_| self.is_visible())?;
        Some(BossBarFrame {
            name: parse_legacy(name),
            health_scale: self.health_scale,
        })
    }
}

/// The English name used before a language file is installed.
fn fallback_name(key: &str) -> &str {
    match key {
        DRAGON_NAME_KEY => "Ender Dragon",
        WITHER_NAME_KEY => "Wither",
        other => other,
    }
}

/// The name of a boss: its custom name, else the translated type name
/// (`Entity.getName`).
fn boss_name(boss: &BossInfo, translations: &Translations) -> String {
    boss.custom_name.clone().unwrap_or_else(|| {
        translations
            .get(boss.name_key)
            .unwrap_or_else(|| fallback_name(boss.name_key))
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_component::plain;

    fn wither(fraction: f32) -> BossInfo {
        BossInfo {
            name_key: WITHER_NAME_KEY,
            custom_name: None,
            health_fraction: fraction,
        }
    }

    fn shown_name(bar: &BossBar) -> Option<String> {
        bar.frame().map(|frame| plain(&frame.name))
    }

    #[test]
    fn nothing_is_shown_before_a_boss_is_seen() {
        let bar = BossBar::new();
        assert!(!bar.is_visible());
        assert_eq!(bar.frame(), None);
    }

    #[test]
    fn the_bar_shows_the_translated_name_and_the_health_fraction() {
        let mut bar = BossBar::new();
        let mut table = Translations::default();
        table.extend(Translations::from_lang(
            "entity.WitherBoss.name=Wither (lang)",
        ));
        bar.observe(Some(&wither(0.5)), &table);
        let frame = bar.frame().expect("visible");
        assert_eq!(plain(&frame.name), "Wither (lang)");
        assert_eq!(frame.health_scale, 0.5);
    }

    #[test]
    fn a_missing_translation_falls_back_to_english() {
        let mut bar = BossBar::new();
        let dragon = BossInfo {
            name_key: DRAGON_NAME_KEY,
            custom_name: None,
            health_fraction: 1.0,
        };
        bar.observe(Some(&dragon), &Translations::default());
        assert_eq!(shown_name(&bar).as_deref(), Some("Ender Dragon"));
    }

    #[test]
    fn a_custom_name_replaces_the_translated_one() {
        let mut bar = BossBar::new();
        let named = BossInfo {
            custom_name: Some("§cBig Bad".to_owned()),
            ..wither(1.0)
        };
        bar.observe(Some(&named), &Translations::default());
        let frame = bar.frame().expect("visible");
        assert_eq!(plain(&frame.name), "Big Bad");
        assert_eq!(frame.name[0].style.color.map(|c| c.index()), Some(12));
    }

    #[test]
    fn the_bar_lasts_one_hundred_ticks_after_the_boss_was_last_seen() {
        let mut bar = BossBar::new();
        bar.observe(Some(&wither(1.0)), &Translations::default());
        for _ in 0..99 {
            bar.tick();
        }
        assert!(bar.is_visible());
        bar.tick();
        assert!(!bar.is_visible());
        assert_eq!(bar.frame(), None);
    }

    #[test]
    fn seeing_the_boss_again_restarts_the_countdown() {
        let mut bar = BossBar::new();
        bar.observe(Some(&wither(1.0)), &Translations::default());
        for _ in 0..90 {
            bar.tick();
        }
        bar.observe(Some(&wither(0.25)), &Translations::default());
        for _ in 0..90 {
            bar.tick();
        }
        assert_eq!(bar.frame().map(|f| f.health_scale), Some(0.25));
    }

    #[test]
    fn observing_no_boss_keeps_the_countdown_running() {
        let mut bar = BossBar::new();
        bar.observe(Some(&wither(1.0)), &Translations::default());
        bar.observe(None, &Translations::default());
        bar.tick();
        assert!(bar.is_visible());
    }
}
