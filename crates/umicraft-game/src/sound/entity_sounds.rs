// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hurt and death sound of the local player.
//!
//! In 1.8 the client plays an entity's hurt sound on `Entity Status` 2 and its
//! death sound on 3 (`EntityLivingBase.handleStatusUpdate`), but only the
//! local player really hears it: every other entity calls
//! `World.playSoundAtEntity`, which on the client forwards to
//! `RenderGlobal.playSound`, an empty method. Their sound comes from the
//! server's Sound Effect packet (`WorldManager.playSound`), which
//! [`super::sound_packet`] already turns into a cue. The local player is the
//! exception twice over: `EntityPlayerSP.playSound` plays locally, and the
//! server leaves the hurt player out of the broadcast
//! (`EntityPlayer.playSound` -> `playSoundToNearExcept`). Playing the status of
//! other entities here would therefore sound everything twice. See
//! `docs/sources.md` ("P11-H hurt and death sounds").

use std::borrow::Cow;

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::cue::{CueCategory, SoundCue};

/// `Entity Status` code of a hurt entity.
pub const STATUS_HURT: i8 = 2;
/// `Entity Status` code of a dead entity.
pub const STATUS_DEATH: i8 = 3;

/// `EntityPlayer.getHurtSound`.
const PLAYER_HURT: &str = "game.player.hurt";
/// `EntityPlayer.getDeathSound`.
const PLAYER_DEATH: &str = "game.player.die";

/// The sound the local player makes when the server reports `status`, or
/// `None` for a status that makes no sound.
///
/// The cue sits at the feet (`posX, posY, posZ`, as `Entity.playSound` does),
/// has the volume of `getSoundVolume` (1.0, `EntityPlayer` does not override
/// it) and the pitch `(rand - rand) * 0.2 + 1.0` that `handleStatusUpdate`
/// draws itself instead of calling `getSoundPitch` (so a child pitch does not
/// apply). Two draws are consumed only when a cue is produced.
///
/// ```
/// use glam::DVec3;
/// use umicraft_core::java_random::JavaRandom;
/// use umicraft_game::sound::entity_sounds::local_player_status_cue;
///
/// let mut rng = JavaRandom::new(1);
/// let cue = local_player_status_cue(2, DVec3::new(1.0, 64.0, 2.0), &mut rng).unwrap();
/// assert_eq!(cue.name, "game.player.hurt");
/// assert_eq!(cue.pos, [1.0, 64.0, 2.0]);
/// assert!(local_player_status_cue(9, DVec3::ZERO, &mut rng).is_none());
/// ```
pub fn local_player_status_cue(status: i8, feet: DVec3, rng: &mut JavaRandom) -> Option<SoundCue> {
    let name = match status {
        STATUS_HURT => PLAYER_HURT,
        STATUS_DEATH => PLAYER_DEATH,
        _ => return None,
    };
    Some(SoundCue {
        name: Cow::Borrowed(name),
        pos: feet.to_array(),
        volume: 1.0,
        pitch: (rng.next_float() - rng.next_float()) * 0.2 + 1.0,
        category: CueCategory::Players,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(status: i8) -> Option<SoundCue> {
        local_player_status_cue(status, DVec3::new(1.5, 64.0, -2.5), &mut JavaRandom::new(7))
    }

    #[test]
    fn status_two_is_the_hurt_sound_at_the_feet() {
        let cue = cue(STATUS_HURT).unwrap();
        assert_eq!(cue.name, "game.player.hurt");
        assert_eq!(cue.pos, [1.5, 64.0, -2.5]);
        assert_eq!(cue.volume, 1.0);
        assert_eq!(cue.category, CueCategory::Players);
    }

    #[test]
    fn status_three_is_the_death_sound() {
        assert_eq!(cue(STATUS_DEATH).unwrap().name, "game.player.die");
    }

    #[test]
    fn other_statuses_make_no_sound_and_draw_no_random_number() {
        let mut rng = JavaRandom::new(7);
        assert!(local_player_status_cue(9, DVec3::ZERO, &mut rng).is_none());
        assert_eq!(rng.next_float(), JavaRandom::new(7).next_float());
    }

    #[test]
    fn the_pitch_stays_within_the_vanilla_spread() {
        let mut rng = JavaRandom::new(3);
        for _ in 0..200 {
            let pitch = local_player_status_cue(STATUS_HURT, DVec3::ZERO, &mut rng)
                .unwrap()
                .pitch;
            assert!((0.8..=1.2).contains(&pitch), "{pitch}");
        }
    }
}
