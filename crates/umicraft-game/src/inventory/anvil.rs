// SPDX-License-Identifier: MIT OR Apache-2.0

//! The anvil's result and cost, predicted as the 1.8 client does it (MCP
//! `ContainerRepair.updateRepairOutput`, `updateItemName` and the result
//! slot's `canTakeStack` and `onPickupFromSlot`).
//!
//! As with the villager ([`super::merchant`]), the server does not send the
//! result of an accepted click, and it sends the cost (window property 0)
//! only when the window opens. The client computes both itself whenever the
//! two input slots or the name change.

use umicraft_protocol::packets::play::ItemStack;

use super::stack::count;
use crate::enchantments::{self, EnchantmentDef};
use crate::items::{self, ItemKind, Tier, ToolKind};
use crate::nbt::Nbt;

/// The item slot, the material slot and the result (`ContainerRepair`).
const FIRST: usize = 0;
const SECOND: usize = 1;
/// The result slot.
pub const RESULT: usize = 2;
/// The cost from which a survival player cannot repair (`maximumCost >= 40`).
const TOO_EXPENSIVE: i32 = 40;
/// Registry name of the enchanted book.
const ENCHANTED_BOOK: &str = "enchanted_book";

/// What the anvil window keeps between two computations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnvilState {
    /// The name the player typed (`repairedItemName`, the text of the last
    /// `MC|ItemName`).
    pub name: String,
    /// The level cost (`maximumCost`).
    pub cost: i32,
    /// How many items of the material slot the repair uses (`materialCost`).
    pub material_cost: i32,
    /// Whether the player is in creative mode (no level cost, no cap).
    pub creative: bool,
    /// The player's experience level.
    pub level: i32,
}

impl AnvilState {
    /// The result slot's `canTakeStack`: a cost, and the levels to pay it
    /// unless in creative.
    #[must_use]
    pub fn can_take(&self) -> bool {
        self.cost > 0 && (self.creative || self.level >= self.cost)
    }
}

/// `updateRepairOutput`: fills the result slot of `own` and the cost.
pub fn update_output(own: &mut [Option<ItemStack>], state: &mut AnvilState) {
    let slot = |index: usize| own.get(index).and_then(Option::as_ref);
    let result = compute(slot(FIRST), slot(SECOND), state);
    if let Some(slot) = own.get_mut(RESULT) {
        *slot = result;
    }
}

/// The result slot's `onPickupFromSlot`: the item slot empties, the material
/// slot loses what the repair used (or empties), and the cost resets; the
/// result is then computed again from what is left.
pub fn take_result(own: &mut [Option<ItemStack>], state: &mut AnvilState) {
    if let Some(slot) = own.get_mut(FIRST) {
        *slot = None;
    }
    if let Some(slot) = own.get_mut(SECOND) {
        let used = state.material_cost;
        match slot {
            Some(material) if used > 0 && count(material) > used => {
                material.stack_size = (count(material) - used) as i8;
            }
            _ => *slot = None,
        }
    }
    state.cost = 0;
    update_output(own, state);
}

/// A stack whose tag is parsed, to be edited.
#[derive(Debug, Clone)]
struct Tagged {
    item_id: i16,
    size: i8,
    damage: i16,
    tag: Option<Nbt>,
    def: &'static items::ItemDef,
}

impl Tagged {
    /// `None` for an unknown item or a tag that does not parse: the anvil
    /// then shows no result rather than a wrong one.
    fn read(stack: &ItemStack) -> Option<Self> {
        let tag = match &stack.nbt {
            Some(bytes) => Some(Nbt::parse(bytes).ok()?),
            None => None,
        };
        Some(Self {
            item_id: stack.item_id,
            size: stack.stack_size,
            damage: stack.damage,
            tag,
            def: items::lookup(i32::from(stack.item_id))?,
        })
    }

    fn into_stack(self) -> ItemStack {
        ItemStack {
            item_id: self.item_id,
            stack_size: self.size,
            damage: self.damage,
            nbt: self.tag.and_then(|tag| tag.to_root_bytes()),
        }
    }

    fn max_damage(&self) -> i32 {
        i32::from(self.def.max_damage)
    }

    /// `isItemStackDamageable`: a durability and no `Unbreakable` tag.
    fn damageable(&self) -> bool {
        let unbreakable = self
            .tag
            .as_ref()
            .and_then(|tag| tag.get("Unbreakable"))
            .and_then(Nbt::as_int)
            .is_some_and(|value| value != 0);
        self.def.is_damageable() && !unbreakable
    }

    fn is_book(&self) -> bool {
        self.def.name == ENCHANTED_BOOK
    }

    /// `getRepairCost`.
    fn repair_cost(&self) -> i32 {
        match self.tag.as_ref().and_then(|tag| tag.get("RepairCost")) {
            Some(Nbt::Int(cost)) => *cost,
            _ => 0,
        }
    }

    fn set_repair_cost(&mut self, cost: i32) {
        self.tag_mut().insert("RepairCost", Nbt::Int(cost));
    }

    fn tag_mut(&mut self) -> &mut Nbt {
        self.tag.get_or_insert_with(|| Nbt::Compound(Vec::new()))
    }

    /// The custom name (`display.Name`), if any.
    fn custom_name(&self) -> Option<&str> {
        self.tag.as_ref()?.get("display")?.get("Name")?.as_str()
    }

    /// `setStackDisplayName`.
    fn set_name(&mut self, name: &str) {
        let tag = self.tag_mut();
        if !matches!(tag.get("display"), Some(Nbt::Compound(_))) {
            tag.insert("display", Nbt::Compound(Vec::new()));
        }
        if let Some(display) = tag.get_mut("display") {
            display.insert("Name", Nbt::String(name.to_string()));
        }
    }

    /// `clearCustomName`: an emptied `display` goes, then an emptied tag.
    fn clear_name(&mut self) {
        let Some(tag) = self.tag.as_mut() else {
            return;
        };
        let Some(display) = tag.get_mut("display") else {
            return;
        };
        display.remove("Name");
        if display.is_empty_compound() {
            tag.remove("display");
            if tag.is_empty_compound() {
                self.tag = None;
            }
        }
    }

    /// The list key of the enchantments: books store theirs apart.
    fn enchantment_key(&self) -> &'static str {
        if self.is_book() {
            "StoredEnchantments"
        } else {
            "ench"
        }
    }

    /// `EnchantmentHelper.getEnchantments`: id and level in list order, a
    /// repeated id keeping its first place and its last level.
    fn enchantments(&self) -> Vec<(i16, i32)> {
        let mut out: Vec<(i16, i32)> = Vec::new();
        let list = self
            .tag
            .as_ref()
            .and_then(|tag| tag.get(self.enchantment_key()))
            .and_then(Nbt::as_list)
            .unwrap_or_default();
        for entry in list {
            let short = |key| entry.get(key).and_then(Nbt::as_int).unwrap_or(0);
            set_level(&mut out, short("id") as i16, short("lvl"));
        }
        out
    }

    /// `EnchantmentHelper.setEnchantments` (books through
    /// `ItemEnchantedBook.addEnchantment`, which only raises levels).
    fn set_enchantments(&mut self, map: &[(i16, i32)]) {
        let known: Vec<(i16, i32)> = map
            .iter()
            .copied()
            .filter(|&(id, _)| enchantments::lookup(id).is_some())
            .collect();
        if self.is_book() {
            let mut stored = self.enchantments();
            for &(id, level) in &known {
                match stored.iter_mut().find(|(stored_id, _)| *stored_id == id) {
                    Some(entry) => entry.1 = entry.1.max(level),
                    None => stored.push((id, level)),
                }
            }
            if !known.is_empty() {
                self.tag_mut()
                    .insert("StoredEnchantments", enchantment_list(&stored));
            }
        } else if !known.is_empty() {
            self.tag_mut().insert("ench", enchantment_list(&known));
        } else if let Some(tag) = self.tag.as_mut() {
            tag.remove("ench");
        }
    }

    /// `getIsRepairable`: the material of a tool, sword or piece of armour.
    fn repaired_by(&self, material: &Tagged) -> bool {
        let wanted = match self.def.kind {
            ItemKind::Tool(
                ToolKind::Sword | ToolKind::Pickaxe | ToolKind::Axe | ToolKind::Shovel,
                tier,
            ) => match tier {
                Tier::Wood => "planks",
                Tier::Stone => "cobblestone",
                Tier::Iron => "iron_ingot",
                Tier::Gold => "gold_ingot",
                Tier::Diamond => "diamond",
            },
            ItemKind::Armor(armor, _) => match armor {
                items::ArmorMaterial::Leather => "leather",
                items::ArmorMaterial::Chain | items::ArmorMaterial::Iron => "iron_ingot",
                items::ArmorMaterial::Gold => "gold_ingot",
                items::ArmorMaterial::Diamond => "diamond",
            },
            _ => return false,
        };
        material.def.name == wanted
    }
}

/// Sets the level of `id`, appending it when new (a `LinkedHashMap.put`).
fn set_level(map: &mut Vec<(i16, i32)>, id: i16, level: i32) {
    match map.iter_mut().find(|(known, _)| *known == id) {
        Some(entry) => entry.1 = level,
        None => map.push((id, level)),
    }
}

/// An `ench` list of `{id: short, lvl: short}` compounds.
fn enchantment_list(map: &[(i16, i32)]) -> Nbt {
    Nbt::List(
        map.iter()
            .map(|&(id, level)| {
                Nbt::Compound(vec![
                    ("id".into(), Nbt::Short(id)),
                    ("lvl".into(), Nbt::Short(level as i16)),
                ])
            })
            .collect(),
    )
}

/// The body of `updateRepairOutput`.
fn compute(
    first: Option<&ItemStack>,
    second: Option<&ItemStack>,
    state: &mut AnvilState,
) -> Option<ItemStack> {
    state.cost = 1;
    state.material_cost = 0;
    let Some(source) = first.and_then(Tagged::read) else {
        state.cost = 0;
        return None;
    };
    let material = match second {
        Some(stack) => match Tagged::read(stack) {
            Some(material) => Some(material),
            None => {
                state.cost = 0;
                return None;
            }
        },
        None => None,
    };
    let mut output = source.clone();
    let mut map = output.enchantments();
    let base = source.repair_cost() + material.as_ref().map_or(0, Tagged::repair_cost);
    let mut price = 0;
    if let Some(material) = &material {
        let Some(added) = combine(&source, material, &mut output, &mut map, state) else {
            state.cost = 0;
            return None;
        };
        price += added;
    }
    let rename_price = rename(&source, &mut output, &state.name);
    price += rename_price;
    state.cost = base + price;
    if rename_price == price && rename_price > 0 && state.cost >= TOO_EXPENSIVE {
        state.cost = TOO_EXPENSIVE - 1;
    }
    if price <= 0 || (state.cost >= TOO_EXPENSIVE && !state.creative) {
        return None;
    }
    let inherited = material.as_ref().map_or(0, Tagged::repair_cost);
    output.set_repair_cost(output.repair_cost().max(inherited) * 2 + 1);
    output.set_enchantments(&map);
    Some(output.into_stack())
}

/// The repair or the combination with the material slot; returns the levels
/// it costs, `None` when the two cannot go together.
fn combine(
    source: &Tagged,
    material: &Tagged,
    output: &mut Tagged,
    map: &mut Vec<(i16, i32)>,
    state: &mut AnvilState,
) -> Option<i32> {
    let from_book = material.is_book() && !material.enchantments().is_empty();
    if output.damageable() && output.repaired_by(material) {
        return repair_with_material(output, material, state);
    }
    if !from_book && (output.item_id != material.item_id || !output.damageable()) {
        return None;
    }
    let mut price = 0;
    if output.damageable() && !from_book {
        price += merge_durability(source, material, output);
    }
    for (id, their_level) in material.enchantments() {
        let Some(def) = enchantments::lookup(id) else {
            continue;
        };
        price += add_enchantment(def, their_level, source, map, from_book, state.creative);
    }
    Some(price)
}

/// Repairs a quarter of the durability per material item; `None` when there
/// is nothing to repair.
fn repair_with_material(
    output: &mut Tagged,
    material: &Tagged,
    state: &mut AnvilState,
) -> Option<i32> {
    let quarter = |output: &Tagged| i32::from(output.damage).min(output.max_damage() / 4);
    let mut chunk = quarter(output);
    if chunk <= 0 {
        return None;
    }
    let mut used = 0;
    let mut price = 0;
    while chunk > 0 && used < i32::from(material.size) {
        output.damage = (i32::from(output.damage) - chunk) as i16;
        price += 1;
        chunk = quarter(output);
        used += 1;
    }
    state.material_cost = used;
    Some(price)
}

/// Two of the same tool: the durabilities add up with a 12% bonus.
fn merge_durability(source: &Tagged, material: &Tagged, output: &mut Tagged) -> i32 {
    let left = source.max_damage() - i32::from(source.damage);
    let added = material.max_damage() - i32::from(material.damage) + output.max_damage() * 12 / 100;
    let damage = (output.max_damage() - (left + added)).max(0);
    if damage < i32::from(output.damage) {
        output.damage = damage as i16;
        2
    } else {
        0
    }
}

/// One enchantment of the material: its new level and price, and one level
/// per enchantment of the item it clashes with.
fn add_enchantment(
    def: &EnchantmentDef,
    their_level: i32,
    source: &Tagged,
    map: &mut Vec<(i16, i32)>,
    from_book: bool,
    creative: bool,
) -> i32 {
    let mine = map
        .iter()
        .find(|(id, _)| *id == def.id)
        .map_or(0, |&(_, level)| level);
    let level = if mine == their_level {
        their_level + 1
    } else {
        their_level.max(mine)
    };
    let mut fits =
        creative || source.is_book() || def.can_apply(source.item_id, source.damageable());
    let mut price = 0;
    for &(other, _) in map.iter() {
        // An id 1.8 does not know is compatible with everything.
        let clashes = other != def.id
            && enchantments::lookup(other).is_some_and(|other| !def.can_apply_together(other));
        if clashes {
            fits = false;
            price += 1;
        }
    }
    if fits {
        let level = level.min(def.max_level);
        set_level(map, def.id, level);
        price += def.anvil_price(from_book) * level;
    }
    price
}

/// The name part: one level to clear a custom name or to set a new one.
fn rename(source: &Tagged, output: &mut Tagged, name: &str) -> i32 {
    if name.trim().is_empty() {
        if source.custom_name().is_some() {
            output.clear_name();
            return 1;
        }
        return 0;
    }
    // `GuiRepair.renameItem` sends an empty name while the field still shows
    // the default name, so a non-empty name differs from it unless it is the
    // custom name already there.
    if source.custom_name() == Some(name) {
        return 0;
    }
    output.set_name(name);
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAMOND_SWORD: i16 = 276;
    const DIAMOND: i16 = 264;
    const IRON_INGOT: i16 = 265;
    const ENCHANTED_BOOK_ID: i16 = 403;
    /// The durability of a diamond sword.
    const SWORD_DURABILITY: i32 = 1561;

    fn stack(item_id: i16, size: i8, damage: i16, tag: Option<Nbt>) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: size,
            damage,
            nbt: tag.and_then(|tag| tag.to_root_bytes()),
        }
    }

    fn enchanted(key: &str, list: &[(i16, i32)]) -> Option<Nbt> {
        Some(Nbt::Compound(vec![(key.into(), enchantment_list(list))]))
    }

    fn survival() -> AnvilState {
        AnvilState {
            level: 30,
            ..AnvilState::default()
        }
    }

    fn run(first: ItemStack, second: Option<ItemStack>, state: &mut AnvilState) -> Option<Tagged> {
        let mut own = vec![Some(first), second, None];
        update_output(&mut own, state);
        own[RESULT]
            .as_ref()
            .map(|s| Tagged::read(s).expect("a known item"))
    }

    /// Regression: the result of an anvil showed up only after the next
    /// click, because the server does not send it.
    #[test]
    fn diamonds_repair_a_quarter_each_and_cost_a_level_each() {
        let mut state = survival();
        let worn = stack(DIAMOND_SWORD, 1, 1000, None);
        let out = run(worn, Some(stack(DIAMOND, 2, 0, None)), &mut state).expect("a repair");
        let quarter = SWORD_DURABILITY / 4;
        assert_eq!(i32::from(out.damage), 1000 - 2 * quarter);
        assert_eq!((state.cost, state.material_cost), (2, 2));
        assert_eq!(out.repair_cost(), 1);
    }

    #[test]
    fn a_wrong_material_or_an_intact_item_gives_nothing() {
        let mut state = survival();
        let worn = stack(DIAMOND_SWORD, 1, 1000, None);
        assert!(run(worn, Some(stack(IRON_INGOT, 1, 0, None)), &mut state).is_none());
        assert_eq!(state.cost, 0);
        let intact = stack(DIAMOND_SWORD, 1, 0, None);
        assert!(run(intact, Some(stack(DIAMOND, 1, 0, None)), &mut state).is_none());
    }

    #[test]
    fn a_book_adds_its_enchantment_at_half_price() {
        let mut state = survival();
        let sword = stack(DIAMOND_SWORD, 1, 0, None);
        let book = stack(
            ENCHANTED_BOOK_ID,
            1,
            0,
            enchanted("StoredEnchantments", &[(16, 3)]),
        );
        let out = run(sword, Some(book), &mut state).expect("an enchanted sword");
        assert_eq!(out.enchantments(), [(16, 3)]);
        // Sharpness has weight 10: one level per level, halved to at least 1.
        assert_eq!(state.cost, 3);
    }

    #[test]
    fn equal_levels_merge_one_higher_up_to_the_maximum() {
        let mut state = survival();
        let a = stack(DIAMOND_SWORD, 1, 0, enchanted("ench", &[(16, 4)]));
        let b = stack(DIAMOND_SWORD, 1, 0, enchanted("ench", &[(16, 4)]));
        let out = run(a, Some(b), &mut state).expect("a merge");
        assert_eq!(out.enchantments(), [(16, 5)]);
        let a = stack(DIAMOND_SWORD, 1, 0, enchanted("ench", &[(16, 5)]));
        let b = stack(DIAMOND_SWORD, 1, 0, enchanted("ench", &[(16, 5)]));
        assert_eq!(
            run(a, Some(b), &mut state).expect("a merge").enchantments(),
            [(16, 5)]
        );
    }

    #[test]
    fn a_clashing_enchantment_is_dropped_but_still_costs() {
        let mut state = survival();
        let sharp = stack(DIAMOND_SWORD, 1, 0, enchanted("ench", &[(16, 1)]));
        let smite = stack(
            ENCHANTED_BOOK_ID,
            1,
            0,
            enchanted("StoredEnchantments", &[(17, 1)]),
        );
        let out = run(sharp, Some(smite), &mut state).expect("a result");
        assert_eq!(out.enchantments(), [(16, 1)]);
        assert_eq!(state.cost, 1);
    }

    #[test]
    fn renaming_costs_one_level_and_sets_the_name() {
        let mut state = AnvilState {
            name: "Excalibur".into(),
            ..survival()
        };
        let out = run(stack(DIAMOND_SWORD, 1, 0, None), None, &mut state).expect("a rename");
        assert_eq!(out.custom_name(), Some("Excalibur"));
        assert_eq!(state.cost, 1);
        // A blank name clears a custom one.
        let mut state = AnvilState::default();
        let named = out.into_stack();
        let cleared = run(named, None, &mut state).expect("a cleared name");
        assert_eq!(cleared.custom_name(), None);
        assert_eq!(cleared.repair_cost(), 3);
    }

    #[test]
    fn nothing_to_do_or_too_expensive_gives_no_result() {
        let mut state = survival();
        assert!(run(stack(DIAMOND_SWORD, 1, 0, None), None, &mut state).is_none());
        let costly = Some(Nbt::Compound(vec![("RepairCost".into(), Nbt::Int(39))]));
        let mut state = AnvilState {
            name: "x".into(),
            ..survival()
        };
        let worn = stack(DIAMOND_SWORD, 1, 100, costly);
        assert!(run(worn.clone(), Some(stack(DIAMOND, 1, 0, None)), &mut state).is_none());
        assert_eq!(state.cost, 41);
        // A rename alone is capped at 39.
        let out = run(worn, None, &mut state);
        assert!(out.is_some());
        assert_eq!(state.cost, 39);
    }

    #[test]
    fn taking_the_result_uses_the_material_and_resets() {
        let mut state = survival();
        let mut own = vec![
            Some(stack(DIAMOND_SWORD, 1, 1000, None)),
            Some(stack(DIAMOND, 5, 0, None)),
            None,
        ];
        update_output(&mut own, &mut state);
        assert!(state.can_take());
        take_result(&mut own, &mut state);
        assert_eq!(own[FIRST], None);
        // 1000 damage takes 390, 390 and the last 220: three diamonds.
        assert_eq!(own[SECOND].as_ref().map(|s| s.stack_size), Some(2));
        assert_eq!((own[RESULT].clone(), state.cost), (None, 0));
    }

    #[test]
    fn the_levels_decide_whether_the_result_can_be_taken() {
        let mut state = AnvilState {
            cost: 5,
            level: 4,
            ..AnvilState::default()
        };
        assert!(!state.can_take());
        state.creative = true;
        assert!(state.can_take());
    }
}
