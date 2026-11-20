// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread lends to the external mods (F3.6, ADR-0086): a read-only
//! view of the world for their queries, and the `player_physics` service.
//!
//! [`GameWorld`] answers `world_block` and `world_entities` while the game thread
//! runs a mod callback inside [`umicraft_modhost::with_world`]. [`ModPhysics`] is
//! the [`PhysicsOverride`] that runs the mods' physics layers around the vanilla
//! step. It converts the player to the ABI state and back, and writes back only the
//! fields a layer changed: re-deriving the box from the position would move the
//! player by a rounding step, so a layer that changes nothing must leave the
//! ADR-0076 trajectory bit for bit as it was.

use glam::DVec3;
use umicraft_game::entity::EntityStore;
use umicraft_game::input::InputIntent;
use umicraft_game::physics::{PhysicsOverride, Player};
use umicraft_modapi_sys::{
    UmiPhysicsInput, UmiPlayerPhysics, UMI_INPUT_JUMP, UMI_INPUT_SNEAK, UMI_INPUT_SPRINT,
    UMI_PHYSICS_FLYING, UMI_PHYSICS_IN_WATER, UMI_PHYSICS_ON_GROUND, UMI_PHYSICS_SNEAKING,
    UMI_PHYSICS_SPRINTING,
};
use umicraft_modhost::{EntityView, PhysicsChain, WorldQuery};
use umicraft_world::{BlockPos, World};

/// The world and the entities as the mods' queries see them.
pub(super) struct GameWorld<'a> {
    /// The world of the session; `None` before `Join Game`.
    pub(super) world: Option<&'a World>,
    /// The entities of the world (the local player is not among them).
    pub(super) entities: &'a EntityStore,
}

impl WorldQuery for GameWorld<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let world = self.world?;
        let pos = BlockPos::new(x, y, z);
        world.chunk(pos.chunk())?;
        Some(world.block(pos).wire())
    }

    fn visit_entities(&self, visit: &mut dyn FnMut(&EntityView<'_>) -> bool) {
        for record in self.entities.entities().values() {
            let name = record
                .profile
                .as_ref()
                .map(|profile| &*profile.name)
                .or(record.name_tag.as_deref())
                .unwrap_or_default();
            let view = EntityView {
                id: record.id.0,
                kind: record.kind.name(),
                position: record.position.to_array(),
                yaw: record.yaw,
                pitch: record.pitch,
                name,
            };
            if !visit(&view) {
                return;
            }
        }
    }
}

/// Runs `f` with the world lent to the mods' queries (`with_world`).
pub(super) fn lend<R>(world: Option<&World>, entities: &EntityStore, f: impl FnOnce() -> R) -> R {
    umicraft_modhost::with_world(&GameWorld { world, entities }, f)
}

/// The mods' `player_physics` layers as the game's physics override.
pub(super) struct ModPhysics {
    chain: PhysicsChain,
}

impl ModPhysics {
    /// Wraps the chain of a session.
    pub(super) fn new(chain: PhysicsChain) -> Self {
        Self { chain }
    }
}

impl PhysicsOverride for ModPhysics {
    fn step(
        &mut self,
        player: &mut Player,
        intent: &InputIntent,
        vanilla: &mut dyn FnMut(&mut Player),
    ) {
        if !self.chain.is_active() {
            vanilla(player);
            return;
        }
        let input = input_of(intent);
        let mut state = state_of(player);
        self.chain.step(&mut state, &input, &mut |state| {
            write_back(state, player);
            vanilla(player);
            *state = state_of(player);
        });
        write_back(&state, player);
    }
}

/// The ABI view of the player's movement state.
fn state_of(player: &Player) -> UmiPlayerPhysics {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    UmiPlayerPhysics {
        size: size_of::<UmiPlayerPhysics>() as u32,
        flags: flag(player.on_ground, UMI_PHYSICS_ON_GROUND)
            | flag(player.sprinting, UMI_PHYSICS_SPRINTING)
            | flag(player.sneaking, UMI_PHYSICS_SNEAKING)
            | flag(player.flying, UMI_PHYSICS_FLYING)
            | flag(player.in_water, UMI_PHYSICS_IN_WATER),
        position: player.position.to_array(),
        velocity: player.velocity.to_array(),
        yaw: player.yaw,
        pitch: player.pitch,
        fall_distance: player.fall_distance,
    }
}

/// The ABI view of the tick's input.
fn input_of(intent: &InputIntent) -> UmiPhysicsInput {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    UmiPhysicsInput {
        size: size_of::<UmiPhysicsInput>() as u32,
        flags: flag(intent.jump, UMI_INPUT_JUMP)
            | flag(intent.sneak, UMI_INPUT_SNEAK)
            | flag(intent.sprint, UMI_INPUT_SPRINT),
        forward: intent.forward,
        strafe: intent.strafe,
        yaw: intent.yaw,
        pitch: intent.pitch,
    }
}

/// Copies the writable fields of `state` that differ from `player` into it. The
/// comparison is on the bits, so a field a layer did not touch is never rewritten
/// (`set_position` rebuilds the box, which a vanilla step derived differently).
fn write_back(state: &UmiPlayerPhysics, player: &mut Player) {
    let before = state_of(player);
    if !same_bits(&state.position, &before.position) {
        player.set_position(DVec3::from_array(state.position));
    }
    if !same_bits(&state.velocity, &before.velocity) {
        player.velocity = DVec3::from_array(state.velocity);
    }
    if state.fall_distance.to_bits() != before.fall_distance.to_bits() {
        player.fall_distance = state.fall_distance;
    }
    player.on_ground = state.flags & UMI_PHYSICS_ON_GROUND != 0;
    player.sprinting = state.flags & UMI_PHYSICS_SPRINTING != 0;
}

fn same_bits(a: &[f64; 3], b: &[f64; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_state_writes_nothing_back() {
        let mut player = Player::new(DVec3::new(0.5, 64.0, 0.5));
        player.velocity = DVec3::new(0.1, -0.0784, 0.2);
        player.bounds.min.x += 1e-9; // a box a vanilla step derived on its own
        let bounds = player.bounds;
        let state = state_of(&player);
        write_back(&state, &mut player);
        assert_eq!(player.bounds, bounds);
        assert_eq!(state_of(&player), state);
    }

    #[test]
    fn a_changed_field_is_written_and_the_others_kept() {
        let mut player = Player::new(DVec3::new(0.5, 64.0, 0.5));
        let mut state = state_of(&player);
        state.velocity[1] = 0.42;
        state.flags |= UMI_PHYSICS_SPRINTING;
        // A read-only flag is ignored.
        state.flags |= UMI_PHYSICS_FLYING;
        write_back(&state, &mut player);
        assert_eq!(player.velocity.y, 0.42);
        assert!(player.sprinting);
        assert!(!player.flying);
        assert_eq!(player.position, DVec3::new(0.5, 64.0, 0.5));
    }

    #[test]
    fn the_input_flags_follow_the_keys() {
        let mut intent = InputIntent::walking(90.0);
        intent.jump = true;
        intent.sprint = true;
        let input = input_of(&intent);
        assert_eq!(input.flags, UMI_INPUT_JUMP | UMI_INPUT_SPRINT);
        assert_eq!((input.forward, input.yaw), (intent.forward, 90.0));
    }
}
