# Packets

Raw play packets on the network thread (API 0.4, F3.5, ADR-0086). Everything on this
page needs `network_packets = true` in the `[permissions]` table of `mod.toml`:
without it `subscribe` to a packet event and `packet_send` return `DENIED`.

## Events

`PACKET_RECEIVED` (clientbound) and `PACKET_SEND` (serverbound) carry the play-state
packet id (protocol 47) and its body: no length prefix, never compressed. Both run on
the network thread, so an intercept handler delays the socket; keep it under the
1 ms budget. An observer gets a copy on the mod's worker; the copy of a big packet
(chunk data) costs a heap allocation, and the queue of 256 drops when the mod falls
behind.

Cancelling `PACKET_RECEIVED` hides the packet from the client (no reply, no event),
except `Set Compression` (0x46) and `Disconnect` (0x40): dropping them would break the
framing or hide the kick reason, so the client keeps them whatever the mods say.
Cancelling `PACKET_SEND` writes nothing. Cancelling a keepalive answer gets the player
kicked: the client does not second-guess a mod with the permission.

Neither event is mutable. Most clientbound packets are only decoded when the client
handles them, and a body it cannot decode ends the session, so a replaced body could
not be checked first. To change a packet the client sends, cancel it and inject the
replacement.

## Injection

`packet_send(id, body)` (Rust: `cx.packets().send(id, &body)`) queues a serverbound
packet from any thread once `init` returned. The body must decode as exactly one packet
of that id with the client's own decoder (`PlayServerbound::decode`), with no byte
left over; the client writes the decoded packet, never the mod's bytes. So a mod can
send what the client itself knows how to send, and nothing malformed.

| Status | Why |
|---|---|
| `OK` | Queued; written on the next turn of the network loop (about one tick) |
| `DENIED` | No `network_packets` permission, before `init` returned, or the mod is off (switched off, failed, or held by the server-compatible mode) |
| `INVALID_ARGUMENT` | An id above 0xff, a body over 32 KiB, a body the decoder refuses or does not consume entirely |
| `BUSY` | The shared queue of 64 packets is full: try again later |

Injected packets go through `PACKET_SEND` like the client's own, are counted in the
mod's stats, and a queue left from a session is cleared when the next one starts, so
an injection never reaches another server. The 1.8 servers count a player's packets
(PandaSpigot kicks above 500 in seven seconds): a mod that injects in a loop gets the
player kicked.

An injected `Animation` + `Use Entity` pair skips what the client does around a click
(the miss delay, the item in use, the local swing and break, the packet order of a
tick). To click as the player does, press the binding instead with the `input`
permission (`input_press`, API 1.5, see `input.md`).

## Server-compatible mode

The switch at the top of the External category of the mods menu (saved as
`multiplayer.mod_server_compat` in `options.toml`) keeps every mod that declares
`network_packets` or `movement` off, so the client behaves like an unmodified one on
a server with an anti-cheat. Such mods carry an "Alters packets/movement" mark on their
card in every mode. The mode is a switch, not a per-server setting. It does not hold
off a mod that only declares `input` (API 1.5, ADR-0098), which carries its own badge.

See `mods/examples/packet_logger`.
