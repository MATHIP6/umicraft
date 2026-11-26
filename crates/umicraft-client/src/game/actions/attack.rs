// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attack button: digging, breaking and hitting entities.

use umicraft_game::digging::{enchantment_level, Digger, AQUA_AFFINITY, HASTE, MINING_FATIGUE};
use umicraft_game::entity::EntityId;
use umicraft_game::gamemode::GameMode;
use umicraft_game::interaction::{AttackTick, Outgoing};
use umicraft_protocol::packets::play::clientbound::BlockBreakAnim;
use umicraft_world::{BlockPos, BlockState};

use super::swing_effects;
use crate::game::combat;
use crate::game::commands::{command_of, CommandQueue};
use crate::game::particles::chip_dug_block;
use crate::game::state::Game;

/// The block ids of water (MCP `Blocks.flowing_water`, `Blocks.water`).
const WATER_FLOWING: u16 = 8;
const WATER_STILL: u16 = 9;

/// A `Block Break Animation` stage that removes the cracks.
const CLEAR_BREAK_STAGE: u8 = 255;

/// The 1.7 block hit (ADR-0079): with the 1.7 Animations mod, holding attack
/// at a block while an item is in use swings the arm, as 1.7.10
/// `Minecraft.func_147115_a` did; 1.8.9 `sendClickBlockToController` skips
/// it while an item is in use. Only the local animation runs: no dig and no
/// swing packet leaves, so the server sees a 1.8 client. Adventure mode never
/// swings (1.7 swung there only with the right tool for the block).
fn swing_while_using(game: &mut Game) {
    let applies = game.legacy.block_hit
        && game.item_use.is_using()
        && game.controls.input.attack
        && !game.spectating()
        && !game.dead()
        && game.gamemode != GameMode::Adventure
        && combat::pointed(game).block().is_some();
    if applies {
        let effects = swing_effects(game);
        game.hand.swing(effects);
        game.body.animation.swing_item();
    }
}

/// Runs one tick of the attack button (`umicraft_game::interaction`).
///
/// The client removes the blocks it breaks at once, like MCP
/// `PlayerControllerMP.onPlayerDestroyBlock`; the server confirms or
/// restores them with a block change.
pub(in crate::game) fn run_attack(game: &mut Game, queue: &mut CommandQueue) {
    let presses = std::mem::take(&mut game.controls.attack_presses);
    // MCP `Minecraft.runTick` lines 2098-2121: the attack button does
    // nothing while an item is in use.
    if game.spectating() || game.dead() || game.item_use.is_using() {
        if presses > 0 {
            tracing::debug!(
                spectating = game.spectating(),
                dead = game.dead(),
                using_item = game.item_use.is_using(),
                "attack click ignored"
            );
        }
        swing_while_using(game);
        return;
    }
    let pointed = combat::pointed(game);
    if presses > 0 {
        combat::log_click(game, &pointed);
    }
    let target = pointed.block();
    let entity = pointed.entity().map(|hit| hit.id);
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let player = state.player();
    let eye = player.eye_position();
    let water = world.block(BlockPos::new(
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    ));
    let effects = game.server.effects();
    let amplifier = |id: u8| effects.get(id).map(|effect| effect.amplifier);
    let digger = Digger {
        held: game.windows.selected_item(),
        haste: amplifier(HASTE),
        fatigue: amplifier(MINING_FATIGUE),
        in_water: matches!(water.id(), WATER_FLOWING | WATER_STILL),
        aqua_affinity: game
            .windows
            .helmet()
            .and_then(|helmet| helmet.nbt.as_deref())
            .is_some_and(|nbt| enchantment_level(nbt, AQUA_AFFINITY) > 0),
        on_ground: player.on_ground,
    };
    let output = game.interaction.attack(&AttackTick {
        mode: game.gamemode,
        presses,
        held: game.controls.input.attack,
        target,
        entity,
        world,
        digger,
    });
    let effects = swing_effects(game);
    if let Some(state) = game.state.as_mut() {
        state.queue_sound_cues(output.sounds);
    }
    for outgoing in output.packets {
        match outgoing {
            Outgoing::Swing => {
                game.hand.swing(effects);
                game.body.animation.swing_item();
            }
            Outgoing::Attack { entity } => combat::after_attack(game, EntityId(entity)),
            _ => {}
        }
        queue.push(command_of(outgoing));
    }
    if let Some(world) = game.world.as_mut() {
        for position in output.broken {
            tracing::info!(block = ?position, "breaking a block");
            let broken = world.block(position);
            game.dirty
                .extend(world.apply_block_change(position, BlockState::AIR));
            game.particles
                .add_block_destroy_effects(position, broken, [1.0; 3]);
        }
    }
    chip_dug_block(game, target.as_ref());
    if let Some(progress) = output.progress {
        game.server.apply_block_break_anim(&BlockBreakAnim {
            entity_id: game.entity_id,
            x: progress.block.x,
            y: progress.block.y,
            z: progress.block.z,
            stage: u8::try_from(progress.stage).unwrap_or(CLEAR_BREAK_STAGE),
        });
    }
}
