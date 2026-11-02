// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player-state half of the HUD (P9-b, P9-c): hearts, food, armour, air,
//! mount health, the experience or jump bar, and the held-item name.
//!
//! [`HudTracker`] keeps what the packets tell about the local player that no
//! other module stores (saturation, air, absorption, hardcore, the hurt
//! countdown, the horse jump charge and the highlight of the held item), steps
//! the HUD clock (`GuiIngame.updateCounter`) once per game tick and turns the
//! lot into the views of [`umicraft_game::hud`]. `game.rs` only forwards the
//! events and the tick, and reads [`HudTracker::views`] in `hud()`.
//!
//! Sources: MCP `GuiIngame.renderPlayerStats`, `renderExpBar`,
//! `func_181551_a`, `updateTick`, `EntityPlayerSP.setPlayerSPHealth` and
//! `onLivingUpdate`; see `docs/sources.md`.

use std::collections::BTreeMap;
use std::time::Instant;

use umicraft_game::entity::{EntityKind, EntityStore, Variant};
use umicraft_game::gamemode::{is_hardcore, GameMode};
use umicraft_game::hud::{
    bottom_bar, vitals_view, BottomBar, FlashFrame, HealthFlash, HeldIdentity, HeldNameTracker,
    HeldNameView, HorseJump, HurtResistance, Riding, VitalsInput, VitalsView,
};
use umicraft_game::inventory::tooltip::tooltip;
use umicraft_game::items;
use umicraft_game::server::attributes::Attributes;
use umicraft_game::server::effects::Effects;
use umicraft_net::play::PlayEvent;
use umicraft_protocol::packets::play::{ItemStack, Metadata};

use crate::tooltip_draw::resolve;

/// `Entity.getAir` of a player that has not been drowning.
const FULL_AIR: i32 = 300;
/// Data watcher entry of the air (MCP `Entity.getAir`).
const AIR_WATCHER: u8 = 1;
/// Data watcher entry of the absorption hearts (MCP `EntityPlayer.entityInit`).
const ABSORPTION_WATCHER: u8 = 17;
/// `Entity Status` that hurts an entity.
const STATUS_HURT: i8 = 2;
/// The attribute behind `getMaxHealth`.
const MAX_HEALTH: &str = "generic.maxHealth";
/// The default `getMaxHealth` of a living entity.
const DEFAULT_MAX_HEALTH: f32 = 20.0;

/// Potion ids (MCP `Potion`).
const REGENERATION: u8 = 10;
const HUNGER: u8 = 17;
const POISON: u8 = 19;
const WITHER: u8 = 20;

/// What the HUD shows of the player state, rebuilt every tick.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HudViews {
    /// The status rows; `None` where the game mode hides them.
    pub(crate) vitals: Option<VitalsView>,
    /// The experience bar or the jump bar.
    pub(crate) bar: BottomBar,
    /// The highlighted name of the held item.
    pub(crate) held_name: Option<HeldNameView>,
}

impl Default for HudViews {
    fn default() -> Self {
        Self {
            vitals: None,
            bar: BottomBar::None,
            held_name: None,
        }
    }
}

/// The facts of the game a tick of the HUD reads.
pub(crate) struct HudFacts<'a> {
    /// The game mode.
    pub(crate) mode: GameMode,
    /// `getHealth`.
    pub(crate) health: f32,
    /// The food level.
    pub(crate) food: f32,
    /// The armour value.
    pub(crate) armor: f32,
    /// The experience bar fill, `0..=1`.
    pub(crate) experience: f32,
    /// The experience level.
    pub(crate) level: i32,
    /// The potion effects of the player.
    pub(crate) effects: &'a Effects,
    /// The attributes of the player.
    pub(crate) attributes: &'a Attributes,
    /// The entity the player rides.
    pub(crate) vehicle: Option<i32>,
    /// The entities, to read the vehicle from.
    pub(crate) entities: &'a EntityStore,
    /// Whether the eyes are in water (`isInsideOfMaterial(water)`).
    pub(crate) head_in_water: bool,
    /// The stack in the selected hotbar slot.
    pub(crate) held: Option<&'a ItemStack>,
    /// Whether the jump key is down.
    pub(crate) jump: bool,
}

/// The state the HUD keeps between ticks.
#[derive(Debug)]
pub(crate) struct HudTracker {
    clock: Instant,
    update_counter: u32,
    saturation: f32,
    air: i32,
    absorption: f32,
    hardcore: bool,
    hurt: HurtResistance,
    flash: HealthFlash,
    held: HeldNameTracker,
    horse_jump: HorseJump,
    was_jumping: bool,
    /// `generic.maxHealth` of the entities the server described.
    max_health: BTreeMap<i32, f32>,
    views: HudViews,
}

impl Default for HudTracker {
    fn default() -> Self {
        Self {
            clock: Instant::now(),
            update_counter: 0,
            saturation: 5.0,
            air: FULL_AIR,
            absorption: 0.0,
            hardcore: false,
            hurt: HurtResistance::new(),
            flash: HealthFlash::new(),
            held: HeldNameTracker::new(),
            horse_jump: HorseJump::new(),
            was_jumping: false,
            max_health: BTreeMap::new(),
            views: HudViews::default(),
        }
    }
}

impl HudTracker {
    /// The views of the last tick.
    pub(crate) fn views(&self) -> &HudViews {
        &self.views
    }

    /// Records what `event` says about the HUD. `own_id` is the entity id of
    /// the local player.
    pub(crate) fn observe(&mut self, event: &PlayEvent, own_id: i32) {
        match event {
            PlayEvent::JoinGame(info) => {
                // A new `EntityPlayerSP`: nothing carries over.
                *self = Self {
                    hardcore: is_hardcore(info.gamemode),
                    ..Self::default()
                };
            }
            PlayEvent::Respawn(_) => {
                self.hurt = HurtResistance::new();
                self.flash = HealthFlash::new();
                self.air = FULL_AIR;
                self.absorption = 0.0;
            }
            PlayEvent::UpdateHealth(packet) => {
                self.saturation = packet.saturation;
                self.hurt.on_health(packet.health);
            }
            PlayEvent::EntityMetadata(packet) if packet.entity_id == own_id => {
                self.apply_metadata(&packet.metadata);
            }
            PlayEvent::EntityStatus(packet)
                if packet.entity_id == own_id && packet.status == STATUS_HURT =>
            {
                self.hurt.on_hurt();
            }
            PlayEvent::EntityProperties(packet) => {
                let max = packet.properties.iter().find(|p| p.key == MAX_HEALTH);
                if let Some(max) = max {
                    self.max_health.insert(packet.entity_id, max.value as f32);
                }
            }
            PlayEvent::DestroyEntities(packet) => {
                for id in &packet.entity_ids {
                    self.max_health.remove(id);
                }
            }
            _ => {}
        }
    }

    fn apply_metadata(&mut self, metadata: &Metadata) {
        if let Some(air) = metadata.get_short(AIR_WATCHER) {
            self.air = i32::from(air);
        }
        if let Some(absorption) = metadata.get_float(ABSORPTION_WATCHER) {
            self.absorption = absorption;
        }
    }

    /// Advances the HUD by one game tick and rebuilds the views. Returns
    /// whether they changed, so the caller publishes the HUD again.
    ///
    /// The hearts shake and wiggle with `updateCounter`, so the HUD changes
    /// on many ticks of a hurt or regenerating player and on none otherwise.
    pub(crate) fn tick(&mut self, facts: &HudFacts<'_>) -> bool {
        self.update_counter = self.update_counter.wrapping_add(1);
        self.hurt.tick();
        let held = facts.held.map(identity_of);
        self.held.tick(held);
        let riding = riding_of(facts, &self.max_health);
        // The jump power of a horse that is ridden; the `Riding Jump` packet
        // it would send is not sent yet (see docs/parity.md).
        let _ = self
            .horse_jump
            .tick(riding_horse(facts), self.was_jumping, facts.jump);
        self.was_jumping = facts.jump;

        let views = self.build(facts, riding);
        let changed = views != self.views;
        self.views = views;
        changed
    }

    fn build(&mut self, facts: &HudFacts<'_>, riding: Riding) -> HudViews {
        let draws_rows = matches!(facts.mode, GameMode::Survival | GameMode::Adventure);
        let spectator = facts.mode == GameMode::Spectator;
        let flash = self.flash.frame(
            facts.health.ceil() as i32,
            self.hurt.active(),
            self.update_counter,
            self.clock.elapsed().as_millis() as u64,
        );
        let bar = bottom_bar(
            draws_rows,
            riding_horse(facts),
            self.horse_jump.power(),
            facts.experience,
            facts.level,
        );
        let held_name = (!spectator)
            .then(|| self.held.view(|| held_display_name(facts.held), draws_rows))
            .flatten();
        HudViews {
            vitals: draws_rows.then(|| vitals_view(&self.vitals_input(facts, riding, flash))),
            bar,
            held_name,
        }
    }

    fn vitals_input(&self, facts: &HudFacts<'_>, riding: Riding, flash: FlashFrame) -> VitalsInput {
        let active = |id| facts.effects.get(id).is_some();
        VitalsInput {
            health: facts.health,
            max_health: facts
                .attributes
                .value(MAX_HEALTH)
                .map_or(DEFAULT_MAX_HEALTH, |value| value as f32),
            absorption: self.absorption,
            armor: facts.armor as i32,
            food: facts.food as i32,
            saturation: self.saturation,
            hardcore: self.hardcore,
            poison: active(POISON),
            wither: active(WITHER),
            regeneration: active(REGENERATION),
            hunger: active(HUNGER),
            update_counter: self.update_counter,
            flash,
            riding,
            air: facts.head_in_water.then_some(self.air),
        }
    }
}

/// The identity `GuiIngame.updateTick` compares: item, damage rule and NBT.
fn identity_of(stack: &ItemStack) -> HeldIdentity {
    HeldIdentity {
        item_id: stack.item_id,
        damage: stack.damage,
        damageable: items::lookup(i32::from(stack.item_id))
            .is_some_and(items::ItemDef::is_damageable),
        nbt: stack.nbt.clone(),
    }
}

/// The name `ItemStack.getDisplayName` gives and whether the player set it
/// (the first line of the tooltip, which is italic exactly then).
fn held_display_name(held: Option<&ItemStack>) -> (String, bool) {
    held.and_then(|stack| tooltip(stack).into_iter().next())
        .map_or_else(Default::default, |line| (resolve(&line.text), line.italic))
}

/// What the player sits on: nothing, an entity without health, or a mob.
fn riding_of(facts: &HudFacts<'_>, max_health: &BTreeMap<i32, f32>) -> Riding {
    let Some(id) = facts.vehicle else {
        return Riding::OnFoot;
    };
    let record = facts
        .entities
        .entities()
        .get(&umicraft_game::entity::EntityId(id));
    match record {
        Some(record) if record.kind.is_living() => {
            let max = max_health.get(&id).copied().unwrap_or(DEFAULT_MAX_HEALTH);
            Riding::Mob {
                health: record.health.unwrap_or(max),
                max_health: max,
            }
        }
        _ => Riding::Vehicle,
    }
}

/// `EntityPlayerSP.isRidingHorse`: a saddled horse, donkey or mule.
fn riding_horse(facts: &HudFacts<'_>) -> bool {
    let Some(id) = facts.vehicle else {
        return false;
    };
    facts
        .entities
        .entities()
        .get(&umicraft_game::entity::EntityId(id))
        .is_some_and(|record| {
            record.kind == EntityKind::Horse
                && matches!(record.meta.variant, Variant::Horse(horse) if horse.saddled())
        })
}

#[cfg(test)]
mod tests {
    use umicraft_net::play::JoinGameInfo;
    use umicraft_protocol::packets::play::clientbound::{EntityStatus, UpdateHealth};

    use super::*;

    struct World {
        effects: Effects,
        attributes: Attributes,
        entities: EntityStore,
    }

    impl World {
        fn new() -> Self {
            Self {
                effects: Effects::default(),
                attributes: Attributes::default(),
                entities: EntityStore::new(),
            }
        }

        fn facts(&self, mode: GameMode, health: f32) -> HudFacts<'_> {
            HudFacts {
                mode,
                health,
                food: 20.0,
                armor: 0.0,
                experience: 0.5,
                level: 3,
                effects: &self.effects,
                attributes: &self.attributes,
                vehicle: None,
                entities: &self.entities,
                head_in_water: false,
                held: None,
                jump: false,
            }
        }
    }

    fn health_packet(health: f32) -> PlayEvent {
        PlayEvent::UpdateHealth(UpdateHealth {
            health,
            food: 20,
            saturation: 0.0,
        })
    }

    #[test]
    fn survival_shows_rows_and_the_experience_bar() {
        let world = World::new();
        let mut tracker = HudTracker::default();
        tracker.tick(&world.facts(GameMode::Survival, 20.0));
        let views = tracker.views();
        assert_eq!(views.vitals.as_ref().map(|v| v.hearts.len()), Some(10));
        assert!(matches!(
            views.bar,
            BottomBar::Experience { level: Some(3), .. }
        ));
    }

    #[test]
    fn creative_and_spectator_hide_the_rows_and_the_bar() {
        let world = World::new();
        for mode in [GameMode::Creative, GameMode::Spectator] {
            let mut tracker = HudTracker::default();
            tracker.tick(&world.facts(mode, 20.0));
            assert!(tracker.views().vitals.is_none());
            assert_eq!(tracker.views().bar, BottomBar::None);
        }
    }

    #[test]
    fn the_hardcore_bit_of_the_join_reaches_the_hearts() {
        let world = World::new();
        let mut tracker = HudTracker::default();
        let join = JoinGameInfo {
            gamemode: 8,
            ..JoinGameInfo::default()
        };
        tracker.observe(&PlayEvent::JoinGame(join), 1);
        tracker.tick(&world.facts(GameMode::Survival, 20.0));
        let hearts = &tracker.views().vitals.as_ref().expect("rows").hearts;
        assert!(hearts.iter().all(|heart| heart.hardcore));
    }

    #[test]
    fn damage_blinks_the_hearts_after_the_health_packet() {
        let world = World::new();
        let mut tracker = HudTracker::default();
        tracker.observe(&health_packet(20.0), 1);
        tracker.tick(&world.facts(GameMode::Survival, 20.0));
        tracker.observe(&health_packet(14.0), 1);
        let mut blinked = false;
        for _ in 0..20 {
            tracker.tick(&world.facts(GameMode::Survival, 14.0));
            let hearts = &tracker.views().vitals.as_ref().expect("rows").hearts;
            blinked |= hearts[0].blink;
        }
        assert!(blinked, "the hearts blink within the 20 ticks after damage");
    }

    #[test]
    fn a_tick_that_changes_nothing_reports_no_change() {
        let world = World::new();
        let mut tracker = HudTracker::default();
        assert!(tracker.tick(&world.facts(GameMode::Survival, 20.0)));
        assert!(!tracker.tick(&world.facts(GameMode::Survival, 20.0)));
    }

    #[test]
    fn the_hurt_status_of_another_entity_is_ignored() {
        let mut tracker = HudTracker::default();
        let status = |entity_id| {
            PlayEvent::EntityStatus(EntityStatus {
                entity_id,
                status: 2,
            })
        };
        tracker.observe(&status(9), 1);
        assert!(!tracker.hurt.active());
        tracker.observe(&status(1), 1);
        assert!(tracker.hurt.active());
    }

    #[test]
    fn air_shows_only_with_the_head_in_water() {
        let world = World::new();
        let mut tracker = HudTracker::default();
        let mut facts = world.facts(GameMode::Survival, 20.0);
        facts.head_in_water = true;
        tracker.tick(&facts);
        let air = tracker.views().vitals.as_ref().and_then(|v| v.air);
        assert_eq!(air.map(|a| a.full), Some(10));
    }

    #[test]
    fn a_held_item_shows_its_name_for_forty_ticks_in_survival() {
        let world = World::new();
        let stack = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let mut tracker = HudTracker::default();
        let mut facts = world.facts(GameMode::Survival, 20.0);
        facts.held = Some(&stack);
        tracker.tick(&facts);
        let name = tracker.views().held_name.clone().expect("name");
        assert_eq!((name.alpha, name.dy), (255, 0));
        assert!(name.text.contains("Sword"));
        for _ in 0..40 {
            tracker.tick(&facts);
        }
        assert!(tracker.views().held_name.is_none());
    }
}
