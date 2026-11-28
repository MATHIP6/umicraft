// SPDX-License-Identifier: MIT OR Apache-2.0

//! The damage flash of the hearts in the tab list.
//!
//! Source: MCP `GuiPlayerTabOverlay.drawScoreboardValues` and the fields of
//! `NetworkPlayerInfo` it keeps (`lastHealth`, `displayHealth`,
//! `lastHealthTime`, `healthBlinkTime`, `field_178869_m`). When a player's
//! value drops the hearts blink for 20 ticks and the lost ones stay drawn as
//! flashing containers; when it rises they blink for 10 ticks. The state
//! resets when the list has been closed, or after a second without a change.

use std::collections::HashMap;

use crate::player_list::Uuid;

/// What the tab list draws for one player's hearts this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartsFlash {
    /// The value the hearts are measured against (`displayHealth`): the
    /// value before the change, until the second without one is over.
    pub display: i32,
    /// The containers are drawn with the flash frame (`flag`).
    pub blink: bool,
}

/// The per-player fields of `NetworkPlayerInfo`.
#[derive(Debug, Clone, Copy, Default)]
struct PlayerHearts {
    last: i32,
    display: i32,
    last_change_ms: u64,
    blink_until_tick: u64,
    opened: u64,
}

/// The flash state of every player, and when the list was last opened.
#[derive(Debug, Default)]
pub struct TabHearts {
    players: HashMap<Uuid, PlayerHearts>,
    opened_ms: u64,
    shown: bool,
}

impl TabHearts {
    /// Tells the tracker whether the list is drawn this frame
    /// (`updatePlayerList`): a list that was not shown and now is has been
    /// opened, which restarts every player's flash.
    pub fn set_shown(&mut self, shown: bool, now_ms: u64) {
        if shown && !self.shown {
            self.opened_ms = now_ms;
        }
        self.shown = shown;
    }

    /// The flash of `uuid` whose value is `value`, at `now_ms` and game tick
    /// `tick` (`drawScoreboardValues`).
    pub fn flash(&mut self, uuid: Uuid, value: i32, now_ms: u64, tick: u64) -> HeartsFlash {
        let opened_ms = self.opened_ms;
        let player = self.players.entry(uuid).or_default();
        if player.opened == opened_ms {
            if value < player.last {
                player.last_change_ms = now_ms;
                player.blink_until_tick = tick + 20;
            } else if value > player.last {
                player.last_change_ms = now_ms;
                player.blink_until_tick = tick + 10;
            }
        }
        if now_ms.saturating_sub(player.last_change_ms) > 1000 || player.opened != opened_ms {
            player.display = value;
            player.last_change_ms = now_ms;
        }
        player.opened = opened_ms;
        player.last = value;
        let remaining = player.blink_until_tick.saturating_sub(tick);
        HeartsFlash {
            display: player.display,
            blink: remaining > 0 && (remaining / 3) % 2 == 1,
        }
    }

    /// Forgets the players that left (`NetworkPlayerInfo` goes with them).
    pub fn retain(&mut self, mut listed: impl FnMut(&Uuid) -> bool) {
        self.players.retain(|uuid, _| listed(uuid));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: Uuid = 7;

    /// A list opened at `1_000` ms with the player at `value`.
    fn opened(value: i32) -> TabHearts {
        let mut hearts = TabHearts::default();
        hearts.set_shown(true, 1_000);
        // The first frame after an opening takes the value as it is.
        let first = hearts.flash(PLAYER, value, 1_000, 20);
        assert_eq!(
            first,
            HeartsFlash {
                display: value,
                blink: false
            }
        );
        hearts
    }

    #[test]
    fn a_drop_keeps_the_old_value_and_blinks_for_twenty_ticks() {
        let mut hearts = opened(20);
        let flash = hearts.flash(PLAYER, 14, 1_050, 21);
        assert_eq!(flash.display, 20, "the lost hearts stay drawn");
        // 20 ticks remain: 20 / 3 = 6, even, so the flash starts off.
        assert!(!flash.blink);
        // 19 remaining: 19 / 3 = 6; 18 / 3 = 6; 17 / 3 = 5, odd: on.
        assert!(hearts.flash(PLAYER, 14, 1_100, 24).blink);
    }

    #[test]
    fn a_rise_blinks_for_ten_ticks() {
        let mut hearts = opened(10);
        hearts.flash(PLAYER, 14, 1_050, 21);
        // 31 - 28 = 3 remaining: 3 / 3 = 1, odd: on.
        assert!(hearts.flash(PLAYER, 14, 1_100, 28).blink);
        assert!(!hearts.flash(PLAYER, 14, 1_150, 31).blink);
    }

    #[test]
    fn a_second_without_a_change_forgets_the_old_value() {
        let mut hearts = opened(20);
        hearts.flash(PLAYER, 14, 1_050, 21);
        assert_eq!(hearts.flash(PLAYER, 14, 2_100, 62).display, 14);
    }

    #[test]
    fn reopening_the_list_restarts_the_flash_without_blinking() {
        let mut hearts = opened(20);
        hearts.set_shown(false, 1_500);
        hearts.set_shown(true, 3_000);
        let flash = hearts.flash(PLAYER, 6, 3_000, 60);
        assert_eq!(
            flash,
            HeartsFlash {
                display: 6,
                blink: false
            }
        );
    }
}
