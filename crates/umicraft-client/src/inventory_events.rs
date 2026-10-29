// SPDX-License-Identifier: MIT OR Apache-2.0

//! Game-thread side of the inventory input: an [`InventoryAction`] from the
//! window becomes predicted clicks and `Click Window` packets.
//!
//! This module is a child of [`crate::game`] so it can reach the game state
//! and the command queue without widening their visibility.

use std::sync::mpsc::SyncSender;

use umicraft_game::gamemode::GameMode;
use umicraft_game::inventory::click::{drag_sequence, ClickRequest};

use super::{publish_inventory, sync_armor, CommandQueue, Game, RenderEvent};
use crate::inventory_input::InventoryAction;
use crate::network::ClientCommand;

/// Predicts the clicks of `action` on the local window, queues the matching
/// `Click Window` packets and publishes the new window state.
///
/// A drag is first reduced to the slots that can take the cursor stack, like
/// vanilla does while the pointer crosses them. When none can, the plain click
/// the release stands for is sent instead.
pub(super) fn apply_inventory_action(
    game: &mut Game,
    action: InventoryAction,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    let creative = game.gamemode == GameMode::Creative;
    for request in requests(game, action) {
        let command = game.windows.click(request, creative);
        queue.push(ClientCommand::ClickWindow {
            window_id: command.window_id,
            slot: command.slot,
            button: command.button,
            action_number: command.action_number,
            mode: command.mode,
            clicked_item: command.clicked_item,
        });
    }
    sync_armor(game);
    publish_inventory(game, frames);
}

/// The `Click Window` requests an action stands for, in send order.
fn requests(game: &mut Game, action: InventoryAction) -> Vec<ClickRequest> {
    match action {
        InventoryAction::Click(request) => vec![request],
        InventoryAction::Drag {
            slots,
            button,
            fallback,
        } => {
            let targets = game.windows.drag_targets(&slots);
            if targets.is_empty() {
                fallback.into_iter().collect()
            } else {
                drag_sequence(&targets, button)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use umicraft_game::inventory::click::DragButton;
    use umicraft_game::inventory::WindowKind;
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

    /// A game with a 27-slot chest (window 7) holding 6 stone in slot 0.
    fn game_with_chest() -> Game {
        let mut game = placed_game();
        game.windows
            .open_window(7, WindowKind::Chest, "Chest".into(), 27);
        let mut items = vec![ItemStack::EMPTY; 63];
        items[0] = stack(1, 6);
        game.windows.apply_window_items(7, &items);
        game
    }

    fn run(game: &mut Game, action: InventoryAction) -> Vec<ClientCommand> {
        let mut queue = CommandQueue::default();
        let (frames, _rx) = sync_channel(8);
        apply_inventory_action(game, action, &mut queue, &frames);
        queue.ordered.into_iter().collect()
    }

    #[test]
    fn a_chest_click_queues_a_click_window_and_predicts_it() {
        let mut game = game_with_chest();
        let commands = run(&mut game, InventoryAction::Click(ClickRequest::left(0)));
        assert_eq!(
            commands,
            vec![ClientCommand::ClickWindow {
                window_id: 7,
                slot: 0,
                button: 0,
                action_number: 1,
                mode: 0,
                clicked_item: stack(1, 6),
            }]
        );
        assert_eq!(game.windows.cursor(), Some(&stack(1, 6)));
    }

    #[test]
    fn a_drag_sends_start_one_add_per_accepting_slot_and_end() {
        let mut game = game_with_chest();
        run(&mut game, InventoryAction::Click(ClickRequest::left(0)));
        // Slot 99 does not exist and is dropped; 1 and 2 take the stack.
        let commands = run(
            &mut game,
            InventoryAction::Drag {
                slots: vec![1, 99, 2],
                button: DragButton::Left,
                fallback: None,
            },
        );
        let sent: Vec<(i16, i8, i8)> = commands
            .iter()
            .filter_map(|c| match c {
                ClientCommand::ClickWindow {
                    slot, button, mode, ..
                } => Some((*slot, *button, *mode)),
                _ => None,
            })
            .collect();
        assert_eq!(sent, vec![(-999, 0, 5), (1, 1, 5), (2, 1, 5), (-999, 2, 5)]);
        assert!(game.windows.cursor().is_none());
    }

    #[test]
    fn a_drag_nothing_accepts_falls_back_to_the_plain_click() {
        let mut game = game_with_chest();
        run(&mut game, InventoryAction::Click(ClickRequest::left(0)));
        let commands = run(
            &mut game,
            InventoryAction::Drag {
                slots: vec![99],
                button: DragButton::Left,
                fallback: Some(ClickRequest::left(3)),
            },
        );
        assert!(matches!(
            commands.as_slice(),
            [ClientCommand::ClickWindow {
                slot: 3,
                mode: 0,
                ..
            }]
        ));
    }
}
