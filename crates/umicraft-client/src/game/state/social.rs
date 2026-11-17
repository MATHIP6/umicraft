// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the server shows of the other players and of itself: the tab list,
//! the scoreboard, the title, the action bar and the boss bar.

use umicraft_game::hud::boss_bar::BossBar;
use umicraft_game::player_list::PlayerList;
use umicraft_game::scoreboard::Scoreboard;
use umicraft_game::title::{ActionBar, TitleState};

/// The tab list, the scoreboard, the title, the action bar and the boss bar.
pub(in crate::game) struct Social {
    /// The tab list of the server.
    pub(in crate::game) player_list: PlayerList,
    /// The scoreboard of the current world.
    pub(in crate::game) scoreboard: Scoreboard,
    /// The title and subtitle.
    pub(in crate::game) title: TitleState,
    /// The message above the hotbar.
    pub(in crate::game) action_bar: ActionBar,
    /// The boss bar (`BossStatus`).
    pub(in crate::game) boss_bar: BossBar,
}

impl Default for Social {
    fn default() -> Self {
        Self {
            player_list: PlayerList::new(),
            scoreboard: Scoreboard::new(),
            title: TitleState::new(),
            action_bar: ActionBar::new(),
            boss_bar: BossBar::new(),
        }
    }
}
