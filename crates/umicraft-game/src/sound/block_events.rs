// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block sounds the 1.8 client plays itself: the hits while digging and
//! the break sound when it destroys a block.
//!
//! What is *not* here, because the reference does not do it on the client:
//! the placing sound. `ItemBlock.onItemUse` calls `World.playSoundEffect`,
//! and on the client `RenderGlobal.playSound` is empty, so the sound arrives
//! as a Sound Effect packet from the server (see
//! [`sound_packet`](super::sound_packet)). The break sound, in contrast, is
//! played locally by `PlayerControllerMP.onPlayerDestroyBlock`
//! (`world.playAuxSFX(2001, ...)`), and the server skips the digger when it
//! broadcasts the same effect.
//!
//! Sources: MCP `PlayerControllerMP.onPlayerDamageBlock` (lines 285-330),
//! `clickBlock`, `onPlayerDestroyBlock` and `RenderGlobal.playAuxSFX` 2001.

use umicraft_core::java_random::JavaRandom;
use umicraft_protocol::packets::play::clientbound::effect::Effect;
use umicraft_world::BlockPos;

use super::cue::{CueCategory, SoundCue};
use super::effect_packet::{effect_cues, BreakSound};
use super::sound_type;

/// The world event of a destroyed block (`RenderGlobal.playAuxSFX` case 2001).
const BLOCK_BREAK_EVENT: i32 = 2001;
/// The hit sound plays on every fourth tick of digging.
const HIT_INTERVAL: u32 = 4;

/// The break sound of a block id (`Block.stepSound`), `None` for air.
///
/// This is the closure [`effect_cues`] wants for the Effect 2001 packet.
#[must_use]
pub fn break_sound(block_id: u32) -> Option<BreakSound> {
    if block_id == 0 {
        return None;
    }
    let kind = sound_type(u16::try_from(block_id).ok()?);
    Some((kind.break_sound().into_cow(), kind.volume, kind.pitch))
}

/// The cue of the block the player just destroyed, `None` for air.
///
/// It goes through the same code as the Effect 2001 packet, so the local
/// break and the one other players hear are identical.
#[must_use]
pub fn break_cue(block: BlockPos, block_id: u16) -> Option<SoundCue> {
    let effect = Effect {
        effect_id: BLOCK_BREAK_EVENT,
        x: block.x,
        y: block.y,
        z: block.z,
        data: i32::from(block_id),
        disable_relative_volume: false,
    };
    // Id 2001 draws no random numbers, so the generator is never advanced.
    effect_cues(&effect, &mut JavaRandom::new(0), break_sound)
        .into_iter()
        .next()
}

/// `PlayerControllerMP.stepSoundTickCounter`: paces the hit sounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HitSounds {
    counter: u32,
}

impl HitSounds {
    /// Restarts the pace; called when a new block starts to be dug and after
    /// one is destroyed (`stepSoundTickCounter = 0.0F`). Aborting a dig does
    /// not reset it in the reference either.
    pub fn restart(&mut self) {
        self.counter = 0;
    }

    /// One tick of digging `block_id` at `block`: every fourth tick (starting
    /// with the first) the block's step sound plays at `(volume + 1) / 8`
    /// and half its pitch, at the block centre.
    pub fn hit(&mut self, block: BlockPos, block_id: u16) -> Option<SoundCue> {
        let due = self.counter % HIT_INTERVAL == 0;
        self.counter += 1;
        if !due {
            return None;
        }
        let kind = sound_type(block_id);
        Some(SoundCue {
            name: kind.step_sound().into_cow(),
            pos: [
                f64::from(block.x) + 0.5,
                f64::from(block.y) + 0.5,
                f64::from(block.z) + 0.5,
            ],
            volume: (kind.volume + 1.0) / 8.0,
            pitch: kind.pitch * 0.5,
            category: CueCategory::Blocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POS: BlockPos = BlockPos { x: 3, y: 64, z: -2 };

    #[test]
    fn digging_hits_every_fourth_tick_starting_with_the_first() {
        let mut hits = HitSounds::default();
        let ticks: Vec<bool> = (0..9).map(|_| hits.hit(POS, 1).is_some()).collect();
        assert_eq!(
            ticks,
            [true, false, false, false, true, false, false, false, true]
        );
    }

    #[test]
    fn a_hit_is_the_quiet_low_step_sound_at_the_block_centre() {
        let cue = HitSounds::default().hit(POS, 2).expect("first tick hits");
        assert_eq!(cue.name, "step.grass");
        assert!((cue.volume - 0.25).abs() < 1e-6);
        assert!((cue.pitch - 0.5).abs() < 1e-6);
        assert_eq!(cue.pos, [3.5, 64.5, -1.5]);
    }

    #[test]
    fn restarting_makes_the_next_tick_hit_again() {
        let mut hits = HitSounds::default();
        assert!(hits.hit(POS, 1).is_some());
        assert!(hits.hit(POS, 1).is_none());
        hits.restart();
        assert!(hits.hit(POS, 1).is_some());
    }

    #[test]
    fn breaking_plays_the_dig_sound_at_the_vanilla_scale() {
        let cue = break_cue(POS, 20).expect("glass breaks");
        assert_eq!(cue.name, "dig.glass");
        assert!((cue.volume - 1.0).abs() < 1e-6);
        assert!((cue.pitch - 0.8).abs() < 1e-6);
        assert_eq!(cue.pos, [3.5, 64.5, -1.5]);
        let metal = break_cue(POS, 42).expect("iron breaks");
        assert!((metal.pitch - 1.2).abs() < 1e-6);
    }

    #[test]
    fn breaking_air_is_silent() {
        assert!(break_cue(POS, 0).is_none());
        assert!(break_sound(0).is_none());
    }
}
