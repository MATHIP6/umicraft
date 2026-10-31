// SPDX-License-Identifier: MIT OR Apache-2.0

//! Weather and game mode, as set by `Change Game State` (0x2B).
//!
//! Source: MCP `NetHandlerPlayClient.handleChangeGameState`. Reasons that
//! only play a sound, a particle, open a screen or print a hint (0 bed
//! unusable, 4 credits, 5 demo, 6 arrow hit, 10 elder guardian curse) change
//! no state and are ignored here; the HUD milestone can add them.
//!
//! # No client-side ramp
//!
//! `World.updateWeather` ramps the strengths by 0.01 per tick, but only when
//! `!isRemote`; `WorldClient.updateWeather` is empty. The 1.8 server sends
//! reasons 7 and 8 whenever its own ramp changes a strength, and
//! `World.setRainStrength` sets the previous and the current value alike, so
//! the client shows exactly what the server sent and the partial tick has
//! nothing to interpolate. The client's `WorldInfo` flags (`raining`,
//! `thundering`) are never used for the strength: only reasons 1 and 2 touch
//! `raining`, nothing sets `thundering`, and `isRaining` / `isThundering` are
//! thresholds on the strengths.
//!
//! # Lightning flash
//!
//! `EntityLightningBolt.onUpdate` sets `World.lastLightningBolt` to 2 on
//! every tick it is flashing, and `Minecraft.runTick` decrements it by one
//! before the entities update, so the flash outlives the bolt by two ticks.

use umicraft_protocol::packets::play::clientbound::change_game_state::ChangeGameState;

/// `handleChangeGameState` reasons that change state.
const BEGIN_RAIN: u8 = 1;
const END_RAIN: u8 = 2;
const GAME_MODE: u8 = 3;
const RAIN_STRENGTH: u8 = 7;
const THUNDER_STRENGTH: u8 = 8;

/// Rain, thunder and the game mode.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Weather {
    raining: bool,
    rain_strength: f32,
    thunder_strength: f32,
    lightning_ticks: u8,
    game_mode: Option<u8>,
}

/// Value `EntityLightningBolt.onUpdate` gives `lastLightningBolt` while it flashes.
const FLASH_TICKS: u8 = 2;

/// `World.setRainStrength` clamps to `0..=1`; a NaN from a hostile server
/// becomes 0.
fn unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

impl Weather {
    /// Applies a game state change.
    ///
    /// Reason 1 starts rain and reason 2 stops it; like the reference they
    /// also reset the strength to 0 / 1 (`NetHandlerPlayClient`); the server's
    /// own ramp then arrives as reasons 7 and 8 (see the module notes). The
    /// game mode value is rounded (`floor(v + 0.5)`) and an unknown id means survival
    /// (`GameType.getByID`).
    pub fn apply(&mut self, packet: &ChangeGameState) {
        match packet.reason {
            BEGIN_RAIN => {
                self.raining = true;
                self.rain_strength = 0.0;
            }
            END_RAIN => {
                self.raining = false;
                self.rain_strength = 1.0;
            }
            GAME_MODE => self.game_mode = Some(game_mode_id(packet.value)),
            RAIN_STRENGTH => self.rain_strength = unit(packet.value),
            THUNDER_STRENGTH => self.thunder_strength = unit(packet.value),
            _ => {}
        }
    }

    /// Whether it rains.
    pub fn raining(&self) -> bool {
        self.raining
    }

    /// The rain level, `0..=1`.
    pub fn rain_strength(&self) -> f32 {
        self.rain_strength
    }

    /// The thunder level the server set, `0..=1`, before it is weighted by
    /// the rain.
    pub fn thunder_strength(&self) -> f32 {
        self.thunder_strength
    }

    /// `World.getThunderStrength`: the thunder level weighted by the rain
    /// level, which is what darkens the sky.
    pub fn effective_thunder(&self) -> f32 {
        self.thunder_strength * self.rain_strength
    }

    /// Advances the lightning flash by one game tick
    /// (`Minecraft.runTick`, then `EntityLightningBolt.onUpdate`).
    /// `bolt_flashing` is true while a lightning bolt is alive.
    pub fn tick(&mut self, bolt_flashing: bool) {
        self.lightning_ticks = self.lightning_ticks.saturating_sub(1);
        if bolt_flashing {
            self.lightning_ticks = FLASH_TICKS;
        }
    }

    /// `World.getLastLightningBolt`: the ticks the sky flash has left, `0`
    /// when there is none.
    pub fn lightning_ticks(&self) -> u8 {
        self.lightning_ticks
    }

    /// The game mode set by a state change (0 survival .. 3 spectator);
    /// `None` until the server sent one.
    pub fn game_mode(&self) -> Option<u8> {
        self.game_mode
    }

    /// Resets the weather for a new world; the game mode is kept because
    /// the respawn packet carries it separately.
    pub fn reset_world(&mut self) {
        *self = Self {
            game_mode: self.game_mode,
            ..Self::default()
        };
    }
}

fn game_mode_id(value: f32) -> u8 {
    let id = (value + 0.5).floor();
    if (0.0..=3.0).contains(&id) {
        id as u8
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(reason: u8, value: f32) -> ChangeGameState {
        ChangeGameState { reason, value }
    }

    #[test]
    fn rain_starts_and_stops() {
        let mut weather = Weather::default();
        weather.apply(&change(1, 0.0));
        assert!(weather.raining());
        assert_eq!(weather.rain_strength(), 0.0);
        weather.apply(&change(2, 0.0));
        assert!(!weather.raining());
        assert_eq!(weather.rain_strength(), 1.0);
    }

    #[test]
    fn strengths_are_clamped() {
        let mut weather = Weather::default();
        weather.apply(&change(7, 0.4));
        weather.apply(&change(8, 7.0));
        assert_eq!(weather.rain_strength(), 0.4);
        assert_eq!(weather.thunder_strength(), 1.0);
        weather.apply(&change(7, f32::NAN));
        weather.apply(&change(8, -3.0));
        assert_eq!(weather.rain_strength(), 0.0);
        assert_eq!(weather.thunder_strength(), 0.0);
    }

    #[test]
    fn the_game_mode_is_rounded_and_unknown_means_survival() {
        let mut weather = Weather::default();
        assert_eq!(weather.game_mode(), None);
        weather.apply(&change(3, 1.0));
        assert_eq!(weather.game_mode(), Some(1));
        weather.apply(&change(3, 2.6));
        assert_eq!(weather.game_mode(), Some(3));
        weather.apply(&change(3, 99.0));
        assert_eq!(weather.game_mode(), Some(0));
        weather.apply(&change(3, f32::NAN));
        assert_eq!(weather.game_mode(), Some(0));
    }

    #[test]
    fn visual_reasons_change_nothing() {
        let mut weather = Weather::default();
        for reason in [0, 4, 5, 6, 10, 200] {
            weather.apply(&change(reason, 1.0));
        }
        assert_eq!(weather, Weather::default());
    }

    #[test]
    fn reset_world_keeps_only_the_game_mode() {
        let mut weather = Weather::default();
        weather.apply(&change(1, 0.0));
        weather.apply(&change(8, 1.0));
        weather.apply(&change(3, 2.0));
        weather.tick(true);
        weather.reset_world();
        assert_eq!(weather.lightning_ticks(), 0);
        assert!(!weather.raining());
        assert_eq!(weather.thunder_strength(), 0.0);
        assert_eq!(weather.game_mode(), Some(2));
    }

    #[test]
    fn the_strengths_are_what_the_server_sent_without_a_ramp() {
        let mut weather = Weather::default();
        weather.apply(&change(7, 0.5));
        weather.tick(false);
        assert_eq!(weather.rain_strength(), 0.5);
        weather.apply(&change(8, 0.5));
        assert_eq!(weather.effective_thunder(), 0.25);
    }

    #[test]
    fn rain_started_by_reason_1_keeps_the_thunder_and_waits_for_reason_7() {
        let mut weather = Weather::default();
        weather.apply(&change(8, 1.0));
        weather.apply(&change(1, 0.0));
        assert_eq!(weather.rain_strength(), 0.0);
        assert_eq!(weather.effective_thunder(), 0.0);
        weather.apply(&change(7, 1.0));
        assert_eq!(weather.effective_thunder(), 1.0);
    }

    #[test]
    fn a_flash_lasts_two_ticks_past_the_last_bolt_tick() {
        let mut weather = Weather::default();
        assert_eq!(weather.lightning_ticks(), 0);
        weather.tick(true);
        assert_eq!(weather.lightning_ticks(), 2);
        weather.tick(true);
        assert_eq!(weather.lightning_ticks(), 2);
        weather.tick(false);
        assert_eq!(weather.lightning_ticks(), 1);
        weather.tick(false);
        weather.tick(false);
        assert_eq!(weather.lightning_ticks(), 0);
    }
}
