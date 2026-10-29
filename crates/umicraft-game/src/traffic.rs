// SPDX-License-Identifier: MIT OR Apache-2.0

//! The serverbound packets of one player tick, in the vanilla order.
//!
//! MCP `EntityPlayerSP.onUpdate` decides what the 1.8 client tells the server
//! at the end of each tick, after the clicks of `Minecraft.runTick`:
//!
//! - **riding**: a `Player Look` (0x05) followed by `Steer Vehicle` (0x0C),
//!   nothing else;
//! - **on foot** (`onUpdateWalkingPlayer`): an `Entity Action` when the
//!   sprint state differs from the one last sent, then one for the sneak
//!   state, then exactly one movement packet — position and look (0x06),
//!   position (0x04), look (0x05) or the bare `Player` (0x03) when nothing
//!   changed.
//!
//! The position counts as moved when it changed by more than 0.03 blocks
//! (`9.0E-4` squared) or when 20 ticks passed since the last position
//! report; the look counts as changed on any difference of yaw or pitch.
//! The parity harness (ADR-0019) cuts the serverbound stream into ticks at
//! the movement packet, so a tick without one is a parity bug.
//!
//! [`ServerView`] is the client's memory of what the server was last told
//! (`serverSprintState`, `serverSneakState`, `lastReportedPos*`,
//! `lastReportedYaw/Pitch`, `positionUpdateTicks`); [`ServerView::tick`]
//! turns a [`TickSnapshot`] into the [`TickPackets`] of the tick.

use glam::DVec3;
use umicraft_protocol::packets::play::serverbound::EntityActionKind;

/// The squared distance above which the position counts as moved
/// (MCP `EntityPlayerSP.onUpdateWalkingPlayer`, `9.0E-4D`).
const MOVE_EPSILON_SQUARED: f64 = 9.0e-4;

/// The ticks after which the position is reported even without movement
/// (MCP `EntityPlayerSP.onUpdateWalkingPlayer`, `positionUpdateTicks >= 20`).
const REPORT_INTERVAL_TICKS: u32 = 20;

/// The most packets one tick produces: two entity actions and a movement
/// packet on foot, a look and a steer packet while riding.
const MAX_TICK_PACKETS: usize = 3;

/// The player state at the end of a tick, as the server is told about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickSnapshot {
    /// The feet position (the bottom of the bounding box).
    pub position: DVec3,
    /// The view yaw in degrees.
    pub yaw: f32,
    /// The view pitch in degrees.
    pub pitch: f32,
    /// Whether the player rests on the ground.
    pub on_ground: bool,
    /// Whether the player sprints.
    pub sprinting: bool,
    /// Whether the player sneaks.
    pub sneaking: bool,
    /// The movement keys while riding, `None` on foot.
    pub riding: Option<SteerInput>,
}

/// The movement keys a riding player sends with `Steer Vehicle` (0x0C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SteerInput {
    /// The strafe axis: positive to the left.
    pub strafe: f32,
    /// The forward axis: positive forwards.
    pub forward: f32,
    /// Whether the jump key is held.
    pub jump: bool,
    /// Whether the sneak key (dismount) is held.
    pub sneak: bool,
}

/// One serverbound packet of a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TickPacket {
    /// `Entity Action` (0x0B): a sprint or sneak transition.
    EntityAction(EntityActionKind),
    /// `Player Position and Look` (0x06).
    PositionLook {
        /// The feet position.
        position: DVec3,
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Position` (0x04).
    Position {
        /// The feet position.
        position: DVec3,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Look` (0x05).
    Look {
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player` (0x03): nothing moved, only the ground flag.
    Ground {
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Steer Vehicle` (0x0C): the keys of a riding player.
    SteerVehicle(SteerInput),
}

/// The packets of one tick, in the order they must be sent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TickPackets {
    packets: [Option<TickPacket>; MAX_TICK_PACKETS],
    len: usize,
}

impl TickPackets {
    /// Appends a packet; the tick rules never produce more than
    /// [`MAX_TICK_PACKETS`].
    fn push(&mut self, packet: TickPacket) {
        debug_assert!(self.len < MAX_TICK_PACKETS, "too many packets in a tick");
        if let Some(slot) = self.packets.get_mut(self.len) {
            *slot = Some(packet);
            self.len += 1;
        }
    }

    /// The packets in sending order.
    pub fn iter(&self) -> impl Iterator<Item = &TickPacket> {
        self.packets.iter().flatten()
    }

    /// Whether the tick reported the position (0x04 or 0x06).
    #[must_use]
    pub fn reports_position(&self) -> bool {
        self.iter().any(|packet| {
            matches!(
                packet,
                TickPacket::Position { .. } | TickPacket::PositionLook { .. }
            )
        })
    }
}

/// What the server was last told about the player.
///
/// ```
/// use glam::DVec3;
/// use umicraft_game::traffic::{ServerView, TickPacket, TickSnapshot};
///
/// let mut view = ServerView::default();
/// let standing = TickSnapshot {
///     position: DVec3::new(0.5, 64.0, 0.5),
///     yaw: 0.0,
///     pitch: 0.0,
///     on_ground: true,
///     sprinting: false,
///     sneaking: false,
///     riding: None,
/// };
/// // The first tick reports the position; the next one has nothing new.
/// assert!(view.tick(&standing).reports_position());
/// let packets: Vec<_> = view.tick(&standing).iter().copied().collect();
/// assert_eq!(packets, [TickPacket::Ground { on_ground: true }]);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ServerView {
    sprinting: bool,
    sneaking: bool,
    position: Option<DVec3>,
    rotation: Option<(f32, f32)>,
    ticks_since_report: u32,
}

impl ServerView {
    /// Computes the packets of the tick and records what they tell the
    /// server.
    pub fn tick(&mut self, snapshot: &TickSnapshot) -> TickPackets {
        let mut packets = TickPackets::default();
        match snapshot.riding {
            Some(steer) => {
                // MCP `EntityPlayerSP.onUpdate`: the riding branch skips
                // `onUpdateWalkingPlayer` and its bookkeeping entirely.
                packets.push(TickPacket::Look {
                    yaw: snapshot.yaw,
                    pitch: snapshot.pitch,
                    on_ground: snapshot.on_ground,
                });
                packets.push(TickPacket::SteerVehicle(steer));
            }
            None => self.walking_tick(snapshot, &mut packets),
        }
        packets
    }

    /// Forgets the reported position so the next tick reports it again.
    ///
    /// Used after a server placement: the answer to the placement already
    /// told the server where the player is, but from a position this view
    /// did not produce.
    pub fn forget_position(&mut self) {
        self.position = None;
    }

    /// MCP `EntityPlayerSP.onUpdateWalkingPlayer`.
    fn walking_tick(&mut self, snapshot: &TickSnapshot, packets: &mut TickPackets) {
        if snapshot.sprinting != self.sprinting {
            packets.push(TickPacket::EntityAction(if snapshot.sprinting {
                EntityActionKind::StartSprinting
            } else {
                EntityActionKind::StopSprinting
            }));
            self.sprinting = snapshot.sprinting;
        }
        if snapshot.sneaking != self.sneaking {
            packets.push(TickPacket::EntityAction(if snapshot.sneaking {
                EntityActionKind::StartSneaking
            } else {
                EntityActionKind::StopSneaking
            }));
            self.sneaking = snapshot.sneaking;
        }

        let moved = self
            .position
            .is_none_or(|last| last.distance_squared(snapshot.position) > MOVE_EPSILON_SQUARED)
            || self.ticks_since_report >= REPORT_INTERVAL_TICKS;
        let looked = self.rotation != Some((snapshot.yaw, snapshot.pitch));
        let on_ground = snapshot.on_ground;
        packets.push(match (moved, looked) {
            (true, true) => TickPacket::PositionLook {
                position: snapshot.position,
                yaw: snapshot.yaw,
                pitch: snapshot.pitch,
                on_ground,
            },
            (true, false) => TickPacket::Position {
                position: snapshot.position,
                on_ground,
            },
            (false, true) => TickPacket::Look {
                yaw: snapshot.yaw,
                pitch: snapshot.pitch,
                on_ground,
            },
            (false, false) => TickPacket::Ground { on_ground },
        });

        self.ticks_since_report += 1;
        if moved {
            self.position = Some(snapshot.position);
            self.ticks_since_report = 0;
        }
        if looked {
            self.rotation = Some((snapshot.yaw, snapshot.pitch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> TickSnapshot {
        TickSnapshot {
            position: DVec3::new(0.5, 64.0, 0.5),
            yaw: 90.0,
            pitch: 10.0,
            on_ground: true,
            sprinting: false,
            sneaking: false,
            riding: None,
        }
    }

    fn packets(view: &mut ServerView, snapshot: &TickSnapshot) -> Vec<TickPacket> {
        view.tick(snapshot).iter().copied().collect()
    }

    #[test]
    fn the_first_tick_reports_position_and_look() {
        let mut view = ServerView::default();
        assert!(matches!(
            packets(&mut view, &standing())[..],
            [TickPacket::PositionLook { .. }]
        ));
    }

    #[test]
    fn an_idle_tick_sends_the_bare_player_packet() {
        let mut view = ServerView::default();
        view.tick(&standing());
        assert_eq!(
            packets(&mut view, &standing()),
            [TickPacket::Ground { on_ground: true }]
        );
    }

    #[test]
    fn an_idle_player_reports_the_position_after_twenty_idle_ticks() {
        // The counter is reset *after* the increment of the reporting tick,
        // so twenty bare ticks separate two reports (regression: the old
        // client reported after nineteen).
        let mut view = ServerView::default();
        view.tick(&standing());
        for _ in 0..20 {
            assert!(!view.tick(&standing()).reports_position());
        }
        assert_eq!(
            packets(&mut view, &standing()),
            [TickPacket::Position {
                position: standing().position,
                on_ground: true
            }]
        );
    }

    #[test]
    fn a_move_below_three_centimetres_is_not_reported() {
        let mut view = ServerView::default();
        view.tick(&standing());
        let mut nudged = standing();
        nudged.position.x += 0.029;
        assert_eq!(
            packets(&mut view, &nudged),
            [TickPacket::Ground { on_ground: true }]
        );
        nudged.position.x += 0.002;
        assert!(view.tick(&nudged).reports_position());
    }

    #[test]
    fn turning_alone_sends_a_look() {
        let mut view = ServerView::default();
        view.tick(&standing());
        let mut turned = standing();
        turned.yaw += 1.0;
        assert_eq!(
            packets(&mut view, &turned),
            [TickPacket::Look {
                yaw: turned.yaw,
                pitch: turned.pitch,
                on_ground: true
            }]
        );
    }

    #[test]
    fn transitions_precede_the_movement_packet_sprint_first() {
        let mut view = ServerView::default();
        view.tick(&standing());
        let mut both = standing();
        both.sprinting = true;
        both.sneaking = true;
        assert_eq!(
            packets(&mut view, &both),
            [
                TickPacket::EntityAction(EntityActionKind::StartSprinting),
                TickPacket::EntityAction(EntityActionKind::StartSneaking),
                TickPacket::Ground { on_ground: true },
            ]
        );
        // The state is only announced once, and its end is announced too.
        assert_eq!(
            packets(&mut view, &both),
            [TickPacket::Ground { on_ground: true }]
        );
        assert_eq!(
            packets(&mut view, &standing()),
            [
                TickPacket::EntityAction(EntityActionKind::StopSprinting),
                TickPacket::EntityAction(EntityActionKind::StopSneaking),
                TickPacket::Ground { on_ground: true },
            ]
        );
    }

    #[test]
    fn a_riding_tick_sends_look_then_steer_only() {
        let mut view = ServerView::default();
        let steer = SteerInput {
            strafe: 0.0,
            forward: 0.98,
            jump: false,
            sneak: false,
        };
        let riding = TickSnapshot {
            riding: Some(steer),
            sprinting: true,
            ..standing()
        };
        assert_eq!(
            packets(&mut view, &riding),
            [
                TickPacket::Look {
                    yaw: riding.yaw,
                    pitch: riding.pitch,
                    on_ground: true
                },
                TickPacket::SteerVehicle(steer),
            ]
        );
    }

    #[test]
    fn steer_vehicle_is_never_sent_on_foot() {
        let mut view = ServerView::default();
        let mut snapshot = standing();
        for tick in 0..40 {
            snapshot.position.z += f64::from(tick % 3) * 0.1;
            snapshot.sprinting = tick % 5 == 0;
            assert!(!view
                .tick(&snapshot)
                .iter()
                .any(|packet| matches!(packet, TickPacket::SteerVehicle(_))));
        }
    }

    #[test]
    fn a_forgotten_position_is_reported_again() {
        let mut view = ServerView::default();
        view.tick(&standing());
        view.forget_position();
        assert!(view.tick(&standing()).reports_position());
    }
}
