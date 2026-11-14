// SPDX-License-Identifier: MIT OR Apache-2.0

//! The potion effects active on the local player.
//!
//! Source: MCP `NetHandlerPlayClient.handleEntityEffect` /
//! `handleRemoveEntityEffect`, `EntityLivingBase.addPotionEffect` and
//! `PotionEffect.combine` / `onUpdate`. The server is authoritative about
//! which effects exist; this table only mirrors them so the HUD can draw the
//! icons and the movement code can read speed or jump boost.

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::entity_effect::EntityEffect;

/// The duration value the server sends for "maximum duration" effects
/// (MCP `S1DPacketEntityEffect.func_149429_c`).
const MAX_DURATION_MARKER: i32 = 32767;

/// One active potion effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveEffect {
    /// Potion id (1 = speed, 8 = jump boost, ...). The wire byte, unsigned.
    pub effect_id: u8,
    /// Amplifier, 0 for level I. Signed like the wire byte.
    pub amplifier: i8,
    /// Remaining duration in ticks.
    pub duration: i32,
    /// Whether the effect particles are shown.
    pub show_particles: bool,
    /// The server flagged the duration as "maximum" (32767 ticks).
    pub max_duration: bool,
}

/// The bucket count a new `java.util.HashMap` starts with.
const INITIAL_BUCKETS: usize = 16;

/// The effects of the local player, keyed by potion id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    active: BTreeMap<u8, ActiveEffect>,
    /// The potion ids in the order they were added; an effect that is
    /// combined keeps its place. Read by [`Self::listed`].
    added: Vec<u8>,
    /// The bucket count of the `HashMap` the reference keeps the effects in
    /// (`EntityLivingBase.activePotionsMap`); `0` until the first effect. It
    /// doubles past three quarters full and never shrinks.
    buckets: usize,
}

impl Effects {
    /// Adds the effect of a packet, merging with an active effect of the
    /// same potion like `PotionEffect.combine`: a higher amplifier replaces
    /// the effect, an equal one keeps the longer duration, a lower one only
    /// updates the particle flag.
    pub fn apply(&mut self, packet: &EntityEffect) {
        let incoming = ActiveEffect {
            effect_id: packet.effect_id as u8,
            amplifier: packet.amplifier,
            duration: packet.duration,
            // The wire field is named "hide particles" but the reference
            // client writes `isShowParticles` into it: non-zero shows them.
            show_particles: packet.hide_particles != 0,
            max_duration: packet.duration == MAX_DURATION_MARKER,
        };
        match self.active.get_mut(&incoming.effect_id) {
            Some(current) => combine(current, incoming),
            None => {
                self.active.insert(incoming.effect_id, incoming);
                self.added.push(incoming.effect_id);
                self.grow();
            }
        }
    }

    /// `HashMap.resize`: twice the buckets once more than three quarters
    /// are used.
    fn grow(&mut self) {
        self.buckets = self.buckets.max(INITIAL_BUCKETS);
        while self.active.len() > self.buckets * 3 / 4 {
            self.buckets *= 2;
        }
    }

    /// Removes the effect with this potion id, if active.
    pub fn remove(&mut self, effect_id: u8) {
        self.active.remove(&effect_id);
        self.added.retain(|&id| id != effect_id);
    }

    /// Advances every effect by one tick (`PotionEffect.onUpdate`).
    ///
    /// An effect that ran out stays at duration 0: MCP
    /// `EntityLivingBase.updatePotionEffects` only removes it on the server
    /// side (`!worldObj.isRemote`), the client waits for `Remove Entity
    /// Effect` (0x1E).
    pub fn tick(&mut self) {
        for effect in self.active.values_mut() {
            effect.duration = effect.duration.saturating_sub(1).max(0);
        }
    }

    /// Forgets every effect, as the new player of a respawn starts with a
    /// new map.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The effect with this potion id, if active.
    pub fn get(&self, effect_id: u8) -> Option<&ActiveEffect> {
        self.active.get(&effect_id)
    }

    /// The active effects in potion id order.
    pub fn iter(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active.values()
    }

    /// The active effects in the order the reference iterates its
    /// `HashMap<Integer, PotionEffect>`, which is the order of the effect list
    /// of the inventory (`InventoryEffectRenderer.drawActivePotionEffects`).
    ///
    /// The hash of an `Integer` is its value, so an effect sits in bucket
    /// `id % buckets`; the buckets are walked in order and a bucket keeps its
    /// effects in the order they were added. With 16 buckets night vision
    /// (16) comes first and hunger (17) right after speed (1).
    pub fn listed(&self) -> Vec<&ActiveEffect> {
        let mask = self.buckets.max(INITIAL_BUCKETS) - 1;
        let mut listed: Vec<(usize, &ActiveEffect)> = self
            .added
            .iter()
            .enumerate()
            .filter_map(|(order, id)| Some((order, self.active.get(id)?)))
            .collect();
        listed.sort_by_key(|&(order, effect)| (usize::from(effect.effect_id) & mask, order));
        listed.into_iter().map(|(_, effect)| effect).collect()
    }

    /// The number of active effects.
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Whether no effect is active.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

/// `PotionEffect.combine`. It does not copy `isPotionDurationMax`: an effect
/// keeps the flag it was added with.
fn combine(current: &mut ActiveEffect, incoming: ActiveEffect) {
    if incoming.amplifier > current.amplifier {
        current.amplifier = incoming.amplifier;
        current.duration = incoming.duration;
    } else if incoming.amplifier == current.amplifier && current.duration < incoming.duration {
        current.duration = incoming.duration;
    }
    current.show_particles = incoming.show_particles;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(effect_id: i8, amplifier: i8, duration: i32) -> EntityEffect {
        EntityEffect {
            entity_id: 1,
            effect_id,
            amplifier,
            duration,
            hide_particles: 1,
        }
    }

    #[test]
    fn a_new_effect_is_stored() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 600));
        let effect = effects.get(1).unwrap();
        assert_eq!((effect.amplifier, effect.duration), (0, 600));
        assert!(effect.show_particles);
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn a_higher_amplifier_replaces_and_a_lower_one_does_not() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 600));
        effects.apply(&packet(1, 2, 100));
        assert_eq!(effects.get(1).unwrap().amplifier, 2);
        assert_eq!(effects.get(1).unwrap().duration, 100);
        effects.apply(&packet(1, 1, 9000));
        assert_eq!(effects.get(1).unwrap().amplifier, 2);
        assert_eq!(effects.get(1).unwrap().duration, 100);
    }

    #[test]
    fn an_equal_amplifier_keeps_the_longer_duration() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 600));
        effects.apply(&packet(1, 0, 100));
        assert_eq!(effects.get(1).unwrap().duration, 600);
        effects.apply(&packet(1, 0, 900));
        assert_eq!(effects.get(1).unwrap().duration, 900);
    }

    #[test]
    fn the_max_duration_marker_is_flagged() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 32767));
        assert!(effects.get(1).unwrap().max_duration);
    }

    #[test]
    fn a_combined_effect_keeps_its_max_duration_flag() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 600));
        effects.apply(&packet(1, 1, 32767));
        let effect = effects.get(1).unwrap();
        assert_eq!((effect.amplifier, effect.duration), (1, 32767));
        assert!(!effect.max_duration, "the flag of the first packet stays");
    }

    fn listed_ids(effects: &Effects) -> Vec<u8> {
        effects.listed().iter().map(|e| e.effect_id).collect()
    }

    #[test]
    fn the_list_follows_the_buckets_of_the_reference_hash_map() {
        let mut effects = Effects::default();
        for id in [1, 16, 17, 5, 3] {
            effects.apply(&packet(id, 0, 600));
        }
        // Bucket 0 holds 16; bucket 1 holds 1 then 17, in the order added.
        assert_eq!(listed_ids(&effects), [16, 1, 17, 3, 5]);
        // Added again after a removal, 1 goes behind 17 in its bucket.
        effects.remove(1);
        effects.apply(&packet(1, 0, 600));
        assert_eq!(listed_ids(&effects), [16, 17, 1, 3, 5]);
        // Combining does not move an effect.
        effects.apply(&packet(17, 2, 600));
        assert_eq!(listed_ids(&effects), [16, 17, 1, 3, 5]);
    }

    #[test]
    fn past_twelve_effects_the_map_grows_and_stays_grown() {
        let mut effects = Effects::default();
        for id in 1..=13 {
            effects.apply(&packet(id, 0, 600));
        }
        effects.apply(&packet(16, 0, 600));
        // 32 buckets: every id has its own, in id order.
        assert_eq!(listed_ids(&effects)[..2], [1, 2]);
        assert_eq!(*listed_ids(&effects).last().unwrap(), 16);
        for id in 2..=13 {
            effects.remove(id);
        }
        assert_eq!(listed_ids(&effects), [1, 16], "the map does not shrink");
        effects.clear();
        effects.apply(&packet(1, 0, 600));
        effects.apply(&packet(16, 0, 600));
        assert_eq!(listed_ids(&effects), [16, 1], "a new player has a new map");
    }

    #[test]
    fn tick_counts_down_and_keeps_expired_effects_until_removed() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 2));
        effects.tick();
        assert_eq!(effects.get(1).unwrap().duration, 1);
        effects.tick();
        effects.tick();
        assert_eq!(effects.get(1).unwrap().duration, 0);
        effects.remove(1);
        assert!(effects.is_empty());
    }

    #[test]
    fn remove_and_clear_forget_effects() {
        let mut effects = Effects::default();
        effects.apply(&packet(1, 0, 10));
        effects.apply(&packet(-1, 0, 10));
        assert_eq!(effects.get(255).map(|e| e.effect_id), Some(255));
        effects.remove(1);
        assert!(effects.get(1).is_none());
        effects.remove(77);
        effects.clear();
        assert_eq!(effects.iter().count(), 0);
    }
}
