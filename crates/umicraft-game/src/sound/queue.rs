// SPDX-License-Identifier: MIT OR Apache-2.0

//! The [`SoundQueue`]: the cues the game thread has produced and the render
//! thread has not played yet.
//!
//! Every producer (footsteps, digging, rain, server packets) appends here and
//! the owner of the audio device drains it once per frame. The queue also
//! owns the generator of the pitch jitter, so cues are reproducible for a
//! given seed and sequence of events.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_protocol::packets::play::clientbound::effect::Effect;
use umicraft_protocol::packets::play::clientbound::sound_effect::SoundEffect;

use super::block_events::break_sound;
use super::cue::SoundCue;
use super::effect_packet::effect_cues;
use super::entity_sounds::local_player_status_cue;
use super::sound_packet::cue_from_packet;
use super::step::Stepper;
use crate::physics::Player;
use crate::shapes::BlockSource;

/// The seed of the jitter generator: the reference seeds `Entity.rand` from
/// the clock, so any value is as faithful as another.
const JITTER_SEED: u64 = 0x5EED_500D;

/// Pending cues plus the generator that varies their pitch.
#[derive(Debug, Clone)]
pub struct SoundQueue {
    cues: Vec<SoundCue>,
    rng: JavaRandom,
}

impl Default for SoundQueue {
    fn default() -> Self {
        Self {
            cues: Vec::new(),
            rng: JavaRandom::new(JITTER_SEED),
        }
    }
}

impl SoundQueue {
    /// Creates an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one cue.
    pub fn push(&mut self, cue: SoundCue) {
        self.cues.push(cue);
    }

    /// Appends several cues in order.
    pub fn extend(&mut self, cues: impl IntoIterator<Item = SoundCue>) {
        self.cues.extend(cues);
    }

    /// Takes every pending cue, oldest first, leaving the queue empty.
    pub fn drain(&mut self) -> Vec<SoundCue> {
        std::mem::take(&mut self.cues)
    }

    /// Whether no cue waits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// Queues the cue of a Sound Effect packet (0x29).
    pub fn sound_effect(&mut self, packet: &SoundEffect) {
        self.cues.push(cue_from_packet(packet));
    }

    /// Queues the cues of an Effect packet (0x28); id 2001 plays the break
    /// sound of the block in its data.
    pub fn effect(&mut self, packet: &Effect) {
        let cues = effect_cues(packet, &mut self.rng, break_sound);
        self.cues.extend(cues);
    }

    /// Queues the hurt or death sound of the local player for an `Entity
    /// Status` code (see [`super::entity_sounds`]).
    pub fn local_player_status(&mut self, status: i8, feet: DVec3) {
        let cue = local_player_status_cue(status, feet, &mut self.rng);
        self.cues.extend(cue);
    }

    /// Queues the movement sounds of the tick the player just ran.
    pub fn record_steps<W: BlockSource + ?Sized>(
        &mut self,
        stepper: &mut Stepper,
        player: &Player,
        before: DVec3,
        world: &W,
    ) {
        stepper.tick(player, before, world, &mut self.rng, &mut self.cues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(effect_id: i32, data: i32) -> Effect {
        Effect {
            effect_id,
            x: 1,
            y: 2,
            z: 3,
            data,
            disable_relative_volume: false,
        }
    }

    #[test]
    fn a_sound_effect_packet_becomes_one_cue_and_drain_empties_the_queue() {
        let mut queue = SoundQueue::new();
        queue.sound_effect(&SoundEffect {
            name: "random.click".into(),
            x: 8,
            y: 16,
            z: 24,
            volume: 0.5,
            pitch: 63,
        });
        assert!(!queue.is_empty());
        let cues = queue.drain();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].name, "random.click");
        assert_eq!(cues[0].pos, [1.0, 2.0, 3.0]);
        assert!(queue.is_empty());
    }

    #[test]
    fn a_break_effect_plays_the_dig_sound_of_the_block_in_the_data() {
        let mut queue = SoundQueue::new();
        // Glass (id 20) with metadata 5 in the high bits, then air.
        queue.effect(&effect(2001, 20 | (5 << 12)));
        queue.effect(&effect(2001, 0));
        let cues = queue.drain();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].name, "dig.glass");
        assert_eq!(cues[0].pos, [1.5, 2.5, 3.5]);
    }

    #[test]
    fn a_click_effect_is_a_cue_and_a_particle_only_effect_is_not() {
        let mut queue = SoundQueue::new();
        queue.effect(&effect(1000, 0));
        queue.effect(&effect(2000, 0));
        assert_eq!(queue.drain()[0].name, "random.click");
        assert!(queue.is_empty());
    }

    #[test]
    fn the_local_player_status_queues_its_hurt_sound_and_ignores_others() {
        let mut queue = SoundQueue::new();
        queue.local_player_status(9, DVec3::ZERO);
        assert!(queue.is_empty());
        queue.local_player_status(2, DVec3::new(1.0, 2.0, 3.0));
        let cues = queue.drain();
        assert_eq!(cues[0].name, "game.player.hurt");
        assert_eq!(cues[0].pos, [1.0, 2.0, 3.0]);
    }

    struct Floor;

    impl BlockSource for Floor {
        fn block(&self, _x: i32, y: i32, _z: i32) -> umicraft_world::BlockState {
            if y == 0 {
                umicraft_world::BlockState::new(1, 0)
            } else {
                umicraft_world::BlockState::AIR
            }
        }
    }

    #[test]
    fn the_game_state_collects_the_steps_of_a_walk_and_drains_them() {
        use crate::{GameState, InputIntent};
        let mut state = GameState::new(DVec3::new(0.5, 1.0, 0.5));
        for _ in 0..60 {
            state.tick(&InputIntent::walking(0.0), &Floor);
        }
        let cues = state.drain_sound_cues();
        assert!(cues.len() >= 4, "{}", cues.len());
        assert!(cues.iter().all(|cue| cue.name == "step.stone"));
        assert!(state.drain_sound_cues().is_empty());
    }
}
