// SPDX-License-Identifier: MIT OR Apache-2.0

//! P1 session state: the data of protocol-47 packets whose consumers (HUD,
//! tab list, scoreboard, movement effects, sign rendering) arrive in later
//! milestones.
//!
//! [`ServerState`] is plain data: the network layer feeds it one decoded
//! packet at a time through the `apply_*` methods and the 20 Hz loop calls
//! [`ServerState::tick`]; the consumers read it through accessors. Nothing
//! here does I/O and no hostile packet can make it panic (counts are capped,
//! indices bounds-checked, unknown ids ignored).
//!
//! Behaviour follows the MCP `NetHandlerPlayClient` handlers (policy:
//! ADR-0001, original Rust); each submodule cites its class. The decisions
//! are in `docs/decisions/0020-p1-protocol-completeness.md` and the sources
//! in `docs/sources.md` ("P1 session state").
//!
//! ```
//! use umicraft_game::server::ServerState;
//! use umicraft_protocol::packets::play::clientbound::change_game_state::ChangeGameState;
//!
//! let mut state = ServerState::new();
//! state.on_join(1);
//! state.apply_game_state(&ChangeGameState { reason: 1, value: 0.0 });
//! assert!(state.weather().raining());
//! ```

pub mod attributes;
pub mod block_entities;
pub mod effects;
pub mod map_colors;
pub mod maps;
pub mod player_list;
pub mod scoreboard;
pub mod title;
pub mod weather;
pub mod world_border;

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::block_break_anim::BlockBreakAnim;
use umicraft_protocol::packets::play::clientbound::camera::Camera;
use umicraft_protocol::packets::play::clientbound::change_game_state::ChangeGameState;
use umicraft_protocol::packets::play::clientbound::combat_event::CombatEvent;
use umicraft_protocol::packets::play::clientbound::display_scoreboard::DisplayScoreboard;
use umicraft_protocol::packets::play::clientbound::entity_effect::EntityEffect;
use umicraft_protocol::packets::play::clientbound::entity_properties::EntityProperties;
use umicraft_protocol::packets::play::clientbound::maps::Maps;
use umicraft_protocol::packets::play::clientbound::player_abilities::PlayerAbilities;
use umicraft_protocol::packets::play::clientbound::player_list_header_footer::PlayerListHeaderFooter;
use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerListItem;
use umicraft_protocol::packets::play::clientbound::plugin_message::PluginMessage;
use umicraft_protocol::packets::play::clientbound::remove_entity_effect::RemoveEntityEffect;
use umicraft_protocol::packets::play::clientbound::resource_pack_send::ResourcePackSend;
use umicraft_protocol::packets::play::clientbound::scoreboard_objective::ScoreboardObjective;
use umicraft_protocol::packets::play::clientbound::server_difficulty::ServerDifficulty;
use umicraft_protocol::packets::play::clientbound::sign_editor_open::SignEditorOpen;
use umicraft_protocol::packets::play::clientbound::statistics::Statistics;
use umicraft_protocol::packets::play::clientbound::teams::Teams;
use umicraft_protocol::packets::play::clientbound::title::Title;
use umicraft_protocol::packets::play::clientbound::update_block_entity::UpdateBlockEntity;
use umicraft_protocol::packets::play::clientbound::update_score::UpdateScore;
use umicraft_protocol::packets::play::clientbound::update_sign::UpdateSign;
use umicraft_protocol::packets::play::clientbound::world_border::WorldBorder;
use umicraft_protocol::types::read_string;

use attributes::Attributes;
use block_entities::{BlockEntities, Pos};
use effects::Effects;
use maps::MapStore;
use player_list::PlayerList;
use scoreboard::Scoreboard;
use title::TitleState;
use weather::Weather;
use world_border::Border;

/// The plugin channel carrying the server brand string.
const BRAND_CHANNEL: &str = "MC|Brand";
/// Character limit of the brand string (MCP reads it with `readStringFromBuffer(32767)`).
const BRAND_MAX_CHARS: usize = 32767;
/// Most statistics kept; names are server-chosen.
const MAX_STATISTICS: usize = 65_536;

/// A resource pack offer (0x48). Downloading and applying it is up to the
/// client; the offer is only remembered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePackOffer {
    /// The pack URL.
    pub url: String,
    /// The SHA-1 the server announced (may be empty).
    pub hash: String,
}

/// All the server-announced session state, see the [module docs](self).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerState {
    local_entity_id: Option<i32>,
    effects: Effects,
    attributes: Attributes,
    abilities: Option<PlayerAbilities>,
    weather: Weather,
    difficulty: Option<u8>,
    border: Border,
    player_list: PlayerList,
    scoreboard: Scoreboard,
    title: TitleState,
    maps: MapStore,
    blocks: BlockEntities,
    statistics: BTreeMap<String, i32>,
    camera: Option<i32>,
    brand: Option<String>,
    death_message: Option<String>,
    sign_editor: Option<Pos>,
    resource_pack: Option<ResourcePackOffer>,
}

impl ServerState {
    /// An empty state, as before the first Join Game.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new session: forgets everything and remembers the entity id
    /// of the local player (from Join Game).
    pub fn on_join(&mut self, entity_id: i32) {
        *self = Self {
            local_entity_id: Some(entity_id),
            ..Self::default()
        };
    }

    /// Handles a Respawn (MCP `handleRespawn`). The server creates a new
    /// player entity, so the local effects, attributes, camera, death
    /// message and sign editor go. When the dimension changed the client also
    /// builds a new world: block entities, break animations, maps, the border
    /// and the weather go, while the scoreboard (explicitly carried over by
    /// the reference), the tab list and the statistics stay.
    pub fn on_respawn(&mut self, dimension_changed: bool) {
        self.effects.clear();
        self.attributes.clear();
        self.camera = None;
        self.death_message = None;
        self.sign_editor = None;
        if dimension_changed {
            self.blocks.clear();
            self.maps.clear();
            self.border = Border::default();
            self.weather.reset_world();
        }
    }

    /// Advances one 20 Hz tick: effect durations (`PotionEffect.onUpdate`),
    /// the title timer (`GuiIngame.updateTick`) and a running border resize.
    pub fn tick(&mut self) {
        self.effects.tick();
        self.title.tick();
        self.border.tick();
    }

    fn is_local(&self, entity_id: i32) -> bool {
        self.local_entity_id == Some(entity_id)
    }

    /// Stores the abilities (0x39).
    pub fn apply_abilities(&mut self, packet: &PlayerAbilities) {
        self.abilities = Some(*packet);
    }

    /// Adds an effect of the local player (0x1D); other entities are ignored.
    pub fn apply_entity_effect(&mut self, packet: &EntityEffect) {
        if self.is_local(packet.entity_id) {
            self.effects.apply(packet);
        }
    }

    /// Removes an effect of the local player (0x1E).
    pub fn apply_remove_entity_effect(&mut self, packet: &RemoveEntityEffect) {
        if self.is_local(packet.entity_id) {
            self.effects.remove(packet.effect_id);
        }
    }

    /// Updates the attributes of the local player (0x20).
    pub fn apply_entity_properties(&mut self, packet: &EntityProperties) {
        if self.is_local(packet.entity_id) {
            self.attributes.apply(packet);
        }
    }

    /// The computed value of a local attribute such as
    /// `generic.movementSpeed` (`ModifiableAttributeInstance.computeValue`).
    pub fn attribute_value(&self, key: &str) -> Option<f64> {
        self.attributes.value(key)
    }

    /// Applies a game state change (0x2B).
    pub fn apply_game_state(&mut self, packet: &ChangeGameState) {
        self.weather.apply(packet);
    }

    /// Stores the difficulty (0x41).
    pub fn apply_difficulty(&mut self, packet: &ServerDifficulty) {
        self.difficulty = Some(packet.difficulty);
    }

    /// Applies a world border packet (0x44).
    pub fn apply_world_border(&mut self, packet: &WorldBorder) {
        self.border.apply(packet);
    }

    /// Applies a tab-list packet (0x38).
    pub fn apply_player_list(&mut self, packet: &PlayerListItem) {
        self.player_list.apply(packet);
    }

    /// Stores the tab-list header and footer (0x47).
    pub fn apply_header_footer(&mut self, packet: &PlayerListHeaderFooter) {
        self.player_list.apply_header_footer(packet);
    }

    /// Applies an objective packet (0x3B).
    pub fn apply_objective(&mut self, packet: &ScoreboardObjective) {
        self.scoreboard.apply_objective(packet);
    }

    /// Applies a score packet (0x3C).
    pub fn apply_score(&mut self, packet: &UpdateScore) {
        self.scoreboard.apply_score(packet);
    }

    /// Applies a display-slot packet (0x3D).
    pub fn apply_display(&mut self, packet: &DisplayScoreboard) {
        self.scoreboard.apply_display(packet);
    }

    /// Applies a team packet (0x3E).
    pub fn apply_team(&mut self, packet: &Teams) {
        self.scoreboard.apply_team(packet);
    }

    /// Applies a title packet (0x45).
    pub fn apply_title(&mut self, packet: &Title) {
        self.title.apply(packet);
    }

    /// Stores sign text (0x33).
    pub fn apply_sign(&mut self, packet: &UpdateSign) {
        self.blocks.apply_sign(packet);
    }

    /// Stores a block entity update (0x35).
    pub fn apply_block_entity(&mut self, packet: &UpdateBlockEntity) {
        self.blocks.apply_update(packet);
    }

    /// Applies a block-break animation (0x25).
    pub fn apply_block_break_anim(&mut self, packet: &BlockBreakAnim) {
        self.blocks.apply_break_anim(packet);
    }

    /// Applies a map packet (0x34).
    pub fn apply_map(&mut self, packet: &Maps) {
        self.maps.apply(packet);
    }

    /// Merges statistics (0x37); a known name takes the new value.
    pub fn apply_statistics(&mut self, packet: &Statistics) {
        for (name, value) in &packet.entries {
            if self.statistics.len() < MAX_STATISTICS || self.statistics.contains_key(name) {
                self.statistics.insert(name.clone(), *value);
            }
        }
    }

    /// Switches the view to another entity (0x43). Pointing the camera at
    /// the local player means "back to normal".
    pub fn apply_camera(&mut self, packet: &Camera) {
        self.camera = (!self.is_local(packet.entity_id)).then_some(packet.entity_id);
    }

    /// Handles a plugin message (0x3F). Only `MC|Brand` is understood: its
    /// payload is a VarInt-length string. Other channels and a malformed
    /// brand are ignored (the crate has no logging).
    pub fn apply_plugin_message(&mut self, packet: &PluginMessage) {
        if packet.channel != BRAND_CHANNEL {
            return;
        }
        if let Ok(brand) = read_string(&mut packet.data.as_slice(), BRAND_MAX_CHARS) {
            self.brand = Some(brand);
        }
    }

    /// Handles a combat event (0x42): the death of the local player stores
    /// its message (chat JSON) for the death screen.
    pub fn apply_combat_event(&mut self, packet: &CombatEvent) {
        if let CombatEvent::EntityDied {
            player_id, message, ..
        } = packet
        {
            if self.is_local(*player_id) {
                self.death_message = Some(message.clone());
            }
        }
    }

    /// Remembers that the server wants the sign editor opened (0x36).
    pub fn apply_sign_editor_open(&mut self, packet: &SignEditorOpen) {
        self.sign_editor = Some((packet.x, packet.y, packet.z));
    }

    /// Remembers the last resource pack offer (0x48).
    pub fn apply_resource_pack_offer(&mut self, packet: &ResourcePackSend) {
        self.resource_pack = Some(ResourcePackOffer {
            url: packet.url.clone(),
            hash: packet.hash.clone(),
        });
    }

    /// The entity id of the local player.
    pub fn local_entity_id(&self) -> Option<i32> {
        self.local_entity_id
    }

    /// The local potion effects.
    pub fn effects(&self) -> &Effects {
        &self.effects
    }

    /// The local attributes.
    pub fn attributes(&self) -> &Attributes {
        &self.attributes
    }

    /// The last abilities packet.
    pub fn abilities(&self) -> Option<&PlayerAbilities> {
        self.abilities.as_ref()
    }

    /// Rain, thunder and game mode.
    pub fn weather(&self) -> &Weather {
        &self.weather
    }

    /// The server difficulty (0 peaceful .. 3 hard).
    pub fn difficulty(&self) -> Option<u8> {
        self.difficulty
    }

    /// The world border.
    pub fn border(&self) -> &Border {
        &self.border
    }

    /// The tab list with header and footer.
    pub fn player_list(&self) -> &PlayerList {
        &self.player_list
    }

    /// The scoreboard.
    pub fn scoreboard(&self) -> &Scoreboard {
        &self.scoreboard
    }

    /// The title overlay.
    pub fn title(&self) -> &TitleState {
        &self.title
    }

    /// The map items.
    pub fn maps(&self) -> &MapStore {
        &self.maps
    }

    /// Signs, block entities and break animations.
    pub fn block_entities(&self) -> &BlockEntities {
        &self.blocks
    }

    /// Mutable access, so the world code can drop the data of a block that
    /// was replaced ([`BlockEntities::remove`]).
    pub fn block_entities_mut(&mut self) -> &mut BlockEntities {
        &mut self.blocks
    }

    /// The statistics by name.
    pub fn statistics(&self) -> &BTreeMap<String, i32> {
        &self.statistics
    }

    /// The statistic with this name.
    pub fn statistic(&self, name: &str) -> Option<i32> {
        self.statistics.get(name).copied()
    }

    /// The entity the view is attached to; `None` is the local player.
    pub fn camera_target(&self) -> Option<i32> {
        self.camera
    }

    /// The server brand from `MC|Brand`.
    pub fn server_brand(&self) -> Option<&str> {
        self.brand.as_deref()
    }

    /// The death message (chat JSON) while the local player is dead.
    pub fn death_message(&self) -> Option<&str> {
        self.death_message.as_deref()
    }

    /// The sign the server asked to edit.
    pub fn pending_sign_editor(&self) -> Option<Pos> {
        self.sign_editor
    }

    /// Takes the pending sign editor request once the GUI handled it.
    pub fn take_sign_editor(&mut self) -> Option<Pos> {
        self.sign_editor.take()
    }

    /// The last resource pack offer.
    pub fn resource_pack_offer(&self) -> Option<&ResourcePackOffer> {
        self.resource_pack.as_ref()
    }
}

#[cfg(test)]
mod tests;
