# ADR-0020 — P1: protocol completeness and vanilla traffic

**Status:** accepted
**Date:** 2026-10-26
**Milestone:** P1 (ROADMAP.md)

## Context

P1 closes every protocol-47 gap listed in `docs/roadmap-review.md` §3.1 and
makes the serverbound traffic of the movement script match vanilla. The work
is wide (34 new clientbound packets, 8 serverbound packets, a session state,
fuzzing and property tests) and is split between several contributors, so
spec §14 rule 11 asks for the interface to be frozen first.

## Decision

### 1. One file and one struct per packet (ADR-0003)

New clientbound packets (`umicraft_protocol::packets::play::clientbound`):

| Id | Module | Type |
|---|---|---|
| 0x0A | `use_bed` | `UseBed` |
| 0x0D | `collect_item` | `CollectItem` |
| 0x10 | `spawn_painting` | `SpawnPainting` |
| 0x11 | `spawn_xp_orb` | `SpawnXpOrb` |
| 0x1D | `entity_effect` | `EntityEffect` |
| 0x1E | `remove_entity_effect` | `RemoveEntityEffect` |
| 0x20 | `entity_properties` | `EntityProperties` |
| 0x27 | `explosion` | `Explosion` |
| 0x28 | `effect` | `Effect` |
| 0x29 | `sound_effect` | `SoundEffect` |
| 0x2A | `particle` | `Particle` |
| 0x2C | `spawn_global_entity` | `SpawnGlobalEntity` |
| 0x33 | `update_sign` | `UpdateSign` |
| 0x34 | `maps` | `Maps` |
| 0x35 | `update_block_entity` | `UpdateBlockEntity` |
| 0x36 | `sign_editor_open` | `SignEditorOpen` |
| 0x37 | `statistics` | `Statistics` |
| 0x38 | `player_list_item` | `PlayerListItem` |
| 0x39 | `player_abilities` | `PlayerAbilities` |
| 0x3A | `tab_complete` | `TabComplete` |
| 0x3B | `scoreboard_objective` | `ScoreboardObjective` |
| 0x3C | `update_score` | `UpdateScore` |
| 0x3D | `display_scoreboard` | `DisplayScoreboard` |
| 0x3E | `teams` | `Teams` |
| 0x3F | `plugin_message` | `PluginMessage` |
| 0x41 | `server_difficulty` | `ServerDifficulty` |
| 0x42 | `combat_event` | `CombatEvent` |
| 0x43 | `camera` | `Camera` |
| 0x44 | `world_border` | `WorldBorder` |
| 0x45 | `title` | `Title` |
| 0x46 | `set_compression` | `SetCompression` |
| 0x47 | `player_list_header_footer` | `PlayerListHeaderFooter` |
| 0x48 | `resource_pack_send` | `ResourcePackSend` |
| 0x49 | `update_entity_nbt` | `UpdateEntityNbt` |

New serverbound packets (`…::serverbound`): 0x03 `player::Player`,
0x11 `enchant_item::EnchantItem`, 0x12 `update_sign::UpdateSign`,
0x13 `player_abilities::PlayerAbilities`, 0x14 `tab_complete::TabComplete`,
0x16 `client_status::ClientStatus`, 0x18 `spectate::Spectate`,
0x19 `resource_pack_status::ResourcePackStatus`.

Packets whose body depends on an action byte (`Player List Item`, `Teams`,
`Combat Event`, `World Border`, `Title`, `Scoreboard Objective`,
`Update Score`) are Rust enums of their actions, so an invalid combination
cannot be built. NBT compounds stay raw bytes (`packets/play/nbt.rs`): no
consumer interprets them yet.

### 2. Session

- Every new clientbound packet becomes a `PlayEvent` variant of the same
  name; the session no longer skips any id silently. An unknown id is still
  skipped (it cannot happen on a protocol-47 server) and logged.
- `Set Compression` (0x46) is not forwarded: the reaction carries the new
  threshold and `drive` applies it to the connection after the replies.
- `Resource Pack Send` (0x48) is answered with `Resource Pack Status`
  *declined* — what vanilla does when the server-pack option of the server
  entry is *disabled* (MCP `NetHandlerPlayClient.handleResourcePack`). P12
  replaces it with the prompt. The event is still forwarded.
- 0x24 Block Action, 0x25 Block Break Animation and 0x2B Change Game State
  are forwarded.

### 3. Game-thread state (`umicraft_game::server`)

`ServerState` keeps the state carried by the new packets even though its
consumers (HUD, tab list, scoreboard, effects in movement…) arrive in later
milestones: abilities, effects and attributes of the local player, game
state (rain, thunder, gamemode), difficulty, world border, player list,
scoreboard and teams, title, block entities and signs, maps, statistics,
camera, server brand and the riding vehicle. It is a plain value updated by
`apply(&PlayEvent-like inputs)` and unit-tested without a server.

### 4. Vanilla traffic (MCP `EntityPlayerSP.onUpdate`, `onUpdateWalkingPlayer`)

Per tick, after the actions of `Minecraft.runTick` (dig, place, chat):

1. on foot: `Entity Action` start/stop sprinting when the sprint state
   differs from the last one sent, then start/stop sneaking likewise, then
   exactly one of 0x06 / 0x04 / 0x05 / **0x03**;
2. riding: 0x05 `Player Look` then 0x0C `Steer Vehicle`, nothing else.

`Steer Vehicle` is never sent on foot.

### 5. Coverage states

`packet_coverage.rs` gains `Forwarded(owner)` (clientbound: decoded and kept,
the consumer arrives in `owner`) and `Encoded(owner)` (serverbound: the client
can encode it; the feature that sends it belongs to `owner`). Neither is a
gap: the audit loses its `#[ignore]` and passes.

### 6. Fuzzing and property tests

`fuzz/` is a `cargo fuzz` crate outside the workspace (nightly only) with
targets for the play decoders and the chunk parser. Every packet gets a
`proptest` round trip in `umicraft-protocol/tests/roundtrip.rs`.

### 7. File ownership during the split

| Owner | Files |
|---|---|
| clientbound A | the 0x0A–0x36 and 0x49 packet files above |
| clientbound B | the 0x37–0x48 packet files above |
| serverbound | `serverbound/*` (new files and `mod.rs`), the `PlayServerbound` enum |
| integration | `clientbound/mod.rs`, `PlayClientbound`, `umicraft-net`, `umicraft-client`, `packet_coverage.rs` |
| game state | `umicraft-game/src/server/` |
| fuzz | `fuzz/` |

## Consequences

- Every protocol-47 id is decoded, so a malformed body of any id now ends
  the session (NET-13) instead of being skipped; the fuzzers guard that the
  decoders fail with errors, never panics.
- `PlayEvent` grows; large variants are boxed to keep the game inbox small.
