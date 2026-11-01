// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread knows for the screen overlays (P8-f).
//!
//! The decisions that need the world, the inventory and the player live here
//! as small pure functions; [`crate::overlay_view`] turns the result into
//! drawable data on the render thread.
//!
//! Sources (MCP 1.8): `Entity.isEntityInsideOpaqueBlock` and
//! `ItemRenderer.renderOverlays` (the block in the way), `EntityPlayerSP.
//! onLivingUpdate` (the portal timer), `Entity.func_181015_d` and
//! `BlockPortal.onEntityCollidedWithBlock` (entering a portal),
//! `GuiIngame.renderGameOverlay` (the pumpkin) and `GuiIngame.renderVignette`
//! (the border warning).

use glam::DVec3;
use umicraft_game::entity::flags::EntityFlags;
use umicraft_game::physics::{PLAYER_HALF_WIDTH, PLAYER_HEIGHT, PLAYER_WIDTH};
use umicraft_game::server::world_border::Border;
use umicraft_protocol::packets::play::Metadata;
use umicraft_world::{BlockPos, BlockState, World};

/// Item id of the pumpkin block (`Item.getItemFromBlock(Blocks.pumpkin)`).
const PUMPKIN_ITEM: i16 = 86;
/// Block id of the nether portal.
const PORTAL_BLOCK: u16 = 90;
/// Metadata index of the entity flags (burning is bit 0).
const FLAGS_INDEX: u8 = 0;
/// Ticks of protection after a portal touch (`EntityPlayer.getPortalCooldown`).
const PORTAL_COOLDOWN: u32 = 10;
/// Growth of the portal timer per tick inside a portal.
const PORTAL_GROWTH: f32 = 0.012_5;
/// Growth per tick under a nausea effect with more than 60 ticks left.
const NAUSEA_GROWTH: f32 = 0.006_666_667;
/// Decay per tick outside a portal.
const PORTAL_DECAY: f32 = 0.05;
/// A nausea effect only feeds the timer while it has more than this left.
const NAUSEA_FEED_TICKS: u32 = 60;
/// Skin of an inset around the player's box, so touching a face does not count
/// (`Entity.doBlockCollisions` shrinks the box by 0.001).
const BOX_INSET: f64 = 0.001;
/// Offsets, in `0..2` steps, of the eight points that test for a block at the
/// eye: `(bit - 0.5)` scaled by 0.1 vertically and by the width times 0.8
/// horizontally.
const EYE_PROBES: u8 = 8;

/// The portal timer of the previous and the newest tick, which the render
/// thread interpolates (`prevTimeInPortal`, `timeInPortal`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PortalTime {
    /// The timer one tick ago.
    pub previous: f32,
    /// The timer now.
    pub current: f32,
}

/// What the game thread publishes for the overlays, once per tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OverlayInputs {
    /// The camera is in the eyes of the player (`thirdPersonView == 0`).
    pub first_person: bool,
    /// The player spectates (`isSpectator`): no water or fire overlay.
    pub spectator: bool,
    /// The opaque block the eye is inside, if any.
    pub in_wall: Option<BlockState>,
    /// The player burns (flag 0 of its entity metadata).
    pub on_fire: bool,
    /// A pumpkin is worn on the head.
    pub pumpkin: bool,
    /// The nether portal timer.
    pub portal: PortalTime,
    /// The player is nauseous, which replaces the portal veil by the warp.
    pub nauseous: bool,
    /// How close the player is to the border's warning, `0.0..=1.0`; the
    /// vignette turns red-blue when above zero.
    pub border_danger: f32,
}

/// The nether portal timer of the local player.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PortalTimer {
    time: PortalTime,
    /// A portal block touched the player's box on the previous tick
    /// (`Entity.inPortal`).
    in_portal: bool,
    /// Ticks left of the cooldown (`Entity.timeUntilPortal`).
    cooldown: u32,
}

impl PortalTimer {
    /// One tick of `EntityPlayerSP.onLivingUpdate`; `nausea_ticks` is the
    /// time left of the nausea effect.
    fn tick(&mut self, nausea_ticks: Option<u32>) {
        self.time.previous = self.time.current;
        let current = self.time.current;
        self.time.current = if self.in_portal {
            self.in_portal = false;
            (current + PORTAL_GROWTH).min(1.0)
        } else if nausea_ticks.is_some_and(|ticks| ticks > NAUSEA_FEED_TICKS) {
            (current + NAUSEA_GROWTH).min(1.0)
        } else {
            (current - PORTAL_DECAY).max(0.0)
        };
        self.cooldown = self.cooldown.saturating_sub(1);
    }

    /// The player's box touched a portal block this tick
    /// (`Entity.func_181015_d`): during the cooldown it only renews it.
    fn touch(&mut self) {
        if self.cooldown > 0 {
            self.cooldown = PORTAL_COOLDOWN;
        } else {
            self.in_portal = true;
        }
    }
}

/// The facts of one tick the overlays depend on.
#[derive(Debug, Clone, Copy)]
pub struct PlayerFacts<'a> {
    /// The loaded world.
    pub world: &'a World,
    /// The feet of the player.
    pub feet: DVec3,
    /// The eyes of the player.
    pub eye: DVec3,
    /// The player spectates.
    pub spectator: bool,
    /// The player rides something (a rider never enters a portal).
    pub riding: bool,
    /// Ticks left of the nausea effect, if it is active.
    pub nausea_ticks: Option<u32>,
    /// The item id worn on the head.
    pub helmet: Option<i16>,
    /// The border of the dimension.
    pub border: &'a Border,
}

/// The overlay state of the local player, advanced once per tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayTracker {
    portal: PortalTimer,
    on_fire: bool,
    spectator: bool,
    in_wall: Option<BlockState>,
    pumpkin: bool,
    nauseous: bool,
    border_danger: f32,
}

impl OverlayTracker {
    /// Forgets everything: a respawn creates a new player entity.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Reads the burning flag from the metadata of the local player.
    pub fn apply_metadata(&mut self, metadata: &Metadata) {
        if let Some(flags) = metadata.get_byte(FLAGS_INDEX) {
            self.on_fire = EntityFlags::new(flags, 0).on_fire();
        }
    }

    /// Advances one tick.
    pub fn tick(&mut self, facts: &PlayerFacts<'_>) {
        self.portal.tick(facts.nausea_ticks);
        if !facts.riding && touches_portal(facts.world, facts.feet) {
            self.portal.touch();
        }
        self.spectator = facts.spectator;
        // A spectator is `noClip`, which `isEntityInsideOpaqueBlock` honours.
        self.in_wall = if facts.spectator {
            None
        } else {
            block_in_the_way(facts.world, facts.feet, facts.eye)
        };
        self.pumpkin = facts.helmet == Some(PUMPKIN_ITEM);
        self.nauseous = facts.nausea_ticks.is_some();
        self.border_danger = border_danger(facts.border, facts.feet.x, facts.feet.z);
    }

    /// The inputs to publish; `first_person` is the perspective of the camera.
    #[must_use]
    pub fn inputs(&self, first_person: bool) -> OverlayInputs {
        OverlayInputs {
            first_person,
            spectator: self.spectator,
            in_wall: self.in_wall,
            on_fire: self.on_fire,
            pumpkin: self.pumpkin,
            portal: self.portal.time,
            nauseous: self.nauseous,
            border_danger: self.border_danger,
        }
    }
}

/// The opaque block the eye is inside, as `ItemRenderer.renderOverlays` picks
/// it: the last of eight points around the eye, spread by 0.1 vertically and by
/// 0.8 of the player's width horizontally, that lies in a visually opaque block.
#[must_use]
pub fn block_in_the_way(world: &World, feet: DVec3, eye: DVec3) -> Option<BlockState> {
    let spread = PLAYER_WIDTH * 0.8;
    let offset = |bit: u8, size: f64| (f64::from(bit & 1) - 0.5) * size;
    let mut found = None;
    for probe in 0..EYE_PROBES {
        let pos = BlockPos::new(
            (feet.x + offset(probe, spread)).floor() as i32,
            (eye.y + offset(probe >> 1, 0.1)).floor() as i32,
            (feet.z + offset(probe >> 2, spread)).floor() as i32,
        );
        let state = world.block(pos);
        if state.is_opaque() {
            found = Some(state);
        }
    }
    found
}

/// Whether the player's box, shrunk by a hair, overlaps a portal block.
#[must_use]
pub fn touches_portal(world: &World, feet: DVec3) -> bool {
    let low = |centre: f64| (centre - PLAYER_HALF_WIDTH + BOX_INSET).floor() as i32;
    let high = |centre: f64| (centre + PLAYER_HALF_WIDTH - BOX_INSET).floor() as i32;
    let (y_low, y_high) = (
        (feet.y + BOX_INSET).floor() as i32,
        (feet.y + PLAYER_HEIGHT - BOX_INSET).floor() as i32,
    );
    (low(feet.x)..=high(feet.x)).any(|x| {
        (y_low..=y_high).any(|y| {
            (low(feet.z)..=high(feet.z))
                .any(|z| world.block(BlockPos::new(x, y, z)).id() == PORTAL_BLOCK)
        })
    })
}

/// How far into the border's warning zone `(x, z)` stands, `0.0..=1.0`
/// (`GuiIngame.renderVignette`): the zone is the warning distance, or the
/// distance the border covers in the warning time while it moves, if larger.
#[must_use]
pub fn border_danger(border: &Border, x: f64, z: f64) -> f32 {
    let distance = border.closest_distance(x, z);
    let covered = (border.resize_speed() * f64::from(border.warning_time()) * 1000.0)
        .min((border.target_diameter() - border.diameter()).abs());
    let zone = f64::from(border.warning_blocks()).max(covered);
    if distance < zone {
        (1.0 - distance / zone) as f32
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::world_border::WorldBorder as BorderPacket;
    use umicraft_protocol::packets::play::metadata::WatchedObject;
    use umicraft_world::{ChunkPayload, ChunkPos};

    /// An empty column (one section, heights 0 to 15) with `blocks` set.
    fn world_with(blocks: &[(BlockPos, BlockState)]) -> World {
        let mut world = World::new(0);
        let data = vec![0u8; 8192 + 2048 + 2048 + 256];
        let payload = ChunkPayload {
            primary_bit_mask: 1,
            ground_up: true,
            data: &data,
        };
        world.apply_chunk(ChunkPos::new(0, 0), payload).unwrap();
        for (pos, state) in blocks {
            world.set_block(*pos, *state);
        }
        world
    }

    const STONE: BlockState = BlockState::new(1, 0);
    const PORTAL: BlockState = BlockState::new(90, 0);

    #[test]
    fn an_open_eye_sees_no_wall() {
        let world = world_with(&[]);
        let feet = DVec3::new(8.5, 3.0, 8.5);
        assert_eq!(block_in_the_way(&world, feet, feet + DVec3::Y * 1.62), None);
    }

    #[test]
    fn an_eye_in_stone_shows_the_stone() {
        let world = world_with(&[(BlockPos::new(8, 4, 8), STONE)]);
        let feet = DVec3::new(8.5, 3.0, 8.5);
        assert_eq!(
            block_in_the_way(&world, feet, feet + DVec3::Y * 1.62),
            Some(STONE)
        );
    }

    #[test]
    fn the_last_opaque_point_wins() {
        // Two stones with different metadata on either side of the eye.
        let low = BlockState::new(1, 1);
        let high = BlockState::new(1, 2);
        let world = world_with(&[
            (BlockPos::new(8, 4, 8), low),
            (BlockPos::new(9, 4, 8), high),
        ]);
        // The points at -0.24 and +0.24 around x = 8.9 straddle blocks 8 and 9,
        // and the last probe is on the positive side.
        let feet = DVec3::new(8.9, 3.0, 8.5);
        assert_eq!(
            block_in_the_way(&world, feet, feet + DVec3::Y * 1.62),
            Some(high)
        );
    }

    #[test]
    fn a_portal_block_in_the_box_counts_and_one_beside_it_does_not() {
        let world = world_with(&[(BlockPos::new(8, 4, 8), PORTAL)]);
        assert!(touches_portal(&world, DVec3::new(8.5, 3.0, 8.5)));
        // Feet one block over: the box ends 0.3 from the portal's face.
        assert!(!touches_portal(&world, DVec3::new(9.8, 3.0, 8.5)));
        // Standing on top: the box starts at the top face, shrunk by a hair.
        assert!(!touches_portal(&world, DVec3::new(8.5, 5.0, 8.5)));
    }

    #[test]
    fn the_timer_grows_in_a_portal_and_fades_outside() {
        let mut timer = PortalTimer::default();
        timer.touch();
        timer.tick(None);
        timer.touch();
        timer.tick(None);
        assert_eq!(timer.time.previous, 0.012_5);
        assert_eq!(timer.time.current, 0.025);
        timer.tick(None);
        assert!((timer.time.current - 0.0).abs() < 1e-6, "{:?}", timer.time);
    }

    #[test]
    fn the_timer_stops_at_one_after_eighty_ticks() {
        let mut timer = PortalTimer::default();
        for _ in 0..100 {
            timer.touch();
            timer.tick(None);
        }
        assert_eq!(timer.time.current, 1.0);
    }

    #[test]
    fn a_long_nausea_feeds_the_timer_and_a_short_one_does_not() {
        let mut timer = PortalTimer::default();
        timer.tick(Some(61));
        assert_eq!(timer.time.current, NAUSEA_GROWTH);
        timer.tick(Some(60));
        assert_eq!(timer.time.current, 0.0);
    }

    #[test]
    fn the_cooldown_blocks_a_new_entry_and_renews_while_touching() {
        let mut timer = PortalTimer {
            cooldown: 3,
            ..PortalTimer::default()
        };
        timer.touch();
        assert!(!timer.in_portal);
        assert_eq!(timer.cooldown, PORTAL_COOLDOWN);
        timer.tick(None);
        assert_eq!(timer.cooldown, PORTAL_COOLDOWN - 1);
    }

    #[test]
    fn the_border_is_quiet_far_away_and_warns_close_to_it() {
        let mut border = Border::default();
        border.apply(&BorderPacket::SetSize { diameter: 100.0 });
        assert_eq!(border_danger(&border, 0.0, 0.0), 0.0);
        // Walls at 50; the warning zone is the default 5 blocks.
        assert_eq!(border_danger(&border, 47.5, 0.0), 0.5);
        assert_eq!(border_danger(&border, 50.0, 0.0), 1.0);
    }

    #[test]
    fn a_shrinking_border_widens_the_warning_with_its_speed() {
        let mut border = Border::default();
        border.apply(&BorderPacket::LerpSize {
            old_diameter: 200.0,
            new_diameter: 100.0,
            speed: 100_000,
        });
        // 1 block per second for 15 s: the zone grows to 15 blocks, short of
        // the 100 blocks still to shrink.
        let wall = border.diameter() / 2.0;
        assert!(border_danger(&border, wall - 7.5, 0.0) > 0.4);
    }

    #[test]
    fn the_tracker_publishes_what_it_saw() {
        let world = world_with(&[(BlockPos::new(8, 4, 8), STONE)]);
        let border = Border::default();
        let feet = DVec3::new(8.5, 3.0, 8.5);
        let facts = PlayerFacts {
            world: &world,
            feet,
            eye: feet + DVec3::Y * 1.62,
            spectator: false,
            riding: false,
            nausea_ticks: None,
            helmet: Some(PUMPKIN_ITEM),
            border: &border,
        };
        let mut tracker = OverlayTracker::default();
        tracker.tick(&facts);
        let inputs = tracker.inputs(true);
        assert!(inputs.first_person && inputs.pumpkin && !inputs.on_fire);
        assert_eq!(inputs.in_wall, Some(STONE));
        // A spectator clips through walls.
        tracker.tick(&PlayerFacts {
            spectator: true,
            ..facts
        });
        assert_eq!(tracker.inputs(true).in_wall, None);
    }

    #[test]
    fn a_rider_never_enters_a_portal() {
        let world = world_with(&[(BlockPos::new(8, 3, 8), PORTAL)]);
        let border = Border::default();
        let feet = DVec3::new(8.5, 3.0, 8.5);
        let facts = PlayerFacts {
            world: &world,
            feet,
            eye: feet + DVec3::Y * 1.62,
            spectator: false,
            riding: true,
            nausea_ticks: None,
            helmet: None,
            border: &border,
        };
        let mut tracker = OverlayTracker::default();
        tracker.tick(&facts);
        tracker.tick(&facts);
        assert_eq!(tracker.inputs(true).portal.current, 0.0);
        tracker.tick(&PlayerFacts {
            riding: false,
            ..facts
        });
        tracker.tick(&facts);
        assert!(tracker.inputs(true).portal.current > 0.0);
    }

    #[test]
    fn a_metadata_flag_sets_the_fire_and_a_reset_puts_it_out() {
        let mut tracker = OverlayTracker::default();
        tracker.apply_metadata(&Metadata {
            objects: vec![WatchedObject {
                type_id: 0,
                index: 0,
                value: vec![1],
            }],
        });
        assert!(tracker.inputs(true).on_fire);
        tracker.reset();
        assert!(!tracker.inputs(true).on_fire);
    }
}
