// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attributes (max health, movement speed, ...) of the local player.
//!
//! Source: MCP `NetHandlerPlayClient.handleEntityProperties` (base value set,
//! modifiers replaced wholesale) and
//! `ModifiableAttributeInstance.computeValue` (the three modifier
//! operations). The vanilla range clamp of known attributes is not applied:
//! the server already sends consistent values and the consumers clamp what
//! they use.

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::entity_properties::{
    AttributeModifier, EntityProperties,
};

/// Modifier operation 0: add the amount to the base value.
const OP_ADD: i8 = 0;
/// Modifier operation 1: add `amount * value after op 0`.
const OP_MULTIPLY_BASE: i8 = 1;
/// Modifier operation 2: multiply the running total by `1 + amount`.
const OP_MULTIPLY_TOTAL: i8 = 2;

/// One attribute: a base value and its modifiers keyed by UUID.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttributeInstance {
    /// The base value.
    pub base: f64,
    modifiers: BTreeMap<u128, AttributeModifier>,
}

impl AttributeInstance {
    /// The modifiers, in UUID order.
    pub fn modifiers(&self) -> impl Iterator<Item = &AttributeModifier> {
        self.modifiers.values()
    }

    /// The final value, like `ModifiableAttributeInstance.computeValue`:
    /// operation 0 amounts are summed onto the base, operation 1 adds
    /// `amount * that sum`, operation 2 multiplies the total by `1 + amount`.
    /// Modifiers with an unknown operation are ignored.
    pub fn value(&self) -> f64 {
        let sum_of = |operation: i8| {
            self.modifiers
                .values()
                .filter(move |m| m.operation == operation)
                .map(|m| m.amount)
        };
        let after_add = self.base + sum_of(OP_ADD).sum::<f64>();
        let mut total = after_add;
        total += sum_of(OP_MULTIPLY_BASE).map(|a| after_add * a).sum::<f64>();
        for amount in sum_of(OP_MULTIPLY_TOTAL) {
            total *= 1.0 + amount;
        }
        total
    }
}

/// The attribute table of the local player, keyed by attribute name such as
/// `generic.movementSpeed`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attributes {
    map: BTreeMap<String, AttributeInstance>,
}

impl Attributes {
    /// Applies a properties packet: per listed attribute the base value is
    /// set and the modifier list replaced; unknown attributes are created.
    pub fn apply(&mut self, packet: &EntityProperties) {
        for property in &packet.properties {
            let instance = self.map.entry(property.key.clone()).or_default();
            instance.base = property.value;
            instance.modifiers = property.modifiers.iter().map(|m| (m.uuid, *m)).collect();
        }
    }

    /// The instance for an attribute name.
    pub fn get(&self, key: &str) -> Option<&AttributeInstance> {
        self.map.get(key)
    }

    /// The computed value of an attribute, if the server sent it.
    pub fn value(&self, key: &str) -> Option<f64> {
        self.get(key).map(AttributeInstance::value)
    }

    /// Forgets every attribute.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// The number of known attributes.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether no attribute is known.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use umicraft_protocol::packets::play::clientbound::entity_properties::EntityProperty;

    use super::*;

    fn modifier(uuid: u128, amount: f64, operation: i8) -> AttributeModifier {
        AttributeModifier {
            uuid,
            amount,
            operation,
        }
    }

    fn packet(key: &str, value: f64, modifiers: Vec<AttributeModifier>) -> EntityProperties {
        EntityProperties {
            entity_id: 1,
            properties: vec![EntityProperty {
                key: key.to_owned(),
                value,
                modifiers,
            }],
        }
    }

    #[test]
    fn without_modifiers_the_value_is_the_base() {
        let mut attributes = Attributes::default();
        attributes.apply(&packet("generic.maxHealth", 20.0, vec![]));
        assert_eq!(attributes.value("generic.maxHealth"), Some(20.0));
        assert_eq!(attributes.value("generic.unknown"), None);
    }

    #[test]
    fn the_three_operations_follow_compute_value() {
        // Sprinting: +30 % (operation 2) on a 0.1 base, plus an additive
        // +0.1 and a +50 % of the base-plus-additive (operation 1).
        let mut attributes = Attributes::default();
        attributes.apply(&packet(
            "generic.movementSpeed",
            0.1,
            vec![
                modifier(1, 0.1, 0),
                modifier(2, 0.5, 1),
                modifier(3, 0.3, 2),
            ],
        ));
        // (0.1 + 0.1) = 0.2; + 0.2 * 0.5 = 0.3; * 1.3 = 0.39.
        let value = attributes.value("generic.movementSpeed").unwrap();
        assert!((value - 0.39).abs() < 1e-12, "{value}");
    }

    #[test]
    fn a_new_packet_replaces_base_and_modifiers() {
        let mut attributes = Attributes::default();
        attributes.apply(&packet("a", 1.0, vec![modifier(1, 5.0, 0)]));
        attributes.apply(&packet("a", 2.0, vec![]));
        assert_eq!(attributes.value("a"), Some(2.0));
        assert_eq!(attributes.get("a").unwrap().modifiers().count(), 0);
    }

    #[test]
    fn a_duplicate_uuid_counts_once_and_unknown_operations_are_ignored() {
        let mut attributes = Attributes::default();
        attributes.apply(&packet(
            "a",
            1.0,
            vec![
                modifier(7, 1.0, 0),
                modifier(7, 2.0, 0),
                modifier(8, 9.0, 5),
            ],
        ));
        assert_eq!(attributes.value("a"), Some(3.0));
    }

    #[test]
    fn clear_forgets_everything() {
        let mut attributes = Attributes::default();
        attributes.apply(&packet("a", 1.0, vec![]));
        assert_eq!(attributes.len(), 1);
        attributes.clear();
        assert!(attributes.is_empty());
    }
}
