// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread published for the screens: the HUD, the inventory,
//! the status, the statistics, the sounds and the end of the session.

use std::time::Instant;

use umicraft_game::inventory::WindowView;
use umicraft_game::sound::cue::SoundCue;
use umicraft_render::HudState;
use umicraft_ui::menus::ConnectStage;

use super::WorldView;
use crate::game::{GameStatus, TextScreenRequest};
use crate::pack_offer::ServerPackRequest;
use crate::screens::connection_end::ConnectionEnd;
use crate::screens::creative::view::CreativeView;

impl WorldView {
    /// The newest HUD the game thread published.
    #[must_use]
    pub fn hud(&self) -> &HudState {
        &self.hud
    }

    /// The hand and the crosshair at the newest tick, for the `INPUT` mod event;
    /// `None` when no external mod is loaded or before the first frame.
    #[must_use]
    pub fn mod_hand(&self) -> Option<umicraft_modhost::HandFacts> {
        self.mod_hand
    }

    /// The newest inventory/container state the game thread published.
    #[must_use]
    pub fn window_view(&self) -> &WindowView {
        &self.window_view
    }

    /// The newest creative inventory snapshot the game thread published.
    #[must_use]
    pub fn creative_view(&self) -> Option<&CreativeView> {
        self.creative.as_ref()
    }

    /// The newest session statistics.
    #[must_use]
    pub fn status(&self) -> &GameStatus {
        &self.status
    }

    /// The statistic counters of the last `Statistics` answer, sorted by name.
    #[must_use]
    pub fn stats(&self) -> &[(String, i32)] {
        &self.stats
    }

    /// How many statistics answers arrived, to tell a new one from the last.
    #[must_use]
    pub fn stats_revision(&self) -> u64 {
        self.stats_revision
    }

    /// Whether a statistics request has no answer yet.
    #[must_use]
    pub fn stats_loading(&self) -> bool {
        self.stats_loading
    }

    /// The achievement the server unlocked last and when, for its toast.
    #[must_use]
    pub fn toast(&self) -> Option<(&'static str, Instant)> {
        self.toast
    }

    /// Notes that a statistics request was sent: the screen shows the
    /// download text until the answer arrives.
    pub fn expect_stats(&mut self) {
        self.stats_loading = true;
    }

    /// Whether the server placed the player: the world is ready to be shown.
    #[must_use]
    pub fn placed(&self) -> bool {
        self.status.placed
    }

    /// Takes the sound cues of the ticks received since the last call, oldest
    /// first. The window calls it once per frame and gives them to the audio
    /// thread.
    pub fn take_sounds(&mut self) -> Vec<SoundCue> {
        std::mem::take(&mut self.sounds)
    }

    /// Why the session ended, once it has.
    #[must_use]
    pub fn ended(&self) -> Option<&ConnectionEnd> {
        self.ended.as_ref()
    }

    /// How far the connection attempt has come.
    #[must_use]
    pub fn connect_stage(&self) -> ConnectStage {
        self.connect_stage
    }

    /// Monotonic revision of [`WorldView::hud`], incremented on every HUD event.
    #[must_use]
    pub fn hud_revision(&self) -> u64 {
        self.hud_revision
    }

    /// Takes the text screen the game thread asked for, if any.
    pub fn take_text_request(&mut self) -> Option<TextScreenRequest> {
        self.text_request.take()
    }

    /// Takes the resource pack offers received since the last call.
    pub fn take_pack_requests(&mut self) -> Vec<ServerPackRequest> {
        std::mem::take(&mut self.pack_requests)
    }
}
