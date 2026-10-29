// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packet-coverage audit: every protocol-47 play packet id, with its state.
//!
//! The id lists mirror the `PLAY` constant of
//! `mcp:net/minecraft/network/EnumConnectionState.java` (registration index =
//! wire id): clientbound `0x00..=0x49`, serverbound `0x00..=0x19`. The packet
//! names are the MCP/wiki names; gap descriptions come from
//! `docs/roadmap-review.md` section 3.1, re-verified against the code.
//!
//! Each id is in exactly one state ([`Coverage`]):
//!
//! - `Handled`: clientbound, the session decodes it and the client acts on
//!   it; serverbound, the client encodes it and sends it when vanilla would.
//! - `Forwarded`: clientbound only; decoded and forwarded, its state kept by
//!   the game thread, the consumer arrives in the named milestone.
//! - `Encoded`: serverbound only; encodable, the feature that sends it
//!   arrives in the named milestone.
//! - `Ignored`: deliberately not acted on; the reason says why it is safe.
//!
//! P1 (ROADMAP.md, ADR-0020) closed the gaps of `docs/roadmap-review.md`
//! §3.1: no id is silently skipped any more. The tests keep the tables
//! honest — complete, ordered, in agreement with the session and with
//! `PlayServerbound`, and every deferred row names its owner milestone.

use umicraft_net::play::{PlayOptions, PlaySession, Reaction};
use umicraft_protocol::packets::play::serverbound;

/// The state of one packet id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coverage {
    /// Fully supported.
    Handled,
    /// Intentionally not acted on; the reason says why that is safe.
    Ignored(&'static str),
    /// Clientbound: decoded and forwarded (its state kept by the game
    /// thread); the text names the milestone of its consumer (ADR-0020 §5).
    Forwarded(&'static str),
    /// Serverbound: encodable; the text names the milestone of the feature
    /// that sends it (ADR-0020 §5).
    Encoded(&'static str),
}

impl Coverage {
    /// Why the row is wrong for its direction, or `None` when it is valid.
    ///
    /// A deferred row must name the milestone that owns the rest of the work
    /// (`(P<n>)`), and each deferral kind belongs to one direction.
    fn problem(self, serverbound: bool) -> Option<&'static str> {
        match self {
            Self::Handled | Self::Ignored(_) => None,
            Self::Forwarded(_) if serverbound => Some("Forwarded is clientbound only"),
            Self::Encoded(_) if !serverbound => Some("Encoded is serverbound only"),
            Self::Forwarded(why) | Self::Encoded(why) => {
                (!names_milestone(why)).then_some("the deferral names no owner milestone")
            }
        }
    }

    /// Whether the clientbound session runs the packet's decoder.
    fn session_decodes(self) -> bool {
        matches!(self, Self::Handled | Self::Forwarded(_))
    }
}

/// Whether a deferral text names a parity milestone, e.g. `(P5)` or `(P2, P9)`.
fn names_milestone(why: &str) -> bool {
    why.split('P')
        .skip(1)
        .any(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// One row of a coverage table.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// Wire packet id.
    id: u8,
    /// MCP/wiki packet name.
    name: &'static str,
    /// Support state.
    coverage: Coverage,
}

impl Row {
    /// Builds a row; keeps the tables one line per packet.
    const fn new(id: u8, name: &'static str, coverage: Coverage) -> Self {
        Self { id, name, coverage }
    }
}

/// Clientbound play packets, ids `0x00..=0x49`.
const CLIENTBOUND: [Row; 0x4A] = [
    Row::new(0x00, "Keep Alive", Coverage::Handled),
    Row::new(0x01, "Join Game", Coverage::Handled),
    Row::new(0x02, "Chat Message", Coverage::Handled),
    Row::new(0x03, "Time Update", Coverage::Handled),
    Row::new(0x04, "Entity Equipment", Coverage::Handled),
    Row::new(0x05, "Spawn Position", Coverage::Handled),
    Row::new(0x06, "Update Health", Coverage::Handled),
    Row::new(0x07, "Respawn", Coverage::Handled),
    Row::new(0x08, "Player Position And Look", Coverage::Handled),
    Row::new(0x09, "Held Item Change", Coverage::Handled),
    Row::new(0x0A, "Use Bed", Coverage::Forwarded("sleeping pose (P5)")),
    Row::new(0x0B, "Animation", Coverage::Forwarded("swing, hurt and crit animations (P5)")),
    Row::new(0x0C, "Spawn Player", Coverage::Handled),
    Row::new(0x0D, "Collect Item", Coverage::Forwarded("pickup animation (P5)")),
    Row::new(0x0E, "Spawn Object", Coverage::Handled),
    Row::new(0x0F, "Spawn Mob", Coverage::Handled),
    Row::new(0x10, "Spawn Painting", Coverage::Forwarded("paintings (P5)")),
    Row::new(0x11, "Spawn Experience Orb", Coverage::Forwarded("experience orbs (P5)")),
    Row::new(0x12, "Entity Velocity", Coverage::Forwarded("local knockback (P2); other entities' motion (P5)")),
    Row::new(0x13, "Destroy Entities", Coverage::Handled),
    Row::new(0x14, "Entity", Coverage::Ignored("the base S14PacketEntity carries only an entity id and moves nothing; vanilla's handler adds a zero delta")),
    Row::new(0x15, "Entity Relative Move", Coverage::Handled),
    Row::new(0x16, "Entity Look", Coverage::Handled),
    Row::new(0x17, "Entity Look And Move", Coverage::Handled),
    Row::new(0x18, "Entity Teleport", Coverage::Handled),
    Row::new(0x19, "Entity Head Look", Coverage::Handled),
    Row::new(0x1A, "Entity Status", Coverage::Forwarded("hurt, death and eating statuses (P5)")),
    Row::new(0x1B, "Attach Entity", Coverage::Forwarded("local riding drives the traffic; passengers and leashes (P5)")),
    Row::new(0x1C, "Entity Metadata", Coverage::Handled),
    Row::new(0x1D, "Entity Effect", Coverage::Forwarded("kept in ServerState; effects in movement (P2) and the HUD (P9)")),
    Row::new(0x1E, "Remove Entity Effect", Coverage::Forwarded("kept in ServerState; effects in movement (P2)")),
    Row::new(0x1F, "Set Experience", Coverage::Handled),
    Row::new(0x20, "Entity Properties", Coverage::Forwarded("kept in ServerState; movement speed attribute (P2)")),
    Row::new(0x21, "Chunk Data", Coverage::Handled),
    Row::new(0x22, "Multi Block Change", Coverage::Handled),
    Row::new(0x23, "Block Change", Coverage::Handled),
    Row::new(0x24, "Block Action", Coverage::Forwarded("chest lids, note blocks, pistons (P7)")),
    Row::new(0x25, "Block Break Animation", Coverage::Forwarded("kept in ServerState; crack overlay (P2)")),
    Row::new(0x26, "Map Chunk Bulk", Coverage::Handled),
    Row::new(0x27, "Explosion", Coverage::Forwarded("blocks removed on the client; knockback (P2), particles (P8)")),
    Row::new(0x28, "Effect", Coverage::Forwarded("block-break, door and record effects (P8, P11)")),
    Row::new(0x29, "Sound Effect", Coverage::Forwarded("sounds (P11)")),
    Row::new(0x2A, "Particles", Coverage::Forwarded("server particles (P8)")),
    Row::new(0x2B, "Change Game State", Coverage::Forwarded("kept in ServerState; weather (P7), gamemode (P2)")),
    Row::new(0x2C, "Spawn Global Entity", Coverage::Forwarded("lightning (P5)")),
    Row::new(0x2D, "Open Window", Coverage::Handled),
    Row::new(0x2E, "Close Window", Coverage::Handled),
    Row::new(0x2F, "Set Slot", Coverage::Handled),
    Row::new(0x30, "Window Items", Coverage::Handled),
    Row::new(0x31, "Window Property", Coverage::Handled),
    Row::new(0x32, "Confirm Transaction", Coverage::Handled),
    Row::new(0x33, "Update Sign", Coverage::Forwarded("kept in ServerState; sign text rendering (P7)")),
    Row::new(0x34, "Maps", Coverage::Forwarded("kept in ServerState; maps in hand and frames (P3, P7)")),
    Row::new(0x35, "Update Tile Entity", Coverage::Forwarded("kept in ServerState; skulls, banners, spawners (P7)")),
    Row::new(0x36, "Open Sign Editor", Coverage::Forwarded("kept in ServerState; sign editor screen (P10)")),
    Row::new(0x37, "Statistics", Coverage::Forwarded("kept in ServerState; statistics screen (P10)")),
    Row::new(0x38, "Player List Item", Coverage::Forwarded("kept in ServerState; tab list (P9), skins (P5)")),
    Row::new(0x39, "Player Abilities", Coverage::Forwarded("kept in ServerState; flying and creative movement (P2)")),
    Row::new(0x3A, "Tab Complete", Coverage::Forwarded("chat completion (P9)")),
    Row::new(0x3B, "Scoreboard Objective", Coverage::Forwarded("kept in ServerState; sidebar (P9)")),
    Row::new(0x3C, "Update Score", Coverage::Forwarded("kept in ServerState; sidebar (P9)")),
    Row::new(0x3D, "Display Scoreboard", Coverage::Forwarded("kept in ServerState; sidebar (P9)")),
    Row::new(0x3E, "Teams", Coverage::Forwarded("kept in ServerState; prefixes and name tags (P5, P9)")),
    Row::new(0x3F, "Custom Payload", Coverage::Forwarded("server brand kept in ServerState; other channels (P4, P12)")),
    Row::new(0x40, "Disconnect", Coverage::Handled),
    Row::new(0x41, "Server Difficulty", Coverage::Forwarded("kept in ServerState; options screen (P10)")),
    Row::new(0x42, "Combat Event", Coverage::Forwarded("death message kept in ServerState; death screen (P2)")),
    Row::new(0x43, "Camera", Coverage::Forwarded("kept in ServerState; spectator camera (P2)")),
    Row::new(0x44, "World Border", Coverage::Forwarded("kept in ServerState; border rendering (P7)")),
    Row::new(0x45, "Title", Coverage::Forwarded("kept in ServerState; title rendering (P9)")),
    Row::new(0x46, "Set Compression", Coverage::Handled),
    Row::new(0x47, "Player List Header And Footer", Coverage::Forwarded("kept in ServerState; tab list (P9)")),
    Row::new(0x48, "Resource Pack Send", Coverage::Handled),
    Row::new(0x49, "Update Entity NBT", Coverage::Forwarded("no 1.8 server entity uses it on the client (P5)")),
];

/// Serverbound play packets, ids `0x00..=0x19`.
const SERVERBOUND: [Row; 0x1A] = [
    Row::new(0x00, "Keep Alive", Coverage::Handled),
    Row::new(0x01, "Chat Message", Coverage::Handled),
    Row::new(
        0x02,
        "Use Entity",
        Coverage::Encoded("attack and interact (P6)"),
    ),
    Row::new(0x03, "Player", Coverage::Handled),
    Row::new(0x04, "Player Position", Coverage::Handled),
    Row::new(0x05, "Player Look", Coverage::Handled),
    Row::new(0x06, "Player Position And Look", Coverage::Handled),
    Row::new(0x07, "Player Digging", Coverage::Handled),
    Row::new(0x08, "Player Block Placement", Coverage::Handled),
    Row::new(0x09, "Held Item Change", Coverage::Handled),
    Row::new(
        0x0A,
        "Animation",
        Coverage::Encoded("arm swing with the hold-to-break rework (P2)"),
    ),
    Row::new(0x0B, "Entity Action", Coverage::Handled),
    Row::new(0x0C, "Steer Vehicle", Coverage::Handled),
    Row::new(0x0D, "Close Window", Coverage::Handled),
    Row::new(
        0x0E,
        "Click Window",
        Coverage::Encoded("inventory clicks (P4)"),
    ),
    Row::new(0x0F, "Confirm Transaction", Coverage::Handled),
    Row::new(
        0x10,
        "Creative Inventory Action",
        Coverage::Encoded("creative inventory (P4)"),
    ),
    Row::new(
        0x11,
        "Enchant Item",
        Coverage::Encoded("enchanting table (P4)"),
    ),
    Row::new(
        0x12,
        "Update Sign",
        Coverage::Encoded("sign editor screen (P10)"),
    ),
    Row::new(
        0x13,
        "Player Abilities",
        Coverage::Encoded("creative flight toggle (P2)"),
    ),
    Row::new(
        0x14,
        "Tab Complete",
        Coverage::Encoded("chat completion (P9)"),
    ),
    Row::new(0x15, "Client Settings", Coverage::Handled),
    Row::new(
        0x16,
        "Client Status",
        Coverage::Encoded("death screen respawn, statistics (P2, P10)"),
    ),
    Row::new(0x17, "Plugin Message", Coverage::Handled),
    Row::new(
        0x18,
        "Spectate",
        Coverage::Encoded("spectator teleport (P2)"),
    ),
    Row::new(0x19, "Resource Pack Status", Coverage::Handled),
];

/// The ids `PlayServerbound` can encode (its variants' `ID` constants).
const ENCODABLE: [u8; 26] = [
    serverbound::KeepAlive::ID,
    serverbound::Chat::ID,
    serverbound::UseEntity::ID,
    serverbound::PlayerPosition::ID,
    serverbound::PlayerLook::ID,
    serverbound::PositionLook::ID,
    serverbound::PlayerDigging::ID,
    serverbound::PlayerBlockPlacement::ID,
    serverbound::HeldItemChange::ID,
    serverbound::Animation::ID,
    serverbound::EntityAction::ID,
    serverbound::Input::ID,
    serverbound::CloseWindow::ID,
    serverbound::ClickWindow::ID,
    serverbound::ConfirmTransaction::ID,
    serverbound::CreativeInventoryAction::ID,
    serverbound::ClientSettings::ID,
    serverbound::Brand::ID,
    serverbound::Player::ID,
    serverbound::EnchantItem::ID,
    serverbound::UpdateSign::ID,
    serverbound::PlayerAbilities::ID,
    serverbound::TabComplete::ID,
    serverbound::ClientStatus::ID,
    serverbound::Spectate::ID,
    serverbound::ResourcePackStatus::ID,
];

fn assert_contiguous(table: &[Row], direction: &str) {
    for (index, row) in table.iter().enumerate() {
        assert_eq!(
            usize::from(row.id),
            index,
            "{direction} row {} is out of order",
            row.name
        );
    }
}

/// Whether a reaction does nothing at all.
fn is_empty(reaction: &Reaction) -> bool {
    *reaction == Reaction::default()
}

#[test]
fn tables_list_every_id_once_in_order() {
    assert_contiguous(&CLIENTBOUND, "clientbound");
    assert_contiguous(&SERVERBOUND, "serverbound");
}

#[test]
fn session_consumes_exactly_the_clientbound_ids_the_table_says() {
    for row in CLIENTBOUND {
        let mut session = PlaySession::new(PlayOptions::default());
        // An empty body makes any real decoder fail; ids the session skips
        // (including the body-less `Entity` base packet) return `Ok`.
        let outcome = session.handle(row.id, &[]);
        if row.coverage.session_decodes() {
            assert!(
                outcome.is_err(),
                "0x{:02X} {} is listed as decoded but the session skipped it",
                row.id,
                row.name
            );
        } else {
            let reaction = outcome.unwrap_or_else(|err| {
                panic!(
                    "0x{:02X} {} is listed as skipped but failed: {err}",
                    row.id, row.name
                )
            });
            assert!(
                is_empty(&reaction),
                "0x{:02X} {} is listed as skipped but reacted",
                row.id,
                row.name
            );
        }
    }
}

#[test]
fn handled_serverbound_ids_are_encodable() {
    for row in SERVERBOUND {
        if row.coverage == Coverage::Handled {
            assert!(
                ENCODABLE.contains(&row.id),
                "0x{:02X} {} is Handled but PlayServerbound cannot encode it",
                row.id,
                row.name
            );
        }
    }
}

#[test]
fn every_protocol_47_id_is_handled_or_deferred_to_a_milestone() {
    let problems: Vec<String> = [(false, &CLIENTBOUND[..]), (true, &SERVERBOUND[..])]
        .into_iter()
        .flat_map(|(serverbound, table)| {
            table.iter().filter_map(move |row| {
                row.coverage.problem(serverbound).map(|why| {
                    let direction = if serverbound {
                        "serverbound"
                    } else {
                        "clientbound"
                    };
                    format!("0x{:02X} {} ({direction}): {why}", row.id, row.name)
                })
            })
        })
        .collect();
    assert!(
        problems.is_empty(),
        "{} invalid coverage rows:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
