// SPDX-License-Identifier: MIT OR Apache-2.0

//! The HUD of the session: the server-less HUD of the demos and the tick of
//! the status rows and bars.

use std::sync::mpsc::{self};

use umicraft_net::play::PlayEvent;
use umicraft_render::HudState;

use super::commands::CommandQueue;
use super::net::handle_net;
use super::state::Game;
use crate::hud::social as social_view;

/// The HUD a session would show after the server sent `events`, with the
/// title and the action bar advanced by `ticks` game ticks.
///
/// The server-less `--hud-demo` uses it: the packets go through the same
/// [`handle_net`] and the same views as in a real session, so the demo shows
/// what the player would see rather than a hand-built picture.
pub(crate) fn hud_after(events: Vec<PlayEvent>, own_name: &str, ticks: u32) -> HudState {
    let mut game = Game::new();
    game.own_name = own_name.to_string();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    for event in events {
        handle_net(&mut game, event, &mut queue, &frames);
    }
    social_view::tick_boss_bar(&mut game.social.boss_bar, &game.entities);
    for _ in 0..ticks {
        game.social.title.tick();
        game.social.action_bar.tick();
        social_view::tick_boss_bar(&mut game.social.boss_bar, &game.entities);
    }
    game.hud()
}

/// Advances the status rows and bars of the HUD by one tick and asks for a new
/// HUD when they changed (they shake, blink and fade with the tick).
pub(super) fn tick_hud_view(game: &mut Game) {
    let head_in_water = match (game.state.as_ref(), game.world.as_ref()) {
        (Some(state), Some(world)) => {
            crate::scene::fog::medium_at(world, state.player().eye_position())
                == umicraft_render::FogMedium::Water
        }
        _ => false,
    };
    let facts = crate::hud::player::HudFacts {
        mode: game.gamemode,
        health: game.vitals.health,
        food: game.vitals.food,
        armor: game.vitals.armor,
        experience: game.vitals.experience,
        level: game.vitals.level,
        effects: game.server.effects(),
        attributes: game.server.attributes(),
        vehicle: game.vehicle,
        entities: &game.entities,
        head_in_water,
        held: game.windows.selected_item(),
        jump: game.controls.input.jump,
    };
    let bouncing = game.hotbar.popups.tick();
    if game.hud_view.tick(&facts) || bouncing {
        game.hud_dirty = true;
    }
}
