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
//!
//! The game mode lives on the game, not on the player, so it travels through a
//! [`SharedGameMode`]: the game stores its mode there before the physics run, a
//! layer's change lands there, and the game applies it afterwards as a local
//! prediction (ADR-0094). Flight is a plain player field; the game reports its
//! change to the server once, after the step.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use glam::DVec3;
use umicraft_game::entity::EntityStore;
use umicraft_game::gamemode::GameMode;
use umicraft_game::input::InputIntent;
use umicraft_game::physics::{PhysicsOverride, Player};
use umicraft_modapi_sys::{
    UmiPhysicsInput, UmiPlayerPhysics, UMI_INPUT_JUMP, UMI_INPUT_SNEAK, UMI_INPUT_SPRINT,
    UMI_PHYSICS_ALLOW_FLYING, UMI_PHYSICS_FLYING, UMI_PHYSICS_IN_WATER, UMI_PHYSICS_ON_GROUND,
    UMI_PHYSICS_SNEAKING, UMI_PHYSICS_SPRINTING,
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

/// The game mode the physics layers read and may change.
///
/// The game stores its mode here before every physics run (a tick or the replay
/// of a correction); a layer's write-back overwrites it, and the game compares
/// it with its own mode afterwards. Shared because the override lives inside the
/// [`umicraft_game::GameState`], out of the game's reach.
#[derive(Debug, Clone, Default)]
pub(super) struct SharedGameMode(Arc<AtomicU8>);

impl SharedGameMode {
    /// The mode currently held.
    pub(super) fn get(&self) -> GameMode {
        GameMode::from_wire(self.0.load(Ordering::Relaxed))
    }

    /// Holds `mode`.
    pub(super) fn set(&self, mode: GameMode) {
        self.0.store(mode.id(), Ordering::Relaxed);
    }
}

/// The game mode of an ABI value; `None` for a value outside `UMI_GAME_MODE_*`,
/// which `GameMode::from_wire` would silently turn into survival.
fn mode_from_abi(raw: u32) -> Option<GameMode> {
    u8::try_from(raw)
        .ok()
        .filter(|id| *id <= GameMode::Spectator.id())
        .map(GameMode::from_wire)
}

/// The mods' `player_physics` layers as the game's physics override.
pub(super) struct ModPhysics {
    chain: PhysicsChain,
    mode: SharedGameMode,
}

impl ModPhysics {
    /// Wraps the chain of a session; `mode` is the game's shared game mode.
    pub(super) fn new(chain: PhysicsChain, mode: SharedGameMode) -> Self {
        Self { chain, mode }
    }
}

/// Runs `chain` (the mods' layers, or a stand-in in tests) around `vanilla`:
/// converts the player to the ABI state, lets the layers change it and writes
/// the changes back. `chain` gets the state, the input and the built-in step as
/// the innermost call.
pub(super) fn run_layers(
    chain: impl FnOnce(&mut UmiPlayerPhysics, &UmiPhysicsInput, &mut dyn FnMut(&mut UmiPlayerPhysics)),
    mode: &SharedGameMode,
    player: &mut Player,
    intent: &InputIntent,
    vanilla: &mut dyn FnMut(&mut Player),
) {
    let input = input_of(intent);
    let mut state = state_of(player, mode.get());
    chain(&mut state, &input, &mut |state| {
        write_back(state, player, mode);
        vanilla(player);
        *state = state_of(player, mode.get());
    });
    write_back(&state, player, mode);
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
        let chain = &self.chain;
        run_layers(
            |state, input, vanilla| chain.step(state, input, vanilla),
            &self.mode,
            player,
            intent,
            vanilla,
        );
    }
}

/// The ABI view of the player's movement state.
fn state_of(player: &Player, mode: GameMode) -> UmiPlayerPhysics {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    UmiPlayerPhysics {
        size: size_of::<UmiPlayerPhysics>() as u32,
        flags: flag(player.on_ground, UMI_PHYSICS_ON_GROUND)
            | flag(player.sprinting, UMI_PHYSICS_SPRINTING)
            | flag(player.sneaking, UMI_PHYSICS_SNEAKING)
            | flag(player.flying, UMI_PHYSICS_FLYING)
            | flag(player.in_water, UMI_PHYSICS_IN_WATER)
            | flag(player.allow_flying, UMI_PHYSICS_ALLOW_FLYING),
        position: player.position.to_array(),
        velocity: player.velocity.to_array(),
        yaw: player.yaw,
        pitch: player.pitch,
        fall_distance: player.fall_distance,
        fly_speed: player.fly_speed,
        game_mode: u32::from(mode.id()),
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
///
/// Flight is applied even when the abilities do not allow it: the server is the
/// authority and corrects a flight it refuses (ADR-0094). A game mode outside
/// `UMI_GAME_MODE_*` is ignored.
fn write_back(state: &UmiPlayerPhysics, player: &mut Player, mode: &SharedGameMode) {
    let before = state_of(player, mode.get());
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
    if (state.flags ^ before.flags) & UMI_PHYSICS_FLYING != 0 {
        player.flying = state.flags & UMI_PHYSICS_FLYING != 0;
    }
    if state.game_mode != before.game_mode {
        if let Some(predicted) = mode_from_abi(state.game_mode) {
            mode.set(predicted);
        }
    }
}

fn same_bits(a: &[f64; 3], b: &[f64; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits())
}

#[cfg(test)]
mod tests {
    use umicraft_modapi_sys::{
        UMI_GAME_MODE_ADVENTURE, UMI_GAME_MODE_CREATIVE, UMI_GAME_MODE_SPECTATOR,
        UMI_GAME_MODE_SURVIVAL,
    };

    use super::*;

    fn player() -> Player {
        Player::new(DVec3::new(0.5, 64.0, 0.5))
    }

    /// Runs a stand-in layer that edits the state before the built-in step.
    fn run(player: &mut Player, mode: &SharedGameMode, edit: impl FnOnce(&mut UmiPlayerPhysics)) {
        run_layers(
            |state, _input, vanilla| {
                edit(state);
                vanilla(state);
            },
            mode,
            player,
            &InputIntent::walking(0.0),
            &mut |_| {},
        );
    }

    #[test]
    fn an_untouched_state_writes_nothing_back() {
        let mut player = player();
        player.velocity = DVec3::new(0.1, -0.0784, 0.2);
        player.bounds.min.x += 1e-9; // a box a vanilla step derived on its own
        let bounds = player.bounds;
        let mode = SharedGameMode::default();
        let state = state_of(&player, mode.get());
        write_back(&state, &mut player, &mode);
        assert_eq!(player.bounds, bounds);
        assert_eq!(state_of(&player, mode.get()), state);
        assert_eq!(mode.get(), GameMode::Survival);
    }

    #[test]
    fn a_changed_field_is_written_and_the_others_kept() {
        let mut player = player();
        let mode = SharedGameMode::default();
        let mut state = state_of(&player, mode.get());
        state.velocity[1] = 0.42;
        state.flags |= UMI_PHYSICS_SPRINTING;
        write_back(&state, &mut player, &mode);
        assert_eq!(player.velocity.y, 0.42);
        assert!(player.sprinting);
        assert!(!player.flying);
        assert_eq!(player.position, DVec3::new(0.5, 64.0, 0.5));
    }

    #[test]
    fn the_state_carries_the_flight_facts_and_the_mode() {
        let mut flier = player();
        flier.allow_flying = true;
        flier.flying = true;
        flier.fly_speed = 0.1;
        let state = state_of(&flier, GameMode::Adventure);
        assert_ne!(state.flags & UMI_PHYSICS_ALLOW_FLYING, 0);
        assert_ne!(state.flags & UMI_PHYSICS_FLYING, 0);
        assert_eq!(state.fly_speed, 0.1);
        assert_eq!(state.game_mode, UMI_GAME_MODE_ADVENTURE);
        let state = state_of(&player(), GameMode::Survival);
        assert_eq!(
            state.flags & (UMI_PHYSICS_ALLOW_FLYING | UMI_PHYSICS_FLYING),
            0
        );
    }

    #[test]
    fn the_game_mode_ids_match_the_abi_constants() {
        let ids = [
            (GameMode::Survival, UMI_GAME_MODE_SURVIVAL),
            (GameMode::Creative, UMI_GAME_MODE_CREATIVE),
            (GameMode::Adventure, UMI_GAME_MODE_ADVENTURE),
            (GameMode::Spectator, UMI_GAME_MODE_SPECTATOR),
        ];
        for (mode, raw) in ids {
            assert_eq!(u32::from(mode.id()), raw);
            assert_eq!(mode_from_abi(raw), Some(mode));
        }
    }

    #[test]
    fn an_unknown_game_mode_value_is_not_a_mode() {
        assert_eq!(mode_from_abi(4), None);
        assert_eq!(mode_from_abi(256), None);
        assert_eq!(mode_from_abi(u32::MAX), None);
    }

    #[test]
    fn a_layer_can_start_and_stop_flying_even_without_the_ability() {
        let mut player = player();
        assert!(!player.allow_flying);
        let mode = SharedGameMode::default();
        run(&mut player, &mode, |state| {
            state.flags |= UMI_PHYSICS_FLYING;
        });
        assert!(player.flying, "the server is the authority, not the host");
        run(&mut player, &mode, |state| {
            state.flags &= !UMI_PHYSICS_FLYING;
        });
        assert!(!player.flying);
    }

    #[test]
    fn the_read_only_fields_are_ignored() {
        let mut player = player();
        let mode = SharedGameMode::default();
        let fly_speed = player.fly_speed;
        run(&mut player, &mode, |state| {
            state.flags |= UMI_PHYSICS_ALLOW_FLYING | UMI_PHYSICS_SNEAKING | UMI_PHYSICS_IN_WATER;
            state.fly_speed = 1.0;
        });
        assert!(!player.allow_flying && !player.sneaking && !player.in_water);
        assert_eq!(player.fly_speed, fly_speed);
    }

    #[test]
    fn a_layer_change_of_the_game_mode_lands_in_the_shared_mode() {
        let mut player = player();
        let mode = SharedGameMode::default();
        run(&mut player, &mode, |state| {
            assert_eq!(state.game_mode, UMI_GAME_MODE_SURVIVAL);
            state.game_mode = UMI_GAME_MODE_CREATIVE;
        });
        assert_eq!(mode.get(), GameMode::Creative);
    }

    #[test]
    fn an_invalid_game_mode_value_changes_nothing() {
        let mut player = player();
        let mode = SharedGameMode::default();
        mode.set(GameMode::Adventure);
        run(&mut player, &mode, |state| state.game_mode = 9);
        assert_eq!(mode.get(), GameMode::Adventure);
    }

    #[test]
    fn a_layer_sees_the_flight_the_inner_step_made() {
        let mut player = player();
        let mode = SharedGameMode::default();
        let mut seen = false;
        run_layers(
            |state, _input, vanilla| {
                vanilla(state);
                seen = state.flags & UMI_PHYSICS_FLYING != 0;
            },
            &mode,
            &mut player,
            &InputIntent::walking(0.0),
            &mut |player| player.flying = true,
        );
        assert!(seen && player.flying);
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
