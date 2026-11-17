// SPDX-License-Identifier: MIT OR Apache-2.0

//! Game-thread side of the container screens beyond slot clicks: enchanting,
//! beacon, villager and anvil (MCP `GuiEnchantment`, `GuiBeacon`,
//! `GuiMerchant`, `GuiRepair`).
//!
//! This module is a child of [`crate::game`] so it can reach the game state
//! and the command queue without widening their visibility.

use std::sync::mpsc::SyncSender;

use umicraft_game::gamemode::GameMode;
use umicraft_game::inventory::payload;
use umicraft_game::inventory::WindowKind;
use umicraft_ui::screens::{beacon_effect, enchant_row_available, Hotspot};

use super::{publish_inventory, CommandQueue, Game, RenderEvent};
use crate::inventory_input::ContainerAction;
use crate::inventory_screen::{screen_state, ScreenFacts};
use crate::network::ClientCommand;

/// The window property holding the primary power of a beacon.
const BEACON_PRIMARY: i16 = 1;
/// The window property holding the secondary power of a beacon.
const BEACON_SECONDARY: i16 = 2;
/// The last tier of powers, which sets the secondary effect.
const SECONDARY_TIER: u8 = 3;

/// Applies a container screen action: local state first, then the packet
/// the reference client sends.
pub(super) fn apply_container_action(
    game: &mut Game,
    action: ContainerAction,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    match action {
        ContainerAction::Press(hotspot) => press(game, hotspot, queue),
        ContainerAction::Rename(name) => rename(&name, queue),
    }
    publish_inventory(game, frames);
}

/// Stores the villager offers of an `MC|TrList` payload.
pub(super) fn apply_trade_list(game: &mut Game, data: &[u8], frames: &SyncSender<RenderEvent>) {
    match payload::parse_trades(data) {
        Ok(list) => {
            game.windows.set_trades(list);
            publish_inventory(game, frames);
        }
        Err(error) => tracing::warn!(%error, "bad MC|TrList payload"),
    }
}

fn press(game: &mut Game, hotspot: Hotspot, queue: &mut CommandQueue) {
    let Some(window_id) = game.windows.container().map(|c| c.window_id) else {
        return;
    };
    match hotspot {
        Hotspot::Enchant(row) => enchant(game, window_id, row, queue),
        Hotspot::TradePrevious => select_trade(game, -1, queue),
        Hotspot::TradeNext => select_trade(game, 1, queue),
        Hotspot::BeaconPower { tier, slot } => choose_power(game, tier, slot),
        Hotspot::BeaconConfirm => confirm_beacon(game, window_id, queue),
        Hotspot::BeaconCancel => close(game, window_id, queue),
    }
}

/// `GuiEnchantment.mouseClicked`: only a row the player can pay is sent.
fn enchant(game: &Game, window_id: u8, row: u8, queue: &mut CommandQueue) {
    let view = game.windows.view();
    let facts = ScreenFacts {
        xp_level: game.level,
        creative: game.gamemode == GameMode::Creative,
        ..ScreenFacts::default()
    };
    let state = screen_state(&view, &facts, None);
    let is_table = game
        .windows
        .container()
        .is_some_and(|c| c.kind == WindowKind::EnchantingTable);
    if is_table && state.filled[0] && enchant_row_available(&state, row) {
        queue.push(ClientCommand::EnchantItem {
            window_id: window_id as i8,
            enchantment: row as i8,
        });
    }
}

/// `GuiMerchant.actionPerformed`: the arrows move the offer and tell the
/// server.
fn select_trade(game: &mut Game, step: i32, queue: &mut CommandQueue) {
    if let Some(index) = game.windows.step_trade(step) {
        queue.push(ClientCommand::PluginMessage {
            channel: payload::TRADE_SELECT,
            data: payload::trade_selection(index as i32),
        });
    }
}

/// `GuiBeacon.actionPerformed`: a power button sets the primary field, or
/// the secondary one in the last tier, if the pyramid is tall enough.
fn choose_power(game: &mut Game, tier: u8, slot: u8) {
    let Some(container) = game.windows.container() else {
        return;
    };
    let property = |id: usize| container.properties.get(id).copied().map(i32::from);
    let (levels, primary) = (property(0).unwrap_or(-1), property(1).unwrap_or(0));
    if i32::from(tier) >= levels {
        return;
    }
    let Some(effect) = beacon_effect(tier, slot, primary).and_then(|e| i16::try_from(e).ok())
    else {
        return;
    };
    let window_id = container.window_id;
    let target = if tier == SECONDARY_TIER {
        BEACON_SECONDARY
    } else {
        BEACON_PRIMARY
    };
    game.windows.apply_property(window_id, target, effect);
}

/// `GuiBeacon` "done": needs a payment and a primary power; sends both
/// powers and closes the screen.
fn confirm_beacon(game: &mut Game, window_id: u8, queue: &mut CommandQueue) {
    let Some(container) = game.windows.container() else {
        return;
    };
    let property = |id: i16| {
        container
            .properties
            .get(id as usize)
            .copied()
            .map_or(0, i32::from)
    };
    let (primary, secondary) = (property(BEACON_PRIMARY), property(BEACON_SECONDARY));
    let paid = container
        .container_slots
        .first()
        .is_some_and(Option::is_some);
    if container.kind != WindowKind::Beacon || !paid || primary <= 0 {
        return;
    }
    queue.push(ClientCommand::PluginMessage {
        channel: payload::BEACON,
        data: payload::beacon(primary, secondary),
    });
    close(game, window_id, queue);
}

fn close(game: &mut Game, window_id: u8, queue: &mut CommandQueue) {
    game.windows.close_window(window_id);
    queue.push(ClientCommand::CloseWindow {
        window_id: window_id as i8,
    });
}

/// `GuiRepair.renameItem`.
fn rename(name: &str, queue: &mut CommandQueue) {
    match payload::item_name(name) {
        Ok(data) => queue.push(ClientCommand::PluginMessage {
            channel: payload::ITEM_NAME,
            data,
        }),
        Err(error) => tracing::warn!(%error, "anvil name not sent"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use umicraft_game::inventory::payload::{Trade, TradeList};
    use umicraft_protocol::packets::play::ItemStack;

    use super::super::tests::placed_game;
    use super::*;

    fn stack(item_id: i16, size: i8) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: size,
            damage: 0,
            nbt: None,
        }
    }

    fn run(game: &mut Game, action: ContainerAction) -> Vec<ClientCommand> {
        let mut queue = CommandQueue::default();
        let (frames, _rx) = sync_channel(8);
        apply_container_action(game, action, &mut queue, &frames);
        queue.ordered.into_iter().collect()
    }

    fn open(game: &mut Game, id: u8, kind: WindowKind, slots: u8, items: &[(usize, ItemStack)]) {
        game.windows.open_window(id, kind, String::new(), slots);
        let own = kind.implicit_slot_count().unwrap_or(usize::from(slots));
        let mut all = vec![ItemStack::EMPTY; own + 36];
        for (index, item) in items {
            all[*index] = item.clone();
        }
        game.windows.apply_window_items(id, &all);
    }

    fn table(game: &mut Game, costs: [i16; 3], lapis: i8) {
        let enchantable = (0, stack(276, 1));
        let dye = ItemStack {
            damage: 4,
            ..stack(351, lapis)
        };
        open(
            game,
            3,
            WindowKind::EnchantingTable,
            0,
            &[enchantable, (1, dye)],
        );
        for (property, cost) in costs.into_iter().enumerate() {
            game.windows.apply_property(3, property as i16, cost);
        }
    }

    #[test]
    fn an_affordable_row_sends_enchant_item() {
        let mut game = placed_game();
        game.level = 12;
        table(&mut game, [10, 20, 30], 3);
        let sent = run(&mut game, ContainerAction::Press(Hotspot::Enchant(0)));
        assert_eq!(
            sent,
            vec![ClientCommand::EnchantItem {
                window_id: 3,
                enchantment: 0
            }]
        );
    }

    #[test]
    fn a_row_without_level_or_lapis_sends_nothing() {
        let mut game = placed_game();
        game.level = 12;
        table(&mut game, [10, 20, 30], 1);
        assert!(run(&mut game, ContainerAction::Press(Hotspot::Enchant(1))).is_empty());
        assert!(run(&mut game, ContainerAction::Press(Hotspot::Enchant(2))).is_empty());
    }

    #[test]
    fn enchanting_needs_an_item_in_the_table() {
        let mut game = placed_game();
        game.level = 50;
        table(&mut game, [10, 20, 30], 3);
        game.windows.apply_set_slot(3, 0, ItemStack::EMPTY);
        assert!(run(&mut game, ContainerAction::Press(Hotspot::Enchant(0))).is_empty());
    }

    fn trade(n: i8) -> Trade {
        Trade {
            buy: stack(emerald_cost(n), n),
            buy_second: None,
            sell: stack(276, 1),
            disabled: false,
            uses: 0,
            max_uses: 7,
        }
    }

    fn emerald_cost(n: i8) -> i16 {
        i16::from(n) + 100
    }

    #[test]
    fn the_villager_arrows_select_offers_and_stop_at_the_ends() {
        let mut game = placed_game();
        open(&mut game, 4, WindowKind::Villager, 3, &[]);
        game.windows.set_trades(TradeList {
            window_id: 4,
            trades: vec![trade(1), trade(2)],
        });
        let next = run(&mut game, ContainerAction::Press(Hotspot::TradeNext));
        assert_eq!(
            next,
            vec![ClientCommand::PluginMessage {
                channel: payload::TRADE_SELECT,
                data: vec![0, 0, 0, 1]
            }]
        );
        assert!(run(&mut game, ContainerAction::Press(Hotspot::TradeNext)).is_empty());
        let back = run(&mut game, ContainerAction::Press(Hotspot::TradePrevious));
        assert_eq!(back.len(), 1);
        assert!(run(&mut game, ContainerAction::Press(Hotspot::TradePrevious)).is_empty());
    }

    #[test]
    fn trades_for_another_window_are_ignored() {
        let mut game = placed_game();
        open(&mut game, 4, WindowKind::Villager, 3, &[]);
        game.windows.set_trades(TradeList {
            window_id: 9,
            trades: vec![trade(1)],
        });
        assert!(game.windows.container().unwrap().trades.is_empty());
    }

    fn beacon(game: &mut Game, levels: i16, paid: bool) {
        let payment = [(0, stack(266, 1))];
        open(
            game,
            5,
            WindowKind::Beacon,
            1,
            if paid { &payment } else { &[] },
        );
        game.windows.apply_property(5, 0, levels);
    }

    #[test]
    fn beacon_powers_set_the_fields_and_done_sends_them() {
        let mut game = placed_game();
        beacon(&mut game, 4, true);
        // Speed (tier 0) then regeneration (tier 3) as the secondary.
        assert!(run(
            &mut game,
            ContainerAction::Press(Hotspot::BeaconPower { tier: 0, slot: 0 })
        )
        .is_empty());
        assert!(run(
            &mut game,
            ContainerAction::Press(Hotspot::BeaconPower { tier: 3, slot: 0 })
        )
        .is_empty());
        let sent = run(&mut game, ContainerAction::Press(Hotspot::BeaconConfirm));
        assert_eq!(
            sent,
            vec![
                ClientCommand::PluginMessage {
                    channel: payload::BEACON,
                    data: vec![0, 0, 0, 1, 0, 0, 0, 10]
                },
                ClientCommand::CloseWindow { window_id: 5 },
            ]
        );
        assert!(game.windows.container().is_none());
    }

    #[test]
    fn a_tier_above_the_pyramid_does_nothing() {
        let mut game = placed_game();
        beacon(&mut game, 1, true);
        run(
            &mut game,
            ContainerAction::Press(Hotspot::BeaconPower { tier: 1, slot: 0 }),
        );
        assert_eq!(game.windows.container().unwrap().properties, vec![1]);
    }

    #[test]
    fn done_needs_a_payment_and_a_primary_power() {
        let mut game = placed_game();
        beacon(&mut game, 4, false);
        run(
            &mut game,
            ContainerAction::Press(Hotspot::BeaconPower { tier: 0, slot: 0 }),
        );
        assert!(run(&mut game, ContainerAction::Press(Hotspot::BeaconConfirm)).is_empty());
        let mut game = placed_game();
        beacon(&mut game, 4, true);
        assert!(run(&mut game, ContainerAction::Press(Hotspot::BeaconConfirm)).is_empty());
    }

    #[test]
    fn cancel_closes_the_window() {
        let mut game = placed_game();
        beacon(&mut game, 4, true);
        let sent = run(&mut game, ContainerAction::Press(Hotspot::BeaconCancel));
        assert_eq!(sent, vec![ClientCommand::CloseWindow { window_id: 5 }]);
    }

    #[test]
    fn a_rename_sends_the_name_channel() {
        let mut game = placed_game();
        let sent = run(&mut game, ContainerAction::Rename("Bob".into()));
        assert_eq!(
            sent,
            vec![ClientCommand::PluginMessage {
                channel: payload::ITEM_NAME,
                data: vec![3, b'B', b'o', b'b']
            }]
        );
    }
}
