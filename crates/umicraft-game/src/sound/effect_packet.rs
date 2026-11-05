// SPDX-License-Identifier: MIT OR Apache-2.0

//! Effect (0x28) world events to [`SoundCue`]s.
//!
//! Source: MCP `RenderGlobal.playAuxSFX` (ids 1000-1022, 2000-2005),
//! `WorldClient.playSoundAtPos` (block centre, `+ 0.5` on each axis),
//! `RenderGlobal.playRecord` and `PositionedSoundRecord.create(location, x, y,
//! z)` (records: volume 4.0, pitch 1.0, at the block corner), and
//! `ItemRecord` / `Item.registerItems` for the record item ids 2256-2267.
//! Ids that only spawn particles (2000, 2003, 2004, 2005) or are unused
//! (1013, 1018, 1019, others) give no cue; 2001 and 2002 also spawn
//! particles, which `particles::block_effects` handles.

use std::borrow::Cow;

use umicraft_core::java_random::JavaRandom;
use umicraft_protocol::packets::play::clientbound::effect::Effect;

use super::cue::{CueCategory, SoundCue};

/// Source of the `Random.nextFloat()` values the effects consume.
pub trait CueRandom {
    /// Next float in `[0, 1)`.
    fn next_float(&mut self) -> f32;
}

impl CueRandom for JavaRandom {
    fn next_float(&mut self) -> f32 {
        JavaRandom::next_float(self)
    }
}

/// The break sound of a block (MCP `Block.SoundType`): name, volume and
/// frequency, before the effect's own scaling.
pub type BreakSound = (Cow<'static, str>, f32, f32);

/// Item id of the first record (`record_13`).
const FIRST_RECORD_ID: i32 = 2256;
/// Names of the records, in item id order from [`FIRST_RECORD_ID`].
const RECORDS: [&str; 12] = [
    "13", "cat", "blocks", "chirp", "far", "mall", "mellohi", "stal", "strad", "ward", "11", "wait",
];

/// Cues produced by an Effect packet; empty for particle-only or unknown ids.
///
/// `rng` supplies the pitch jitter; `break_sound` maps a block id (the low 12
/// bits of the data of id 2001) to its step sound type, or `None` for air.
/// A record id 1005 with a non-record item is the "stop the jukebox" case,
/// which makes no cue (stopping is the audio backend's business).
///
/// ```
/// # use umicraft_game::sound::effect_packet::effect_cues;
/// # use umicraft_core::java_random::JavaRandom;
/// # use umicraft_protocol::packets::play::clientbound::effect::Effect;
/// let click = Effect { effect_id: 1000, x: 0, y: 64, z: 0, data: 0, disable_relative_volume: false };
/// let cues = effect_cues(&click, &mut JavaRandom::new(1), |_| None);
/// assert_eq!(cues[0].name, "random.click");
/// ```
pub fn effect_cues(
    effect: &Effect,
    rng: &mut impl CueRandom,
    break_sound: impl Fn(u32) -> Option<BreakSound>,
) -> Vec<SoundCue> {
    let centre = [
        f64::from(effect.x) + 0.5,
        f64::from(effect.y) + 0.5,
        f64::from(effect.z) + 0.5,
    ];
    let at_centre = |name: &'static str, volume: f32, pitch: f32, category: CueCategory| {
        vec![SoundCue {
            name: Cow::Borrowed(name),
            pos: centre,
            volume,
            pitch,
            category,
        }]
    };
    let blocks = CueCategory::Blocks;
    let mobs = CueCategory::Hostile;
    match effect.effect_id {
        1000 => at_centre("random.click", 1.0, 1.0, blocks),
        1001 => at_centre("random.click", 1.0, 1.2, blocks),
        1002 => at_centre("random.bow", 1.0, 1.2, CueCategory::Players),
        1003 => at_centre("random.door_open", 1.0, door_pitch(rng), blocks),
        1004 => at_centre("random.fizz", 0.5, 2.6 + spread(rng) * 0.8, blocks),
        1005 => record_cue(effect),
        1006 => at_centre("random.door_close", 1.0, door_pitch(rng), blocks),
        1007 => at_centre("mob.ghast.charge", 10.0, jitter(rng), mobs),
        1008 => at_centre("mob.ghast.fireball", 10.0, jitter(rng), mobs),
        1009 => at_centre("mob.ghast.fireball", 2.0, jitter(rng), mobs),
        1010 => at_centre("mob.zombie.wood", 2.0, jitter(rng), mobs),
        1011 => at_centre("mob.zombie.metal", 2.0, jitter(rng), mobs),
        1012 => at_centre("mob.zombie.woodbreak", 2.0, jitter(rng), mobs),
        1014 => at_centre("mob.wither.shoot", 2.0, jitter(rng), mobs),
        1015 => at_centre("mob.bat.takeoff", 0.05, jitter(rng), CueCategory::Neutral),
        1016 => at_centre("mob.zombie.infect", 2.0, jitter(rng), mobs),
        1017 => at_centre("mob.zombie.unfect", 2.0, jitter(rng), mobs),
        1020 => at_centre("random.anvil_break", 1.0, door_pitch(rng), blocks),
        1021 => at_centre("random.anvil_use", 1.0, door_pitch(rng), blocks),
        1022 => at_centre("random.anvil_land", 0.3, door_pitch(rng), blocks),
        2001 => block_break_cue(effect, centre, break_sound),
        2002 => at_centre("game.potion.smash", 1.0, door_pitch(rng), blocks),
        _ => Vec::new(),
    }
}

/// `rand.nextFloat() * 0.1F + 0.9F`: doors and anvils.
fn door_pitch(rng: &mut impl CueRandom) -> f32 {
    rng.next_float() * 0.1 + 0.9
}

/// `nextFloat() - nextFloat()`, evaluated left to right like Java.
pub(super) fn spread(rng: &mut impl CueRandom) -> f32 {
    let first = rng.next_float();
    first - rng.next_float()
}

/// `(nextFloat() - nextFloat()) * 0.2F + 1.0F`: mob effects.
fn jitter(rng: &mut impl CueRandom) -> f32 {
    spread(rng) * 0.2 + 1.0
}

/// Id 1005: the record named by the item id, playing at the block corner.
fn record_cue(effect: &Effect) -> Vec<SoundCue> {
    let index = effect
        .data
        .checked_sub(FIRST_RECORD_ID)
        .and_then(|i| usize::try_from(i).ok());
    let Some(name) = index.and_then(|i| RECORDS.get(i)) else {
        return Vec::new();
    };
    vec![SoundCue {
        name: Cow::Owned(format!("records.{name}")),
        pos: [
            f64::from(effect.x),
            f64::from(effect.y),
            f64::from(effect.z),
        ],
        volume: 4.0,
        pitch: 1.0,
        category: CueCategory::Records,
    }]
}

/// Id 2001: `(volume + 1) / 2` at `frequency * 0.8` of the block's break
/// sound; `None` (air) is silent.
fn block_break_cue(
    effect: &Effect,
    centre: [f64; 3],
    break_sound: impl Fn(u32) -> Option<BreakSound>,
) -> Vec<SoundCue> {
    let block_id = (effect.data & 4095) as u32;
    break_sound(block_id)
        .map(|(name, volume, frequency)| SoundCue {
            name,
            pos: centre,
            volume: (volume + 1.0) / 2.0,
            pitch: frequency * 0.8,
            category: CueCategory::Blocks,
        })
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the queued floats in order, then 0.0.
    struct Fixed(Vec<f32>);

    impl CueRandom for Fixed {
        fn next_float(&mut self) -> f32 {
            if self.0.is_empty() {
                0.0
            } else {
                self.0.remove(0)
            }
        }
    }

    fn effect(id: i32, data: i32) -> Effect {
        Effect {
            effect_id: id,
            x: 10,
            y: 64,
            z: -3,
            data,
            disable_relative_volume: false,
        }
    }

    fn cues(id: i32, data: i32, floats: Vec<f32>) -> Vec<SoundCue> {
        effect_cues(&effect(id, data), &mut Fixed(floats), |block| {
            (block == 1).then_some((Cow::Borrowed("dig.stone"), 1.0, 1.0))
        })
    }

    #[test]
    fn clicks_and_bow_have_fixed_pitch() {
        let click = &cues(1000, 0, vec![])[0];
        assert_eq!(click.name, "random.click");
        assert_eq!((click.volume, click.pitch), (1.0, 1.0));
        assert_eq!(click.pos, [10.5, 64.5, -2.5]);
        assert_eq!(cues(1001, 0, vec![])[0].pitch, 1.2);
        assert_eq!(cues(1002, 0, vec![])[0].name, "random.bow");
    }

    #[test]
    fn doors_and_anvils_use_the_narrow_pitch_range() {
        for (id, name, volume) in [
            (1003, "random.door_open", 1.0),
            (1006, "random.door_close", 1.0),
            (1020, "random.anvil_break", 1.0),
            (1021, "random.anvil_use", 1.0),
            (1022, "random.anvil_land", 0.3),
            (2002, "game.potion.smash", 1.0),
        ] {
            let cue = &cues(id, 0, vec![0.5])[0];
            assert_eq!((cue.name.as_ref(), cue.volume), (name, volume), "id {id}");
            assert_eq!(cue.pitch, 0.5 * 0.1 + 0.9, "id {id}");
        }
    }

    #[test]
    fn fizz_subtracts_two_floats_in_order() {
        let cue = &cues(1004, 0, vec![0.75, 0.25])[0];
        assert_eq!(cue.volume, 0.5);
        assert_eq!(cue.pitch, 2.6 + 0.5 * 0.8);
    }

    #[test]
    fn mob_effects_jitter_around_one() {
        for (id, name, volume) in [
            (1007, "mob.ghast.charge", 10.0),
            (1008, "mob.ghast.fireball", 10.0),
            (1009, "mob.ghast.fireball", 2.0),
            (1010, "mob.zombie.wood", 2.0),
            (1011, "mob.zombie.metal", 2.0),
            (1012, "mob.zombie.woodbreak", 2.0),
            (1014, "mob.wither.shoot", 2.0),
            (1015, "mob.bat.takeoff", 0.05),
            (1016, "mob.zombie.infect", 2.0),
            (1017, "mob.zombie.unfect", 2.0),
        ] {
            let cue = &cues(id, 0, vec![0.75, 0.25])[0];
            assert_eq!((cue.name.as_ref(), cue.volume), (name, volume), "id {id}");
            assert_eq!(cue.pitch, 0.5 * 0.2 + 1.0, "id {id}");
        }
    }

    #[test]
    fn records_map_item_ids_and_play_at_the_block_corner() {
        let cue = &cues(1005, 2257, vec![])[0];
        assert_eq!(cue.name, "records.cat");
        assert_eq!(cue.pos, [10.0, 64.0, -3.0]);
        assert_eq!((cue.volume, cue.pitch), (4.0, 1.0));
        assert_eq!(cue.category, CueCategory::Records);
        assert_eq!(cues(1005, 2267, vec![])[0].name, "records.wait");
    }

    #[test]
    fn stopping_a_record_or_a_non_record_makes_no_cue() {
        assert!(cues(1005, 0, vec![]).is_empty());
        assert!(cues(1005, 2255, vec![]).is_empty());
        assert!(cues(1005, 2268, vec![]).is_empty());
        assert!(cues(1005, -5, vec![]).is_empty());
    }

    #[test]
    fn block_break_scales_the_sound_type() {
        let data = 1 | (3 << 12);
        let cue = effect_cues(&effect(2001, data), &mut Fixed(vec![]), |id| {
            assert_eq!(id, 1);
            Some((Cow::Borrowed("dig.glass"), 0.5, 1.5))
        });
        assert_eq!(cue.len(), 1);
        assert_eq!(cue[0].name, "dig.glass");
        assert_eq!(cue[0].volume, 0.75);
        assert_eq!(cue[0].pitch, 1.5 * 0.8);
        assert_eq!(cue[0].pos, [10.5, 64.5, -2.5]);
    }

    #[test]
    fn air_breaks_silently() {
        assert!(cues(2001, 0, vec![]).is_empty());
    }

    #[test]
    fn particle_only_and_unknown_ids_make_no_cue() {
        for id in [2000, 2003, 2004, 2005, 1013, 1018, 1019, 1023, 0, -1] {
            assert!(cues(id, 0, vec![]).is_empty(), "id {id}");
        }
    }

    #[test]
    fn java_random_is_a_cue_random() {
        let mut a = JavaRandom::new(3);
        let mut b = JavaRandom::new(3);
        assert_eq!(CueRandom::next_float(&mut a), b.next_float());
    }
}
