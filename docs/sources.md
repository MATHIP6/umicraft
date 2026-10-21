# Sources & provenance

Every non-obvious behavior fact must cite where it came from (AGENTS.md, spec §14.6).
Per ADR-0001, references may include the local MCP tree; record the consulted path.

Entry format:

| Date | Area | Source | Notes |
|---|---|---|---|

Source class tags:

- `mcp:<path under /mcp>` — deobfuscated reference code (ADR-0001).
- `docs:<url>` — community protocol/behavior documentation (archived wiki.vg, etc.).
- `capture:<name>.pcap` — packet captures from `umicraft-testkit` (from M1 on).

No entries yet — first entries expected with M1 (protocol 47 packet work).

---

## Entries

| Date | Area | Source | Notes |
|---|---|---|---|
| 2026-10-21 | protocol 47 / VarInt / types | `docs: wiki.vg Protocol (pre-1.13 archive)` | VarInt is 7-bit groups, LSB-first, MSB continuation; ≤5 bytes for i32. Strings are VarInt-length-prefixed UTF-8, ≤256 chars standard cap (4 bytes/char on the wire). No MCP consult needed. |
| 2026-10-21 | protocol 47 / framing | `docs: wiki.vg Protocol / Compression (pre-1.13 archive)` | Post-Set-Compression format `[packet len][uncompressed len][data]`; uncompressed-length varint 0 = raw passthrough; packet len counts the varint + data; vanilla default threshold 256. |
| 2026-10-21 | protocol 47 / packet ids | `docs: wiki.vg Protocol (pre-1.13 archive)` | Handshake sb 0x00 (version, addr, port, next-state 1|2); Status sb 0x00/0x01, cb 0x00/0x01; Login sb 0x00/0x01, cb 0x00 Disconnect / 0x01 Encryption Request / 0x02 Login Success / 0x03 Set Compression; Play subset ids as implemented in `packets.rs`. |
| 2026-10-21 | protocol 47 / keepalive | `docs: wiki.vg Protocol (pre-1.13 archive)` + `mcp: src/minecraft/net/minecraft/network/play/{server/S00,client/C00}PacketKeepAlive.java` | ~~1.8 Keep Alive uses i64 ids~~ **Corrected (2026-10-21):** 1.8 ids are VarInt ints both directions (`read/writeVarIntFromBuffer` in both MCP classes); the i64 id is a 1.12.2+ change. wiki.vg row was wrong for 47. Confirmed live: PandaSpigot 1.8.8 sends VarInt bodies (e.g. 4 bytes); our earlier i64 decode killed the session ~3 s after join. |
| 2026-10-21 | encryption / CFB8 stream | `docs: NIST SP 800-38A (CFB mode)` + RustCrypto `cfb8` 0.8 doc vector | Our CFB8 loop pinned to crate's published test vector (key 0x42×16, iv 0x24×16) in `encryption.rs` tests. RustCrypto `cipher` 0.4.4 APIs consume the cipher object, so the stateful stream is implemented in-repo. |
| 2026-10-21 | encryption / shared secret | `docs: wiki.vg Encryption (pre-1.13 archive)` | Key **and** IV both set to the 16-byte shared secret; secret + verify token travel encrypted with the server's X.509 SPKI RSA-1024 public key, PKCS#1 v1.5. |
| 2026-10-21 | encryption / server hash | `docs: wiki.vg Protocol Details / Server ID hash (pre-1.13 archive)` | `sha1(shared_secret \|\| server_nonce)` read as signed big-endian big-int; hex string without leading zeros, `-`-prefixed when negative. |
| 2026-10-21 | play / Join Game 0x01 | `mcp: src/minecraft/net/minecraft/network/play/server/S01PacketJoinGame.java` | Wire layout: Int entity id (plain BE i32, **not** VarInt), u8 gamemode (hardcore bit 8 folded in), i8 dimension, u8 difficulty, u8 max players, string (cap 16) level type, trailing bool reducedDebugInfo. Live cross-check: server log "entity id 1723" == our decoded `entity_id`; `level_type=flat`, `max_players=5` == server.properties. |
| 2026-10-21 | play / Client Settings 0x15 | `mcp: src/minecraft/net/minecraft/network/play/client/C15PacketClientSettings.java` | Layout: locale string (server read cap **7**), i8 view distance, byte chat visibility (not a VarInt), bool chat colors, u8 skin-part mask. |
| 2026-10-21 | play / Plugin Message 0x17 | `mcp: src/minecraft/net/minecraft/network/play/client/C17PacketCustomPayload.java` | Channel string (cap 20) then raw remainder bytes — 1.8 adds **no** inner length prefix at the packet layer; `MC|Brand`'s payload itself is a VarInt-length-prefixed string. |
| 2026-10-21 | play / Position & Look | `mcp: src/minecraft/net/minecraft/network/play/server/S08PacketPlayerPosLook.java` | x/y/z f64 + yaw/pitch f32 + u8 relative-flags, exactly as implemented; teleport reply echoes accepted coords. Live: initial placement (`-1701.5, 4.0, 106.5`) reproduced server log coords. |
| 2026-10-21 | 1.8 fixes vs MCP | implementation commit | Protocols above were wrong in three M1 spots until the MCP consult: Keep Alive ids (VarInt, not i64), Join Game entity id (fixed Int, not VarInt) + forgotten trailing reducedDebugInfo, Client Settings locale cap (7) + chat visibility (byte, not VarInt), Plugin Message payload (no inner length at the packet layer). All verified against a live PandaSpigot 1.8.8 flat-world session; see later capture rows for details. |
