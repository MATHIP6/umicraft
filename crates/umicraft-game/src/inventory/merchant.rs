// SPDX-License-Identifier: MIT OR Apache-2.0

//! The villager window's result slot, predicted as the 1.8 client does it
//! (MCP `InventoryMerchant.resetRecipeAndSlots`, `MerchantRecipeList
//! .canRecipeBeUsed`, `SlotMerchantResult.onPickupFromSlot`).
//!
//! The server never sends the result of an accepted click: it handles the
//! click with `isChangingQuantityOnly` set, so `detectAndSendChanges` only
//! updates its own copy (`NetHandlerPlayServer.processClickWindow`). The
//! client fills the result from the offers itself, and empties the wanted
//! items itself when the result is taken; without that, the result shows up
//! only after the next click, which the server then rejects.

use umicraft_protocol::packets::play::ItemStack;

use super::payload::Trade;
use super::stack::count;

/// The first wanted item, the second one and the result.
const FIRST: usize = 0;
const SECOND: usize = 1;
/// The result slot of the window (`ContainerMerchant`, slot 2).
pub const RESULT: usize = 2;

/// `resetRecipeAndSlots`: fills the result slot of `own` (the villager
/// window's own slots) from the offer the wanted items match, or empties it.
/// The two wanted slots are tried in both orders.
pub fn reset(own: &mut [Option<ItemStack>], trades: &[Trade], selected: usize) {
    let result = current_trade(own, trades, selected).map(|index| trades[index].sell.clone());
    if let Some(slot) = own.get_mut(RESULT) {
        *slot = result;
    }
}

/// The offer the wanted items pay for (`currentRecipe`), if any and not used
/// up.
#[must_use]
pub fn current_trade(
    own: &[Option<ItemStack>],
    trades: &[Trade],
    selected: usize,
) -> Option<usize> {
    let slot = |index: usize| own.get(index).and_then(Option::as_ref);
    let (first, second) = match (slot(FIRST), slot(SECOND)) {
        (None, second) => (second?, None),
        (Some(first), second) => (first, second),
    };
    let usable = |index: Option<usize>| index.filter(|&i| !trades[i].disabled);
    usable(matching(trades, first, second, selected))
        .or_else(|| usable(matching(trades, second?, Some(first), selected)))
}

/// `canRecipeBeUsed`: with a selected offer past the first, only that one is
/// checked; otherwise the first offer that the items pay for.
fn matching(
    trades: &[Trade],
    first: &ItemStack,
    second: Option<&ItemStack>,
    selected: usize,
) -> Option<usize> {
    if selected > 0 && selected < trades.len() {
        return pays(&trades[selected], first, second).then_some(selected);
    }
    trades.iter().position(|trade| pays(trade, first, second))
}

/// Whether `first` and `second` pay for `trade`: the same items, enough of
/// each, and no second item unless the offer wants one.
fn pays(trade: &Trade, first: &ItemStack, second: Option<&ItemStack>) -> bool {
    let first_pays = same_kind(first, &trade.buy) && count(first) >= count(&trade.buy);
    let second_pays = match (&trade.buy_second, second) {
        (None, None) => true,
        (Some(wanted), Some(given)) => same_kind(given, wanted) && count(given) >= count(wanted),
        _ => false,
    };
    first_pays && second_pays
}

/// `MerchantRecipeList.func_181078_a`: the same item and damage, and the
/// wanted tags present. The reference accepts any stack whose tags contain
/// the wanted ones; here the tags must be equal, which is the same for the
/// offers of 1.8 villagers (they want plain items).
fn same_kind(given: &ItemStack, wanted: &ItemStack) -> bool {
    given.item_id == wanted.item_id
        && given.damage == wanted.damage
        && (wanted.nbt.is_none() || given.nbt == wanted.nbt)
}

/// `SlotMerchantResult.onPickupFromSlot`: the result was taken, so the
/// wanted items are paid (`doTrade`, both orders), the offer counts one more
/// use, and the result is filled again from what is left.
pub fn take_result(own: &mut [Option<ItemStack>], trades: &mut [Trade], selected: usize) {
    let Some(index) = current_trade(own, trades, selected) else {
        return;
    };
    let paid = pay(own, &trades[index], FIRST, SECOND) || pay(own, &trades[index], SECOND, FIRST);
    if paid {
        let trade = &mut trades[index];
        // `NpcMerchant.useRecipe`: `incrementToolUses`.
        trade.uses = trade.uses.saturating_add(1);
        trade.disabled = trade.uses >= trade.max_uses;
    }
    reset(own, trades, selected);
}

/// `doTrade`: takes the price out of the slots `first` and `second` when they
/// hold the wanted items (by item only, as the reference checks).
fn pay(own: &mut [Option<ItemStack>], trade: &Trade, first: usize, second: usize) -> bool {
    let item = |index: usize| own.get(index).and_then(Option::as_ref).map(|s| s.item_id);
    if item(first) != Some(trade.buy.item_id) {
        return false;
    }
    match (&trade.buy_second, item(second)) {
        (Some(wanted), Some(given)) if given == wanted.item_id => {
            shrink(own, first, count(&trade.buy));
            shrink(own, second, count(wanted));
            true
        }
        (None, None) => {
            shrink(own, first, count(&trade.buy));
            true
        }
        _ => false,
    }
}

/// Removes `amount` items from a slot, emptying it at zero or below.
fn shrink(own: &mut [Option<ItemStack>], index: usize, amount: i32) {
    let Some(slot) = own.get_mut(index) else {
        return;
    };
    if let Some(stack) = slot {
        let left = count(stack) - amount;
        if left <= 0 {
            *slot = None;
        } else {
            stack.stack_size = left as i8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMERALD: i16 = 388;
    const WHEAT: i16 = 296;
    const BREAD: i16 = 297;

    fn stack(item_id: i16, size: i8) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: size,
            damage: 0,
            nbt: None,
        }
    }

    fn trade(buy: ItemStack, buy_second: Option<ItemStack>, sell: ItemStack) -> Trade {
        Trade {
            buy,
            buy_second,
            sell,
            disabled: false,
            uses: 0,
            max_uses: 7,
        }
    }

    /// 20 wheat for an emerald, then an emerald for 4 bread.
    fn farmer() -> Vec<Trade> {
        vec![
            trade(stack(WHEAT, 20), None, stack(EMERALD, 1)),
            trade(stack(EMERALD, 1), None, stack(BREAD, 4)),
        ]
    }

    /// Regression: putting the wanted item in left the result empty until the
    /// next click.
    #[test]
    fn the_wanted_items_fill_the_result() {
        let mut own = vec![Some(stack(WHEAT, 20)), None, None];
        reset(&mut own, &farmer(), 0);
        assert_eq!(own[RESULT], Some(stack(EMERALD, 1)));
    }

    #[test]
    fn too_few_items_or_the_second_slot_alone_follow_the_reference() {
        let mut own = vec![Some(stack(WHEAT, 19)), None, Some(stack(EMERALD, 1))];
        reset(&mut own, &farmer(), 0);
        assert_eq!(own[RESULT], None);
        // A stack in the second slot alone counts as the first.
        let mut own = vec![None, Some(stack(EMERALD, 3)), None];
        reset(&mut own, &farmer(), 0);
        assert_eq!(own[RESULT], Some(stack(BREAD, 4)));
    }

    #[test]
    fn a_selected_offer_only_checks_itself() {
        let mut own = vec![Some(stack(WHEAT, 20)), None, None];
        reset(&mut own, &farmer(), 1);
        assert_eq!(own[RESULT], None);
    }

    #[test]
    fn a_used_up_offer_gives_nothing() {
        let mut trades = farmer();
        trades[0].disabled = true;
        let mut own = vec![Some(stack(WHEAT, 64)), None, None];
        reset(&mut own, &trades, 0);
        assert_eq!(own[RESULT], None);
    }

    #[test]
    fn taking_the_result_pays_and_offers_the_next_one() {
        let mut trades = farmer();
        let mut own = vec![Some(stack(WHEAT, 45)), None, Some(stack(EMERALD, 1))];
        own[RESULT] = None;
        take_result(&mut own, &mut trades, 0);
        assert_eq!(own[0], Some(stack(WHEAT, 25)));
        assert_eq!(own[RESULT], Some(stack(EMERALD, 1)));
        assert_eq!(trades[0].uses, 1);
        take_result(&mut own, &mut trades, 0);
        assert_eq!(own[0], Some(stack(WHEAT, 5)));
        assert_eq!(own[RESULT], None);
    }

    #[test]
    fn a_trade_with_two_items_pays_both() {
        let book = 340;
        let mut trades = vec![trade(
            stack(book, 1),
            Some(stack(EMERALD, 5)),
            stack(403, 1),
        )];
        let mut own = vec![Some(stack(EMERALD, 5)), Some(stack(book, 1)), None];
        reset(&mut own, &trades, 0);
        assert_eq!(own[RESULT].as_ref().map(|s| s.item_id), Some(403));
        take_result(&mut own, &mut trades, 0);
        assert_eq!((own[0].clone(), own[1].clone()), (None, None));
        assert_eq!(own[RESULT], None);
    }

    #[test]
    fn the_last_use_disables_the_offer() {
        let mut trades = farmer();
        trades[0].uses = 6;
        let mut own = vec![Some(stack(WHEAT, 64)), None, None];
        take_result(&mut own, &mut trades, 0);
        assert!(trades[0].disabled);
        assert_eq!(own[RESULT], None);
    }
}
