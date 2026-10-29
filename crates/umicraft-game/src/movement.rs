// SPDX-License-Identifier: MIT OR Apache-2.0

//! The movement stats the server and the status effects give the local
//! player: the movement speed attribute, Jump Boost and Blindness.
//!
//! MCP applies the Speed and Slowness effects as attribute modifiers
//! (`Potion.registerPotionAttributeModifier`, `Potion.getAttributeModifierAmount`:
//! `amount * (amplifier + 1)`), and the server also sends them in `Entity
//! Properties`. A modifier has a UUID, so one that arrives both ways counts
//! once. The sprint boost is added by the movement code itself
//! (`EntityLivingBase.setSprinting`), so the sprint modifier the server may
//! list is left out.

use std::collections::BTreeMap;

use crate::physics::WALK_SPEED;
use crate::server::attributes::{AttributeInstance, Attributes};
use crate::server::effects::Effects;

/// The attribute name of the movement speed (MCP `generic.movementSpeed`).
pub const MOVEMENT_SPEED: &str = "generic.movementSpeed";

/// The potion id of Speed (MCP `Potion.moveSpeed`).
pub const SPEED: u8 = 1;
/// The potion id of Slowness (MCP `Potion.moveSlowdown`).
pub const SLOWNESS: u8 = 2;
/// The potion id of Jump Boost (MCP `Potion.jump`).
pub const JUMP_BOOST: u8 = 8;
/// The potion id of Blindness (MCP `Potion.blindness`).
pub const BLINDNESS: u8 = 15;

/// The UUID of the sprint boost (MCP `EntityLivingBase.java:56`).
const SPRINT_UUID: u128 = 0x662A_6B8D_DA3E_4C1C_8813_96EA_6097_278D;
/// The UUID of the Speed modifier (MCP `Potion.java:26`).
const SPEED_UUID: u128 = 0x91AE_AA56_376B_4498_935B_2F7F_6807_0635;
/// The UUID of the Slowness modifier (MCP `Potion.java:27`).
const SLOWNESS_UUID: u128 = 0x7107_DE5E_7CE8_4030_940E_514C_1F16_0890;

/// Speed adds 20 % per level (MCP `Potion.java:26`).
const SPEED_AMOUNT: f64 = 0.200_000_002_980_232_24;
/// Slowness removes 15 % per level (MCP `Potion.java:27`).
const SLOWNESS_AMOUNT: f64 = -0.150_000_005_960_464_48;

/// Modifier operations (MCP `ModifiableAttributeInstance.computeValue`).
const OP_ADD: i8 = 0;
const OP_MULTIPLY_BASE: i8 = 1;
const OP_MULTIPLY_TOTAL: i8 = 2;

/// What the movement code reads from the server state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementStats {
    /// The movement speed attribute without the sprint boost.
    pub speed: f64,
    /// The Jump Boost level, `0` without the effect.
    pub jump_boost: u8,
    /// Whether the player is blind.
    pub blinded: bool,
}

impl Default for MovementStats {
    fn default() -> Self {
        Self {
            speed: WALK_SPEED,
            jump_boost: 0,
            blinded: false,
        }
    }
}

/// The movement stats from the attributes and the effects of the server.
///
/// `walk_speed` is the base when the server sent no `generic.movementSpeed`.
#[must_use]
pub fn movement_stats(
    attributes: &Attributes,
    effects: &Effects,
    walk_speed: f64,
) -> MovementStats {
    let instance = attributes.get(MOVEMENT_SPEED);
    let base = instance.map_or(walk_speed, |instance| instance.base);

    let mut modifiers: BTreeMap<u128, (i8, f64)> = instance
        .map(AttributeInstance::modifiers)
        .into_iter()
        .flatten()
        .map(|modifier| (modifier.uuid, (modifier.operation, modifier.amount)))
        .collect();
    modifiers.remove(&SPRINT_UUID);
    for (potion, uuid, amount) in [
        (SPEED, SPEED_UUID, SPEED_AMOUNT),
        (SLOWNESS, SLOWNESS_UUID, SLOWNESS_AMOUNT),
    ] {
        if let Some(effect) = effects.get(potion) {
            let levels = f64::from(effect.amplifier) + 1.0;
            modifiers.insert(uuid, (OP_MULTIPLY_TOTAL, amount * levels));
        }
    }

    MovementStats {
        speed: compute(base, modifiers.values().copied()).max(0.0),
        jump_boost: effects
            .get(JUMP_BOOST)
            .map_or(0, |effect| (effect.amplifier.max(-1) + 1) as u8),
        blinded: effects.get(BLINDNESS).is_some(),
    }
}

/// `ModifiableAttributeInstance.computeValue` over `(operation, amount)`.
fn compute(base: f64, modifiers: impl Iterator<Item = (i8, f64)> + Clone) -> f64 {
    let sum_of = |operation: i8| {
        modifiers
            .clone()
            .filter(move |(op, _)| *op == operation)
            .map(|(_, amount)| amount)
    };
    let after_add = base + sum_of(OP_ADD).sum::<f64>();
    let mut total = after_add + sum_of(OP_MULTIPLY_BASE).map(|a| after_add * a).sum::<f64>();
    for amount in sum_of(OP_MULTIPLY_TOTAL) {
        total *= 1.0 + amount;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::entity_properties::{
        AttributeModifier, EntityProperties, EntityProperty,
    };
    use umicraft_protocol::packets::play::clientbound::EntityEffect;

    fn effect(id: i8, amplifier: i8) -> EntityEffect {
        EntityEffect {
            entity_id: 1,
            effect_id: id,
            amplifier,
            duration: 600,
            hide_particles: 0,
        }
    }

    fn effects(list: &[(i8, i8)]) -> Effects {
        let mut effects = Effects::default();
        for &(id, amplifier) in list {
            effects.apply(&effect(id, amplifier));
        }
        effects
    }

    fn attributes(base: f64, modifiers: Vec<AttributeModifier>) -> Attributes {
        let mut attributes = Attributes::default();
        attributes.apply(&EntityProperties {
            entity_id: 1,
            properties: vec![EntityProperty {
                key: MOVEMENT_SPEED.to_string(),
                value: base,
                modifiers,
            }],
        });
        attributes
    }

    fn modifier(uuid: u128, amount: f64) -> AttributeModifier {
        AttributeModifier {
            uuid,
            amount,
            operation: OP_MULTIPLY_TOTAL,
        }
    }

    #[test]
    fn without_anything_the_player_walks_at_the_default_speed() {
        let stats = movement_stats(&Attributes::default(), &Effects::default(), 0.1);
        assert_eq!(stats, MovementStats::default());
    }

    #[test]
    fn speed_and_slowness_scale_the_speed_by_their_levels() {
        let speed_two = movement_stats(&Attributes::default(), &effects(&[(1, 1)]), 0.1);
        assert!((speed_two.speed - 0.1 * 1.4).abs() < 1e-9);
        let slow_one = movement_stats(&Attributes::default(), &effects(&[(2, 0)]), 0.1);
        assert!((slow_one.speed - 0.1 * 0.85).abs() < 1e-9);
        let both = movement_stats(&Attributes::default(), &effects(&[(1, 0), (2, 0)]), 0.1);
        assert!((both.speed - 0.1 * 1.2 * 0.85).abs() < 1e-9);
    }

    #[test]
    fn a_modifier_the_server_also_sends_counts_once() {
        let from_server = attributes(0.1, vec![modifier(SPEED_UUID, SPEED_AMOUNT)]);
        let stats = movement_stats(&from_server, &effects(&[(1, 0)]), 0.1);
        assert!((stats.speed - 0.1 * 1.2).abs() < 1e-9);
    }

    #[test]
    fn the_server_sprint_modifier_is_left_to_the_movement_code() {
        let from_server = attributes(0.1, vec![modifier(SPRINT_UUID, 0.3)]);
        let stats = movement_stats(&from_server, &Effects::default(), 0.1);
        assert!((stats.speed - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_plugin_walk_speed_is_the_base() {
        let from_server = attributes(0.2, Vec::new());
        let stats = movement_stats(&from_server, &Effects::default(), 0.1);
        assert!((stats.speed - 0.2).abs() < 1e-9);
    }

    #[test]
    fn jump_boost_and_blindness_are_read_from_the_effects() {
        let stats = movement_stats(&Attributes::default(), &effects(&[(8, 1), (15, 0)]), 0.1);
        assert_eq!(stats.jump_boost, 2);
        assert!(stats.blinded);
    }
}
