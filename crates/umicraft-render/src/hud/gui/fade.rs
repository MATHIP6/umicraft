// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fade of a chat line (MCP `GuiNewChat.drawChat`, `getLineBrightness`).

use std::time::Instant;

/// A line stays fully visible for this many ticks before it fades.
const VISIBLE_TICKS: f32 = 200.0;
/// The milliseconds of one game tick.
const TICK_MS: u128 = 50;

/// The opacity of a chat line `age_ticks` old: opaque while the chat is open,
/// otherwise `clamp((1 - age / 200) * 10, 0, 1)` squared.
#[must_use]
pub(super) fn line_alpha(age_ticks: f32, open: bool) -> f32 {
    if open {
        return 1.0;
    }
    let fade = ((1.0 - age_ticks / VISIBLE_TICKS) * 10.0).clamp(0.0, 1.0);
    fade * fade
}

/// The game ticks between `received` and `now`; a missing `now` freezes the
/// fade at age zero.
pub(super) fn age_ticks(received: Instant, now: Option<Instant>) -> f32 {
    now.map_or(0.0, |now| {
        (now.saturating_duration_since(received).as_millis() / TICK_MS) as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_line_fades_after_two_hundred_ticks_and_stays_opaque_when_open() {
        assert_eq!(line_alpha(0.0, false), 1.0);
        assert_eq!(line_alpha(180.0, false), 1.0);
        let fading = line_alpha(190.0, false);
        assert!(fading > 0.0 && fading < 1.0, "fading was {fading}");
        assert_eq!(line_alpha(200.0, false), 0.0);
        assert_eq!(line_alpha(10_000.0, false), 0.0);
        assert_eq!(line_alpha(10_000.0, true), 1.0);
    }

    #[test]
    fn the_fade_is_quadratic() {
        // 195 ticks: (1 - 0.975) * 10 = 0.25, squared 0.0625.
        assert!((line_alpha(195.0, false) - 0.0625).abs() < 1e-4);
    }

    #[test]
    fn the_age_counts_fifty_millisecond_ticks() {
        let received = Instant::now();
        let now = received + Duration::from_millis(260);
        assert_eq!(age_ticks(received, Some(now)), 5.0);
        assert_eq!(age_ticks(received, None), 0.0);
    }
}
