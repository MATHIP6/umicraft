// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rain sound of the 1.8 client.
//!
//! Rain is played by the client, from the same pass that makes the splash
//! particles (`EntityRenderer.addRainParticles`, line 1573): after the
//! drops of a tick it occasionally plays `ambient.weather.rain` at one of the
//! splash spots, louder and higher when the player is outdoors, quieter and
//! lower when a roof is over the player's head and the rain falls above it.
//! [`particles::ambient::rain_splashes`](crate::particles::ambient::rain_splashes)
//! feeds this module with what the drops hit.
//!
//! Thunder is *not* here: `EntityLightningBolt.onUpdate` plays
//! `ambient.weather.thunder` and `random.explode` with
//! `World.playSoundEffect`, which does nothing on the client
//! (`RenderGlobal.playSound` is empty), so the sounds only exist as Sound
//! Effect packets of the server ([`sound_packet`](super::sound_packet)).

use std::borrow::Cow;

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::cue::{CueCategory, SoundCue};

/// The event the rain plays.
const RAIN_EVENT: &str = "ambient.weather.rain";
/// Volume and pitch with a roof over the player and the rain above it.
const MUFFLED: (f32, f32) = (0.1, 0.5);
/// Volume and pitch otherwise.
const OPEN: (f32, f32) = (0.2, 1.0);

/// What the drops of one tick hit: `j` and `(d0, d1, d2)` of the reference.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RainDrops {
    hits: u32,
    spot: Option<DVec3>,
}

impl RainDrops {
    /// Records a drop that landed on a block at `spot` (the splash position
    /// lowered by one, as the reference keeps it) and draws whether it
    /// replaces the remembered spot (`random.nextInt(++j) == 0`).
    pub fn record(&mut self, spot: DVec3, rng: &mut JavaRandom) {
        self.hits += 1;
        if rng.next_int(self.hits) == 0 {
            self.spot = Some(spot);
        }
    }
}

/// `EntityRenderer.rainSoundCounter`: makes the sound more likely the longer
/// it was silent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RainSound {
    counter: u32,
}

impl RainSound {
    /// Decides whether the tick's drops make the rain sound.
    ///
    /// `rng` is the generator the drops were drawn from, and `sheltered` is
    /// whether a block stops precipitation at or above the player's feet
    /// (`getPrecipitationHeight(player).getY() > player.getY()`). Without a
    /// hit nothing is drawn and the counter stays.
    ///
    /// ```
    /// # use umicraft_game::sound::weather::{RainDrops, RainSound};
    /// # use umicraft_core::java_random::JavaRandom;
    /// let mut sound = RainSound::default();
    /// assert!(sound.decide(&RainDrops::default(), 64, false, &mut JavaRandom::new(1)).is_none());
    /// ```
    pub fn decide(
        &mut self,
        drops: &RainDrops,
        feet_y: i32,
        sheltered: bool,
        rng: &mut JavaRandom,
    ) -> Option<SoundCue> {
        let spot = drops.spot.filter(|_| drops.hits > 0)?;
        let due = rng.next_int(3) < self.counter;
        self.counter += 1;
        if !due {
            return None;
        }
        self.counter = 0;
        let (volume, pitch) = if spot.y > f64::from(feet_y + 1) && sheltered {
            MUFFLED
        } else {
            OPEN
        };
        Some(SoundCue {
            name: Cow::Borrowed(RAIN_EVENT),
            pos: spot.to_array(),
            volume,
            pitch,
            category: CueCategory::Weather,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drops_at(y: f64) -> RainDrops {
        let mut drops = RainDrops::default();
        drops.record(DVec3::new(1.0, y, 2.0), &mut JavaRandom::new(5));
        drops
    }

    #[test]
    fn the_first_drop_always_becomes_the_spot() {
        let drops = drops_at(70.0);
        assert_eq!(drops.hits, 1);
        assert_eq!(drops.spot, Some(DVec3::new(1.0, 70.0, 2.0)));
    }

    #[test]
    fn no_drops_make_no_sound_and_draw_nothing() {
        let mut rng = JavaRandom::new(9);
        let mut untouched = JavaRandom::new(9);
        let mut sound = RainSound::default();
        assert!(sound
            .decide(&RainDrops::default(), 64, false, &mut rng)
            .is_none());
        assert_eq!(rng.next_int(1000), untouched.next_int(1000));
        assert_eq!(sound, RainSound::default());
    }

    #[test]
    fn the_sound_gets_likelier_until_it_plays_and_then_starts_over() {
        let mut sound = RainSound::default();
        let drops = drops_at(64.0);
        let mut rng = JavaRandom::new(3);
        let mut plays = 0;
        for _ in 0..30 {
            // The first call of a series can never play: `nextInt(3) < 0`.
            let before = sound;
            let cue = sound.decide(&drops, 64, false, &mut rng);
            if before == RainSound::default() {
                assert!(cue.is_none());
            }
            plays += usize::from(cue.is_some());
        }
        // After three silent ticks `nextInt(3) < 3` always holds.
        assert!(plays >= 30 / 4, "{plays}");
    }

    #[test]
    fn rain_above_a_roof_is_quiet_and_low_otherwise_open() {
        let sound_for = |y: f64, sheltered: bool| {
            let mut sound = RainSound { counter: 3 };
            sound
                .decide(&drops_at(y), 64, sheltered, &mut JavaRandom::new(1))
                .expect("a full counter always plays")
        };
        let roofed = sound_for(70.0, true);
        assert_eq!((roofed.volume, roofed.pitch), (0.1, 0.5));
        let open = sound_for(70.0, false);
        assert_eq!((open.volume, open.pitch), (0.2, 1.0));
        let low = sound_for(65.0, true);
        assert_eq!((low.volume, low.pitch), (0.2, 1.0));
        assert_eq!(open.category, CueCategory::Weather);
        assert_eq!(open.name, "ambient.weather.rain");
    }
}
