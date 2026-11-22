// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sounds that loop while their source lasts: the rolling of the
//! minecarts (MCP 1.8.9 `MovingSoundMinecart`, `MovingSoundMinecartRiding`).
//!
//! The reference starts these `ITickableSound`s once and sets their volume
//! and position every tick. Here the game thread lists, each tick, every loop
//! that should be heard with its current volume; the audio side starts the
//! new ones, updates the others and ends the ones that left the list, so a
//! removed cart or a dismount needs no message of its own.

use glam::Vec3;

use crate::entity::{EntityId, EntityKind, EntityRecord, EntityStore};

/// The rolling of a cart, heard around it (`minecraft:minecart.base`).
pub const MINECART_BASE: &str = "minecart.base";
/// The rolling heard inside the cart the player rides
/// (`minecraft:minecart.inside`), without attenuation.
pub const MINECART_INSIDE: &str = "minecart.inside";
/// Below this horizontal speed (blocks per tick) a cart is silent.
const ROLLING_SPEED: f32 = 0.01;
/// The key bit that tells the inside sound of a cart from its base sound.
const INSIDE_KEY: u64 = 1 << 32;

/// One looping sound and how it should sound this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopingSound {
    /// Stable while the sound lasts and unique among the loops of a tick.
    pub key: u64,
    /// The event name in `sounds.json`.
    pub name: &'static str,
    /// The source position; `None` for a sound heard without attenuation.
    pub position: Option<[f64; 3]>,
    /// `ISound.getVolume`, before the entry and category volumes.
    pub volume: f32,
}

/// Appends the rolling sounds of the tracked minecarts to `out`: one around
/// every cart, and one inside the cart `riding` when it is one.
pub fn minecart_loops(
    entities: &EntityStore,
    riding: Option<EntityId>,
    out: &mut Vec<LoopingSound>,
) {
    for cart in entities
        .entities()
        .values()
        .filter(|entity| entity.kind == EntityKind::Minecart)
    {
        let speed = rolling_speed(cart);
        let key = cart_key(cart.id);
        let position = cart.position;
        out.push(LoopingSound {
            key,
            name: MINECART_BASE,
            position: Some([position.x, position.y, position.z]),
            volume: base_volume(speed),
        });
        if riding == Some(cart.id) {
            out.push(LoopingSound {
                key: key | INSIDE_KEY,
                name: MINECART_INSIDE,
                position: None,
                volume: inside_volume(speed),
            });
        }
    }
}

/// The key of the base sound of cart `id`; its inside sound sets
/// [`INSIDE_KEY`] too.
fn cart_key(id: EntityId) -> u64 {
    u64::from(id.0 as u32)
}

/// The horizontal speed of a cart: the client's motion, the last velocity
/// the server sent (`setVelocity`; the client never changes it).
fn rolling_speed(cart: &EntityRecord) -> f32 {
    Vec3::new(cart.velocity.x, 0.0, cart.velocity.z).length()
}

/// `MovingSoundMinecart.update`: `clamp(speed, 0, 0.5) * 0.7` while rolling.
fn base_volume(speed: f32) -> f32 {
    if speed >= ROLLING_SPEED {
        speed.clamp(0.0, 0.5) * 0.7
    } else {
        0.0
    }
}

/// `MovingSoundMinecartRiding.update`: `clamp(speed, 0, 1) * 0.75` while
/// rolling.
fn inside_volume(speed: f32) -> f32 {
    if speed >= ROLLING_SPEED {
        speed.clamp(0.0, 1.0) * 0.75
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;

    fn store_with_cart(velocity: Vec3) -> EntityStore {
        let mut store = EntityStore::new();
        store.spawn_object(
            EntityId(5),
            10,
            DVec3::new(1.0, 64.0, 2.0),
            0.0,
            0.0,
            velocity,
            0,
        );
        store
    }

    fn loops(store: &EntityStore, riding: Option<EntityId>) -> Vec<LoopingSound> {
        let mut out = Vec::new();
        minecart_loops(store, riding, &mut out);
        out
    }

    #[test]
    fn a_resting_cart_has_a_silent_loop_and_a_rolling_one_is_heard() {
        let still = loops(&store_with_cart(Vec3::ZERO), None);
        assert_eq!(still.len(), 1);
        assert_eq!((still[0].name, still[0].volume), (MINECART_BASE, 0.0));
        assert_eq!(still[0].position, Some([1.0, 64.0, 2.0]));
        let rolling = loops(&store_with_cart(Vec3::new(0.3, 0.0, 0.4)), None);
        assert!(
            (rolling[0].volume - 0.5 * 0.7).abs() < 1e-6,
            "capped at 0.5"
        );
        let slow = loops(&store_with_cart(Vec3::new(0.2, -3.0, 0.0)), None);
        assert!(
            (slow[0].volume - 0.2 * 0.7).abs() < 1e-6,
            "the fall does not count"
        );
    }

    #[test]
    fn the_ridden_cart_adds_the_inside_sound_without_position() {
        let store = store_with_cart(Vec3::new(0.8, 0.0, 0.0));
        let inside = loops(&store, Some(EntityId(5)));
        assert_eq!(inside.len(), 2);
        assert_eq!(inside[1].name, MINECART_INSIDE);
        assert_eq!(inside[1].position, None);
        assert!((inside[1].volume - 0.8 * 0.75).abs() < 1e-6);
        assert_ne!(inside[0].key, inside[1].key);
        // Another vehicle has no inside sound.
        assert_eq!(loops(&store, Some(EntityId(6))).len(), 1);
    }

    #[test]
    fn other_entities_make_no_loop() {
        let mut store = EntityStore::new();
        store.spawn_object(EntityId(1), 1, DVec3::ZERO, 0.0, 0.0, Vec3::X, 0);
        assert!(loops(&store, None).is_empty());
    }
}
