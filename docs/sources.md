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
| 2026-10-21 | protocol 47 / keepalive | `docs: wiki.vg Protocol (pre-1.13 archive)` | 1.8 Keep Alive uses i64 ids both directions; client must echo the id. |
| 2026-10-21 | encryption / CFB8 stream | `docs: NIST SP 800-38A (CFB mode)` + RustCrypto `cfb8` 0.8 doc vector | Our CFB8 loop pinned to crate's published test vector (key 0x42×16, iv 0x24×16) in `encryption.rs` tests. RustCrypto `cipher` 0.4.4 APIs consume the cipher object, so the stateful stream is implemented in-repo. |
| 2026-10-21 | encryption / shared secret | `docs: wiki.vg Encryption (pre-1.13 archive)` | Key **and** IV both set to the 16-byte shared secret; secret + verify token travel encrypted with the server's X.509 SPKI RSA-1024 public key, PKCS#1 v1.5. |
| 2026-10-21 | encryption / server hash | `docs: wiki.vg Protocol Details / Server ID hash (pre-1.13 archive)` | `sha1(shared_secret \|\| server_nonce)` read as signed big-endian big-int; hex string without leading zeros, `-`-prefixed when negative. |
