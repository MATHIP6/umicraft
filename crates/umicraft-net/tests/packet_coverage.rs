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
//! - `Handled`: clientbound, the session decodes it and forwards an event or
//!   a reply; serverbound, the client encodes it and sends it when vanilla
//!   would.
//! - `Ignored`: deliberately not acted on; the reason says why it is safe.
//! - `Dropped`: clientbound only; the session decodes it but the data is
//!   discarded (by the session or by the game thread). This is a gap.
//! - `Missing`: not decoded, not encoded, or never sent. This is a gap.
//!
//! The tests that run in CI keep the tables honest (complete, ordered, and in
//! agreement with the session and with `PlayServerbound`). The audit itself,
//! [`every_protocol_47_id_is_handled_or_intentionally_ignored`], fails on the
//! known gaps, so it is `#[ignore]`d. Run it with:
//!
//! ```text
//! cargo test -p umicraft-net --test packet_coverage -- --ignored
//! ```
//!
//! P1 (ROADMAP.md) must close the gaps, make the audit pass and then remove
//! the `#[ignore]`.

use umicraft_net::play::{PlayOptions, PlaySession, Reaction};
use umicraft_protocol::packets::play::serverbound;

/// The state of one packet id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coverage {
    /// Fully supported.
    Handled,
    /// Intentionally not acted on; the reason says why that is safe.
    Ignored(&'static str),
    /// Decoded but its data is discarded; the text says where and what is lost.
    Dropped(&'static str),
    /// Not supported; the text says what is missing and which milestone owns it.
    Missing(&'static str),
}

impl Coverage {
    /// The explanation of a gap, or `None` when the id needs no work.
    fn gap(self) -> Option<&'static str> {
        match self {
            Self::Handled | Self::Ignored(_) => None,
            Self::Dropped(why) | Self::Missing(why) => Some(why),
        }
    }

    /// Whether the clientbound session runs the packet's decoder.
    fn session_decodes(self) -> bool {
        matches!(self, Self::Handled | Self::Dropped(_))
    }
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
    Row::new(0x0A, "Use Bed", Coverage::Missing("not decoded; sleeping pose needs it (P1)")),
    Row::new(0x0B, "Animation", Coverage::Dropped("forwarded as PlayEvent::Animation but the game thread ignores it (game.rs, P1/M8)")),
    Row::new(0x0C, "Spawn Player", Coverage::Handled),
    Row::new(0x0D, "Collect Item", Coverage::Missing("not decoded; pickup animation (P1)")),
    Row::new(0x0E, "Spawn Object", Coverage::Handled),
    Row::new(0x0F, "Spawn Mob", Coverage::Handled),
    Row::new(0x10, "Spawn Painting", Coverage::Missing("not decoded; paintings are never spawned (P1)")),
    Row::new(0x11, "Spawn Experience Orb", Coverage::Missing("not decoded; XP orbs are never spawned (P1)")),
    Row::new(0x12, "Entity Velocity", Coverage::Dropped("applied to other entities only; the local player never receives knockback (P1)")),
    Row::new(0x13, "Destroy Entities", Coverage::Handled),
    Row::new(0x14, "Entity", Coverage::Ignored("the base S14PacketEntity carries only an entity id and moves nothing; vanilla's handler adds a zero delta")),
    Row::new(0x15, "Entity Relative Move", Coverage::Handled),
    Row::new(0x16, "Entity Look", Coverage::Handled),
    Row::new(0x17, "Entity Look And Move", Coverage::Handled),
    Row::new(0x18, "Entity Teleport", Coverage::Handled),
    Row::new(0x19, "Entity Head Look", Coverage::Handled),
    Row::new(0x1A, "Entity Status", Coverage::Dropped("forwarded but the game thread ignores it: no hurt/death/eat statuses (P1/M8)")),
    Row::new(0x1B, "Attach Entity", Coverage::Dropped("forwarded but the game thread ignores it: no riding or leashes (P1/M8)")),
    Row::new(0x1C, "Entity Metadata", Coverage::Handled),
    Row::new(0x1D, "Entity Effect", Coverage::Missing("not decoded; potion effects (speed, jump, blindness...) (P1)")),
    Row::new(0x1E, "Remove Entity Effect", Coverage::Missing("not decoded; potion effects (P1)")),
    Row::new(0x1F, "Set Experience", Coverage::Handled),
    Row::new(0x20, "Entity Properties", Coverage::Missing("not decoded; movement speed attribute for sprint/slowness (P1)")),
    Row::new(0x21, "Chunk Data", Coverage::Handled),
    Row::new(0x22, "Multi Block Change", Coverage::Handled),
    Row::new(0x23, "Block Change", Coverage::Handled),
    Row::new(0x24, "Block Action", Coverage::Missing("decoder exists in umicraft-protocol but PlaySession::react skips the id; chest lids, note blocks, pistons (P1)")),
    Row::new(0x25, "Block Break Animation", Coverage::Missing("decoder exists in umicraft-protocol but PlaySession::react skips the id; other players' crack overlay (P1)")),
    Row::new(0x26, "Map Chunk Bulk", Coverage::Handled),
    Row::new(0x27, "Explosion", Coverage::Missing("not decoded; block removal and explosion knockback (P1)")),
    Row::new(0x28, "Effect", Coverage::Missing("not decoded; block-break/door/record effects (P1)")),
    Row::new(0x29, "Sound Effect", Coverage::Missing("not decoded; nearly all 1.8 sounds, needs the audio milestone (M11)")),
    Row::new(0x2A, "Particles", Coverage::Missing("not decoded; server particles (P1)")),
    Row::new(0x2B, "Change Game State", Coverage::Missing("decoder exists in umicraft-protocol but PlaySession::react skips the id; rain, gamemode change, credits (P1)")),
    Row::new(0x2C, "Spawn Global Entity", Coverage::Missing("not decoded; lightning (P1)")),
    Row::new(0x2D, "Open Window", Coverage::Handled),
    Row::new(0x2E, "Close Window", Coverage::Handled),
    Row::new(0x2F, "Set Slot", Coverage::Handled),
    Row::new(0x30, "Window Items", Coverage::Handled),
    Row::new(0x31, "Window Property", Coverage::Handled),
    Row::new(0x32, "Confirm Transaction", Coverage::Handled),
    Row::new(0x33, "Update Sign", Coverage::Missing("not decoded; sign text (P1)")),
    Row::new(0x34, "Maps", Coverage::Missing("not decoded; maps in hand and item frames (P1)")),
    Row::new(0x35, "Update Tile Entity", Coverage::Missing("not decoded; skulls, banners, spawners, flower pots (P1)")),
    Row::new(0x36, "Open Sign Editor", Coverage::Missing("not decoded; sign editing (P1)")),
    Row::new(0x37, "Statistics", Coverage::Missing("not decoded; statistics screen (P1)")),
    Row::new(0x38, "Player List Item", Coverage::Missing("not decoded; tab list, skins, others' gamemode and ping (P1/M10)")),
    Row::new(0x39, "Player Abilities", Coverage::Missing("not decoded; flying, fly speed, invulnerability (P1)")),
    Row::new(0x3A, "Tab Complete", Coverage::Missing("not decoded; chat completion (M10)")),
    Row::new(0x3B, "Scoreboard Objective", Coverage::Missing("not decoded; scoreboard (M10)")),
    Row::new(0x3C, "Update Score", Coverage::Missing("not decoded; scoreboard (M10)")),
    Row::new(0x3D, "Display Scoreboard", Coverage::Missing("not decoded; scoreboard (M10)")),
    Row::new(0x3E, "Teams", Coverage::Missing("not decoded; team prefixes, colours, name-tag visibility (M10)")),
    Row::new(0x3F, "Custom Payload", Coverage::Missing("not decoded; MC")),
    Row::new(0x40, "Disconnect", Coverage::Handled),
    Row::new(0x41, "Server Difficulty", Coverage::Missing("not decoded; options screen (P1)")),
    Row::new(0x42, "Combat Event", Coverage::Missing("not decoded; death message and death screen (P1)")),
    Row::new(0x43, "Camera", Coverage::Missing("not decoded; spectator camera (P1)")),
    Row::new(0x44, "World Border", Coverage::Missing("not decoded; border rendering (P1)")),
    Row::new(0x45, "Title", Coverage::Missing("not decoded; titles, subtitles, action bar (M10)")),
    Row::new(0x46, "Set Compression", Coverage::Missing("not decoded; compression change in play state (P1)")),
    Row::new(0x47, "Player List Header And Footer", Coverage::Missing("not decoded; tab list header and footer (M10)")),
    Row::new(0x48, "Resource Pack Send", Coverage::Missing("not decoded; server resource packs, and no status reply is sent (P1)")),
    Row::new(0x49, "Update Entity NBT", Coverage::Missing("not decoded; rare (P1)")),
];

/// Serverbound play packets, ids `0x00..=0x19`.
const SERVERBOUND: [Row; 0x1A] = [
    Row::new(0x00, "Keep Alive", Coverage::Handled),
    Row::new(0x01, "Chat Message", Coverage::Handled),
    Row::new(
        0x02,
        "Use Entity",
        Coverage::Missing("encoded but never built by the client: no attack, no interact (P1)"),
    ),
    Row::new(
        0x03,
        "Player",
        Coverage::Missing(
            "not encoded; vanilla sends the on-ground-only packet every tick without movement (P1)",
        ),
    ),
    Row::new(0x04, "Player Position", Coverage::Handled),
    Row::new(0x05, "Player Look", Coverage::Handled),
    Row::new(0x06, "Player Position And Look", Coverage::Handled),
    Row::new(0x07, "Player Digging", Coverage::Handled),
    Row::new(0x08, "Player Block Placement", Coverage::Handled),
    Row::new(0x09, "Held Item Change", Coverage::Handled),
    Row::new(
        0x0A,
        "Animation",
        Coverage::Missing(
            "encoded but never built by the client: the arm swing is never announced (P1)",
        ),
    ),
    Row::new(
        0x0B,
        "Entity Action",
        Coverage::Missing(
            "encoded but never built by the client: sprint and sneak are never announced (P1)",
        ),
    ),
    Row::new(
        0x0C,
        "Steer Vehicle",
        Coverage::Missing("sent every tick by the client; vanilla sends it only while riding (P1)"),
    ),
    Row::new(0x0D, "Close Window", Coverage::Handled),
    Row::new(
        0x0E,
        "Click Window",
        Coverage::Missing(
            "encoded and ClientCommand::ClickWindow exists, but no code builds it (M9)",
        ),
    ),
    Row::new(0x0F, "Confirm Transaction", Coverage::Handled),
    Row::new(
        0x10,
        "Creative Inventory Action",
        Coverage::Missing("encoded but never built by the client (P1)"),
    ),
    Row::new(
        0x11,
        "Enchant Item",
        Coverage::Missing("not encoded; enchanting table (P1)"),
    ),
    Row::new(
        0x12,
        "Update Sign",
        Coverage::Missing("not encoded; sign writing (P1)"),
    ),
    Row::new(
        0x13,
        "Player Abilities",
        Coverage::Missing("not encoded; toggling flight (P1)"),
    ),
    Row::new(
        0x14,
        "Tab Complete",
        Coverage::Missing("not encoded; chat completion (M10)"),
    ),
    Row::new(0x15, "Client Settings", Coverage::Handled),
    Row::new(
        0x16,
        "Client Status",
        Coverage::Missing("not encoded; respawn after death, statistics (P1)"),
    ),
    Row::new(0x17, "Plugin Message", Coverage::Handled),
    Row::new(
        0x18,
        "Spectate",
        Coverage::Missing("not encoded; spectator teleport (P1)"),
    ),
    Row::new(
        0x19,
        "Resource Pack Status",
        Coverage::Missing("not encoded; servers requiring a pack (P1)"),
    ),
];

/// The ids `PlayServerbound` can encode (its variants' `ID` constants).
const ENCODABLE: [u8; 18] = [
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
#[ignore = "known protocol gaps; closed by P1 (ROADMAP.md)"]
fn every_protocol_47_id_is_handled_or_intentionally_ignored() {
    let gaps: Vec<String> = [
        ("clientbound", &CLIENTBOUND[..]),
        ("serverbound", &SERVERBOUND[..]),
    ]
    .into_iter()
    .flat_map(|(direction, table)| {
        table.iter().filter_map(move |row| {
            row.coverage
                .gap()
                .map(|why| format!("0x{:02X} {} ({direction}): {why}", row.id, row.name))
        })
    })
    .collect();
    assert!(
        gaps.is_empty(),
        "{} protocol-47 packet gaps:\n{}",
        gaps.len(),
        gaps.join("\n")
    );
}
