// SPDX-License-Identifier: MIT OR Apache-2.0

//! Game-thread side of the creative inventory: a [`CreativeEvent`] from the
//! window drives the [`CreativeScreen`](umicraft_game::inventory::creative::CreativeScreen),
//! every slot change it reports becomes a `Creative Inventory Action` packet
//! and the new state is published to the render thread.
//!
//! This module is a child of [`crate::game`] so it can reach the game state
//! and the command queue without widening their visibility.

use std::sync::mpsc::SyncSender;

use umicraft_game::gamemode::GameMode;
use umicraft_game::inventory::creative::CreativeAction;

use super::{publish_inventory, sync_armor, CommandQueue, Game, RenderEvent};
use crate::creative_input::{CreativeEvent, SlotClick};
use crate::creative_view::CreativeView;
use crate::network::ClientCommand;
use crate::tooltip_draw::resolve;

/// Applies `event` to the creative screen and the local inventory, queues the
/// `Creative Inventory Action` packets and publishes the new state.
///
/// Events are ignored outside creative mode: the server would not honour the
/// packets and the screen is not shown then.
pub(super) fn apply_creative_event(
    game: &mut Game,
    event: CreativeEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    if game.gamemode != GameMode::Creative {
        return;
    }
    for action in dispatch(game, event) {
        queue.push(ClientCommand::CreativeInventoryAction {
            slot: action.slot,
            stack: action.stack,
        });
    }
    sync_armor(game);
    publish_inventory(game, frames);
    publish_creative(game, frames);
}

/// Publishes the creative window snapshot to the render thread.
fn publish_creative(game: &Game, frames: &SyncSender<RenderEvent>) {
    let view = CreativeView::of(&game.creative);
    let _ = frames.try_send(RenderEvent::Creative(Box::new(view)));
}

/// Runs the event on the screen and returns the actions to send.
fn dispatch(game: &mut Game, event: CreativeEvent) -> Vec<CreativeAction> {
    let Game {
        creative, windows, ..
    } = game;
    let resolve = &resolve;
    match event {
        CreativeEvent::Open => creative.reopen(&resolve),
        CreativeEvent::Click(click) => return click_slot(game, click),
        CreativeEvent::Drag {
            button,
            slots,
            fallback,
        } => {
            let wanted: Vec<i16> = slots
                .iter()
                .filter_map(|slot| slot.window_slot())
                .filter_map(|slot| i16::try_from(slot).ok())
                .collect();
            // Vanilla only splits once the pointer has crossed two slots that
            // take the stack; otherwise the release is a plain click.
            if windows.drag_targets(&wanted).len() > 1 {
                return creative.drag_split(windows, button, &slots);
            }
            return fallback.map_or_else(Vec::new, |click| click_slot(game, click));
        }
        CreativeEvent::DoubleClick(slot) => return creative.double_click(windows, slot),
        CreativeEvent::Wheel(delta) => creative.wheel(delta),
        CreativeEvent::ScrollPress((x, y)) => {
            creative.press_scrollbar(x, y);
        }
        CreativeEvent::ScrollDrag(y) => creative.drag_scrollbar(y),
        CreativeEvent::ScrollRelease => creative.release_scrollbar(),
        CreativeEvent::SelectTab(tab) => creative.select_tab(tab, &resolve),
        CreativeEvent::OpenSearch => creative.open_search(&resolve),
        CreativeEvent::Number {
            hovered,
            key,
            typed,
        } => match creative.number_key(windows, hovered, key) {
            Some(actions) => return actions,
            None => {
                if let Some(ch) = typed {
                    creative.search_input(ch, &resolve);
                }
            }
        },
        CreativeEvent::Drop { hovered, whole } => {
            return creative.drop_key(windows, hovered, whole);
        }
        CreativeEvent::Char(ch) => {
            creative.search_input(ch, &resolve);
        }
        CreativeEvent::Backspace => {
            creative.search_backspace(&resolve);
        }
    }
    Vec::new()
}

/// One click on the screen.
fn click_slot(game: &mut Game, click: SlotClick) -> Vec<CreativeAction> {
    game.creative
        .click(&mut game.windows, click.target, click.button, click.shift)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{sync_channel, Receiver};

    use umicraft_game::inventory::creative::{CreativeSlot, CreativeTab, MouseButton};

    use super::super::tests::placed_game;
    use super::*;

    fn creative_game() -> Game {
        let mut game = placed_game();
        game.gamemode = GameMode::Creative;
        game
    }

    fn run(
        game: &mut Game,
        events: Vec<CreativeEvent>,
    ) -> (Vec<ClientCommand>, Receiver<RenderEvent>) {
        let mut queue = CommandQueue::default();
        let (frames, rx) = sync_channel(64);
        for event in events {
            apply_creative_event(game, event, &mut queue, &frames);
        }
        (queue.ordered.into_iter().collect(), rx)
    }

    fn click(target: CreativeSlot) -> CreativeEvent {
        CreativeEvent::Click(SlotClick {
            target: Some(target),
            button: MouseButton::Left,
            shift: false,
        })
    }

    fn last_view(rx: &Receiver<RenderEvent>) -> Option<CreativeView> {
        rx.try_iter()
            .filter_map(|event| match event {
                RenderEvent::Creative(view) => Some(*view),
                _ => None,
            })
            .last()
    }

    #[test]
    fn picking_from_the_grid_and_placing_in_the_hotbar_sends_the_slot() {
        let mut game = creative_game();
        let events = vec![
            CreativeEvent::Open,
            click(CreativeSlot::Grid(0)),
            click(CreativeSlot::Hotbar(0)),
        ];
        let (commands, _rx) = run(&mut game, events);
        assert!(game.windows.cursor().is_none());
        let placed = game.windows.inventory().slot(36).cloned();
        assert!(placed.is_some());
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            ClientCommand::CreativeInventoryAction { slot: 36, stack } if Some(stack.item_id) == placed.map(|s| s.item_id)
        ));
    }

    #[test]
    fn events_are_ignored_outside_creative_mode() {
        let mut game = placed_game();
        let (commands, rx) = run(&mut game, vec![click(CreativeSlot::Grid(0))]);
        assert!(commands.is_empty());
        assert!(game.windows.cursor().is_none());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn typing_on_the_search_tab_filters_the_published_grid() {
        let mut game = creative_game();
        let events = vec![
            CreativeEvent::Open,
            CreativeEvent::SelectTab(CreativeTab::Search),
            CreativeEvent::Char('x'),
            CreativeEvent::Char('y'),
            CreativeEvent::Backspace,
        ];
        let (_, rx) = run(&mut game, events);
        let view = last_view(&rx).expect("a snapshot is published");
        assert_eq!(view.tab, CreativeTab::Search);
        assert_eq!(view.search, "x");
    }

    #[test]
    fn a_digit_that_is_no_shortcut_is_typed_on_the_search_tab() {
        let mut game = creative_game();
        let digit = CreativeEvent::Number {
            hovered: None,
            key: 4,
            typed: Some('5'),
        };
        let events = vec![CreativeEvent::SelectTab(CreativeTab::Search), digit];
        let (_, rx) = run(&mut game, events);
        assert_eq!(last_view(&rx).map(|v| v.search), Some("5".to_owned()));
    }

    #[test]
    fn a_drag_over_one_slot_falls_back_to_the_plain_click() {
        let mut game = creative_game();
        let hotbar = CreativeSlot::Hotbar(1);
        let events = vec![
            click(CreativeSlot::Grid(0)),
            CreativeEvent::Drag {
                button: MouseButton::Left,
                slots: vec![hotbar],
                fallback: Some(SlotClick {
                    target: Some(hotbar),
                    button: MouseButton::Left,
                    shift: false,
                }),
            },
        ];
        let (commands, _rx) = run(&mut game, events);
        assert!(game.windows.inventory().slot(37).is_some());
        assert_eq!(commands.len(), 1);
    }

    #[test]
    fn a_wheel_step_moves_the_published_handle() {
        let mut game = creative_game();
        let (_, rx) = run(&mut game, vec![CreativeEvent::Wheel(-1)]);
        let view = last_view(&rx).expect("a snapshot is published");
        assert!(view.needs_scrollbar);
        assert!(view.handle_offset > 0);
    }
}
