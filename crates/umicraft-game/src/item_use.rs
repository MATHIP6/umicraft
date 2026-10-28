// SPDX-License-Identifier: MIT OR Apache-2.0

//! Using the held item with the right mouse button, and dropping it.
//!
//! [`ItemUse`] follows MCP `Minecraft.runTick` / `rightClickMouse` and
//! `PlayerControllerMP.sendUseItem` / `onStoppedUsingItem`: a click on a block
//! tries the block first (activation, placement); a click that did nothing
//! locally falls through to the use of the item itself, which the server
//! answers by starting the use (eating, drinking, drawing a bow, blocking
//! with a sword). While an item is in use the client slows the player
//! ([`crate::input::RawInput::using_item`]), ignores the other buttons and
//! tells the server when the button is released.
//!
//! The 1.8 item registry does not exist yet (P3), so [`use_action`] and
//! [`is_activatable`] hold the few item and block ids they need, taken from
//! the reference classes cited on each table.

use umicraft_protocol::packets::play::serverbound::DiggingStatus;
use umicraft_protocol::packets::play::{Facing, ItemStack};
use umicraft_world::BlockPos;

use crate::gamemode::GameMode;
use crate::interaction::{Interaction, Outgoing};
use crate::raycast::RayHit;
use crate::shapes::{Aabb, BlockSource};

/// The ticks before a held right button clicks again
/// (MCP `Minecraft.rightClickDelayTimer`, `rightClickMouse` line 1552).
const RIGHT_CLICK_DELAY: u8 = 4;

/// The food level at which the player is no longer hungry
/// (MCP `FoodStats.needFood`: `foodLevel < 20`).
pub const FULL_FOOD: i32 = 20;

/// What using an item does while the button is held
/// (MCP `EnumAction`, reduced to the actions that slow the player).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseAction {
    /// Food (MCP `ItemFood`, `ItemSoup`, `ItemAppleGold`).
    Eat,
    /// A drinkable potion or a bucket of milk.
    Drink,
    /// A sword held up (MCP `ItemSword.getItemUseAction`).
    Block,
    /// A bow being drawn (MCP `ItemBow`).
    Bow,
}

/// The bow item id.
const BOW: i16 = 261;

/// The potion item id; a splash potion (bit 14 of the damage) is thrown
/// instead (MCP `ItemPotion.isSplash`).
const POTION: i16 = 373;

/// The milk bucket item id.
const MILK_BUCKET: i16 = 335;

/// The golden apple, the one food that is always edible
/// (MCP `Item.java:831`, `setAlwaysEdible`).
const GOLDEN_APPLE: i16 = 322;

/// The splash bit of a potion's damage value.
const SPLASH_BIT: i16 = 0x4000;

/// The sword item ids: wooden, stone, iron, diamond and golden
/// (MCP `Item.java:776-792`).
const SWORDS: [i16; 5] = [268, 272, 267, 276, 283];

/// The food item ids (MCP `Item.registerItems`: every `ItemFood`,
/// `ItemSoup` and `ItemSeedFood`).
const FOODS: &[i16] = &[
    260, 282, 297, 319, 320, 322, 349, 350, 357, 360, 363, 364, 365, 366, 367, 375, 391, 392, 393,
    394, 396, 400, 411, 412, 413, 423, 424,
];

/// What using `item` does, `None` for an item that starts no use.
///
/// MCP `Item.getItemUseAction`; the registry-less client only knows the
/// items that start a use on the client side (`EntityPlayer.setItemInUse`).
#[must_use]
pub fn use_action(item: &ItemStack) -> Option<UseAction> {
    match item.item_id {
        BOW => Some(UseAction::Bow),
        MILK_BUCKET => Some(UseAction::Drink),
        POTION if item.damage & SPLASH_BIT == 0 => Some(UseAction::Drink),
        id if SWORDS.contains(&id) => Some(UseAction::Block),
        id if FOODS.contains(&id) => Some(UseAction::Eat),
        _ => None,
    }
}

/// The blocks whose `onBlockActivated` returns `true` on the client, so that
/// a right click opens or toggles them instead of placing or using the item
/// (MCP `Block.onBlockActivated` overrides): dispenser, note block, bed,
/// chest, crafting table, furnaces, wooden doors, lever, buttons, cake,
/// repeaters, trapdoors, fences (leads), fence gates, enchanting table,
/// brewing stand, cauldron, dragon egg, ender chest, command block, beacon,
/// anvil, trapped chest, comparators, daylight sensors, hopper, dropper.
/// An iron door (71) returns `false`; a jukebox, a flower pot and TNT only
/// depend on the held item or a block entity, and are left out.
const ACTIVATABLE: &[u16] = &[
    23, 25, 26, 54, 58, 61, 62, 64, 69, 77, 85, 92, 93, 94, 96, 107, 113, 116, 117, 118, 122, 130,
    137, 138, 143, 145, 146, 149, 150, 151, 154, 158, 167, 178, 183, 184, 185, 186, 187, 188, 189,
    190, 191, 192, 193, 194, 195, 196, 197,
];

/// Whether a right click on the block with this id is consumed by the block.
///
/// MCP `PlayerControllerMP.onPlayerRightClick` line 404: sneaking with an
/// item in hand skips the activation (the caller checks that).
#[must_use]
pub fn is_activatable(block_id: u16) -> bool {
    ACTIVATABLE.contains(&block_id)
}

/// Whether the item is a block item: its id is the id of its block
/// (MCP `ItemBlock`, ids below 256).
#[must_use]
pub fn is_block_item(item: &ItemStack) -> bool {
    (1..256).contains(&item.item_id)
}

/// How the player drops what it holds (the `Q` key, `Ctrl+Q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropKind {
    /// One item of the stack (`Q`).
    One,
    /// The whole stack (`Ctrl+Q`).
    Stack,
}

/// The packet that drops the held item, `None` for a spectator.
///
/// MCP `Minecraft.runTick` line 2080 and `EntityPlayerSP.dropOneItem`: the
/// client only sends `Player Digging` with the drop status and an origin
/// position; the server removes the item and sends the slot back.
#[must_use]
pub fn drop_item(mode: GameMode, kind: DropKind) -> Option<Outgoing> {
    if mode == GameMode::Spectator {
        return None;
    }
    let status = match kind {
        DropKind::One => DiggingStatus::DropItem,
        DropKind::Stack => DiggingStatus::DropAllItems,
    };
    Some(Outgoing::Dig {
        status,
        block: BlockPos::new(0, 0, 0),
        face: Facing::Down,
    })
}

/// The item being used.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InUse {
    slot: u8,
    item_id: i16,
    damage: i16,
}

impl InUse {
    /// Whether `item` in `slot` is still the stack that is in use (MCP
    /// `EntityPlayer.onUpdate` compares the stack identity; a changed slot
    /// or a replaced stack ends the use).
    fn matches(&self, slot: u8, item: Option<&ItemStack>) -> bool {
        self.slot == slot
            && item.is_some_and(|item| item.item_id == self.item_id && item.damage == self.damage)
    }
}

/// The inputs of one tick of the use button.
#[derive(Debug, Clone, Copy)]
pub struct UseTick<'a, W: BlockSource + ?Sized> {
    /// The gamemode of the player.
    pub mode: GameMode,
    /// Whether the button went down during this tick.
    pub pressed: bool,
    /// Whether the button is down.
    pub held: bool,
    /// The block under the crosshair.
    pub target: Option<RayHit>,
    /// The loaded world.
    pub world: &'a W,
    /// The bounding box of the player, to refuse a placement inside it.
    pub player: Option<Aabb>,
    /// Whether the sneak key is held.
    pub sneaking: bool,
    /// The selected hotbar slot.
    pub slot: u8,
    /// The held item.
    pub item: Option<&'a ItemStack>,
    /// The food level of the player.
    pub food: i32,
    /// Whether the inventory holds an arrow.
    pub has_arrow: bool,
}

/// The result of one tick of the use button.
#[derive(Debug, Default, PartialEq)]
pub struct UseOutput {
    /// The packets to send, in order.
    pub packets: Vec<Outgoing>,
    /// Whether the arm swings (the click was consumed by a block).
    pub swing: bool,
}

/// The use state of the local player (MCP `EntityPlayer.itemInUse`).
#[derive(Debug, Default)]
pub struct ItemUse {
    using: Option<InUse>,
    /// Ticks before a held button clicks again.
    delay: u8,
}

impl ItemUse {
    /// Creates an idle state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an item is in use, which slows the player and blocks the
    /// other buttons (MCP `EntityPlayer.isUsingItem`).
    #[must_use]
    pub fn is_using(&self) -> bool {
        self.using.is_some()
    }

    /// Ends the use without telling the server: the server finished it
    /// (`Entity Status` 9, MCP `EntityPlayer.handleStatusUpdate`).
    pub fn finish(&mut self) {
        self.using = None;
    }

    /// Runs one tick of the use button.
    ///
    /// While an item is in use the only thing that happens is the release of
    /// the button, which sends `Player Digging` (release) and ends the use;
    /// a press is swallowed (MCP `Minecraft.runTick` lines 2098-2121).
    /// Otherwise a press, or the held button once the repeat delay ran out,
    /// clicks.
    pub fn tick<W: BlockSource + ?Sized>(
        &mut self,
        interaction: &mut Interaction,
        tick: &UseTick<'_, W>,
    ) -> UseOutput {
        self.delay = self.delay.saturating_sub(1);
        let mut output = UseOutput::default();
        if self
            .using
            .as_ref()
            .is_some_and(|using| !using.matches(tick.slot, tick.item))
        {
            self.using = None;
        }
        if self.using.is_some() {
            if !tick.held {
                self.using = None;
                output.packets.push(release_packet());
            }
            return output;
        }
        if tick.pressed || (tick.held && self.delay == 0) {
            self.click(interaction, tick, &mut output);
        }
        output
    }

    /// MCP `Minecraft.rightClickMouse`.
    fn click<W: BlockSource + ?Sized>(
        &mut self,
        interaction: &mut Interaction,
        tick: &UseTick<'_, W>,
        output: &mut UseOutput,
    ) {
        if interaction.digging_block().is_some() {
            return;
        }
        self.delay = RIGHT_CLICK_DELAY;
        let held = tick.item.filter(|item| item.item_id >= 0);
        if let Some(hit) = tick.target.filter(|hit| is_solid(tick.world, hit)) {
            let activated = tick.mode != GameMode::Spectator
                && (!tick.sneaking || held.is_none())
                && is_activatable(block_id(tick.world, &hit));
            let item = held.cloned().unwrap_or(ItemStack::EMPTY);
            let places = held.is_some_and(is_block_item) && tick.mode.can_place();
            if let Some(place) =
                interaction.place_block(tick.mode, &hit, tick.player.as_ref(), item)
            {
                output.packets.push(place);
                if activated || places {
                    output.swing = true;
                    return;
                }
            } else if activated {
                return;
            }
        }
        if let Some(item) = held.filter(|_| tick.mode != GameMode::Spectator) {
            output
                .packets
                .push(Outgoing::UseItem { item: item.clone() });
            self.start(tick, item);
        }
    }

    /// Starts the use of `item` when the player may use it
    /// (MCP `ItemFood.onItemRightClick`, `ItemBow.onItemRightClick`, ...).
    fn start<W: BlockSource + ?Sized>(&mut self, tick: &UseTick<'_, W>, item: &ItemStack) {
        let allowed = match use_action(item) {
            None => false,
            Some(UseAction::Eat) => {
                !tick.mode.invulnerable() && (tick.food < FULL_FOOD || item.item_id == GOLDEN_APPLE)
            }
            Some(UseAction::Bow) => tick.mode == GameMode::Creative || tick.has_arrow,
            Some(UseAction::Drink | UseAction::Block) => true,
        };
        if allowed {
            self.using = Some(InUse {
                slot: tick.slot,
                item_id: item.item_id,
                damage: item.damage,
            });
        }
    }
}

/// `Player Digging` with the release status
/// (MCP `PlayerControllerMP.onStoppedUsingItem`).
fn release_packet() -> Outgoing {
    Outgoing::Dig {
        status: DiggingStatus::ReleaseUseItem,
        block: BlockPos::new(0, 0, 0),
        face: Facing::Down,
    }
}

fn block_id<W: BlockSource + ?Sized>(world: &W, hit: &RayHit) -> u16 {
    world.block(hit.block.x, hit.block.y, hit.block.z).id()
}

/// Whether the block of the hit is not air (MCP `rightClickMouse` line 1578).
fn is_solid<W: BlockSource + ?Sized>(world: &W, hit: &RayHit) -> bool {
    block_id(world, hit) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raycast::Face;
    use glam::DVec3;
    use umicraft_world::BlockState;

    /// One stone block at the origin, air elsewhere.
    struct Stone;

    impl BlockSource for Stone {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            if (x, y, z) == (0, 0, 0) {
                BlockState::new(1, 0)
            } else {
                BlockState::AIR
            }
        }
    }

    fn item(id: i16, damage: i16) -> ItemStack {
        ItemStack {
            item_id: id,
            stack_size: 1,
            damage,
            nbt: None,
        }
    }

    fn hit() -> RayHit {
        RayHit {
            block: BlockPos::new(0, 0, 0),
            face: Face::Up,
            position: DVec3::new(0.5, 1.0, 0.5),
            distance: 2.0,
        }
    }

    fn tick<'a>(item: Option<&'a ItemStack>, pressed: bool, held: bool) -> UseTick<'a, Stone> {
        UseTick {
            mode: GameMode::Survival,
            pressed,
            held,
            target: None,
            world: &Stone,
            player: None,
            sneaking: false,
            slot: 0,
            item,
            food: 10,
            has_arrow: true,
        }
    }

    fn run(
        state: &mut ItemUse,
        interaction: &mut Interaction,
        tick: &UseTick<'_, Stone>,
    ) -> UseOutput {
        state.tick(interaction, tick)
    }

    #[test]
    fn the_item_use_table_follows_the_reference() {
        assert_eq!(use_action(&item(297, 0)), Some(UseAction::Eat));
        assert_eq!(use_action(&item(268, 0)), Some(UseAction::Block));
        assert_eq!(use_action(&item(261, 0)), Some(UseAction::Bow));
        assert_eq!(use_action(&item(335, 0)), Some(UseAction::Drink));
        assert_eq!(use_action(&item(373, 8193)), Some(UseAction::Drink));
        // A splash potion is thrown, not drunk.
        assert_eq!(use_action(&item(373, 0x4000 | 16)), None);
        assert_eq!(use_action(&item(1, 0)), None);
    }

    #[test]
    fn a_right_click_in_the_air_uses_the_item_and_slows_the_player() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let bread = item(297, 0);
        let output = run(
            &mut state,
            &mut interaction,
            &tick(Some(&bread), true, true),
        );
        assert_eq!(output.packets, vec![Outgoing::UseItem { item: bread }]);
        assert!(!output.swing);
        assert!(state.is_using());
    }

    #[test]
    fn releasing_the_button_sends_the_release() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let sword = item(267, 0);
        run(
            &mut state,
            &mut interaction,
            &tick(Some(&sword), true, true),
        );
        let held = run(
            &mut state,
            &mut interaction,
            &tick(Some(&sword), false, true),
        );
        assert!(held.packets.is_empty());
        assert!(state.is_using());
        let released = run(
            &mut state,
            &mut interaction,
            &tick(Some(&sword), false, false),
        );
        assert_eq!(released.packets.len(), 1);
        assert!(matches!(
            released.packets[0],
            Outgoing::Dig {
                status: DiggingStatus::ReleaseUseItem,
                ..
            }
        ));
        assert!(!state.is_using());
    }

    #[test]
    fn a_full_player_does_not_eat_except_a_golden_apple() {
        let mut full = tick(None, true, true);
        full.food = FULL_FOOD;
        let (bread, apple) = (item(297, 0), item(322, 0));
        for (stack, uses) in [(&bread, false), (&apple, true)] {
            let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
            full.item = Some(stack);
            let output = run(&mut state, &mut interaction, &full);
            // The packet is sent either way; only the local use differs.
            assert_eq!(output.packets.len(), 1);
            assert_eq!(state.is_using(), uses, "item {}", stack.item_id);
        }
    }

    #[test]
    fn a_bow_needs_an_arrow_outside_creative() {
        let bow = item(261, 0);
        let mut without = tick(Some(&bow), true, true);
        without.has_arrow = false;
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        run(&mut state, &mut interaction, &without);
        assert!(!state.is_using());
        without.mode = GameMode::Creative;
        run(&mut state, &mut interaction, &without);
        assert!(state.is_using());
    }

    #[test]
    fn a_held_button_repeats_every_fourth_tick() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let ball = item(332, 0);
        let sent: Vec<usize> = (0..9)
            .map(|index| {
                let pressed = index == 0;
                run(
                    &mut state,
                    &mut interaction,
                    &tick(Some(&ball), pressed, true),
                )
                .packets
                .len()
            })
            .collect();
        assert_eq!(sent, vec![1, 0, 0, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn changing_the_slot_ends_the_use_without_a_release() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let sword = item(267, 0);
        run(
            &mut state,
            &mut interaction,
            &tick(Some(&sword), true, true),
        );
        let mut moved = tick(Some(&sword), false, true);
        moved.slot = 1;
        let output = run(&mut state, &mut interaction, &moved);
        assert!(!output.packets.iter().any(|packet| matches!(
            packet,
            Outgoing::Dig {
                status: DiggingStatus::ReleaseUseItem,
                ..
            }
        )));
    }

    #[test]
    fn the_server_finishing_the_use_clears_it() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let bread = item(297, 0);
        run(
            &mut state,
            &mut interaction,
            &tick(Some(&bread), true, true),
        );
        state.finish();
        assert!(!state.is_using());
    }

    #[test]
    fn a_click_on_a_block_places_and_swings_without_using_the_item() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let cobble = item(4, 0);
        let mut click = tick(Some(&cobble), true, true);
        click.target = Some(hit());
        let output = run(&mut state, &mut interaction, &click);
        assert!(output.swing);
        assert_eq!(output.packets.len(), 1);
        assert!(matches!(output.packets[0], Outgoing::Place { .. }));
    }

    #[test]
    fn a_sword_on_a_block_falls_through_to_the_use() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let sword = item(267, 0);
        let mut click = tick(Some(&sword), true, true);
        click.target = Some(hit());
        let output = run(&mut state, &mut interaction, &click);
        assert!(!output.swing);
        assert!(matches!(output.packets[0], Outgoing::Place { .. }));
        assert!(matches!(output.packets[1], Outgoing::UseItem { .. }));
        assert!(state.is_using());
    }

    #[test]
    fn an_activatable_block_consumes_the_click_unless_sneaking_with_an_item() {
        struct Table;
        impl BlockSource for Table {
            fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
                if (x, y, z) == (0, 0, 0) {
                    BlockState::new(58, 0)
                } else {
                    BlockState::AIR
                }
            }
        }
        let sword = item(267, 0);
        let attempt = |sneaking: bool, held: Option<&ItemStack>| {
            let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
            let click = UseTick {
                mode: GameMode::Survival,
                pressed: true,
                held: true,
                target: Some(hit()),
                world: &Table,
                player: None,
                sneaking,
                slot: 0,
                item: held,
                food: 10,
                has_arrow: false,
            };
            (state.tick(&mut interaction, &click), state.is_using())
        };
        let (output, using) = attempt(false, Some(&sword));
        assert!(output.swing && !using);
        assert_eq!(output.packets.len(), 1);
        let (output, using) = attempt(true, Some(&sword));
        assert!(!output.swing && using);
        let (output, _) = attempt(true, None);
        assert!(output.swing);
    }

    #[test]
    fn a_spectator_never_uses_an_item() {
        let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
        let bread = item(297, 0);
        let mut click = tick(Some(&bread), true, true);
        click.mode = GameMode::Spectator;
        let output = run(&mut state, &mut interaction, &click);
        assert!(output.packets.is_empty());
        assert!(!state.is_using());
    }

    #[test]
    fn q_drops_one_item_and_ctrl_q_the_stack() {
        for (kind, status) in [
            (DropKind::One, DiggingStatus::DropItem),
            (DropKind::Stack, DiggingStatus::DropAllItems),
        ] {
            assert_eq!(
                drop_item(GameMode::Survival, kind),
                Some(Outgoing::Dig {
                    status,
                    block: BlockPos::new(0, 0, 0),
                    face: Facing::Down,
                })
            );
        }
        assert_eq!(drop_item(GameMode::Spectator, DropKind::One), None);
    }
}
