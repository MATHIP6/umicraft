# ADR-0041 — P10 server list: book and status pinger

Status: accepted (2026-11-04)

## Context

The P10 multiplayer screen needs a persistent list of servers and live status
(MOTD, players, latency, favicon) for each entry, without blocking the render
thread.

## Decisions

- **Book.** `servers::ServerBook` stores `[[servers.entries]]` (`name`,
  `address`) in `servers.toml` in the data directory, saved atomically through
  `ConfigStore`. Loading never fails: a missing or corrupt file is an empty
  book and entries with an invalid address are skipped. An empty name becomes
  "Minecraft Server" (the vanilla default). Reordering swaps with the neighbour
  like the vanilla arrows. Addresses accept `host`, `host:port`, `[v6]:port`
  and bare IPv6; a bad port is an error, not a silent fallback to 25565.
- **Pinger.** One thread with a current-thread Tokio runtime (as `network.rs`)
  pings every entry concurrently using `umicraft_net::ping::ping`. A refresh
  aborts the pings in flight and increments a generation; `Pinger::poll`
  (non-blocking, for the UI) drops results of older generations.
- **Timeout.** 5 s per step (connect, response, pong), not vanilla's 30 s, so a
  dead server stops spinning quickly; worst case 15 s per entry.
- **Untrusted data.** MOTD 512 chars, version 64, at most 16 sample lines of
  64 chars, counts clamped to `u32`, favicon must be a PNG of at most 64 KiB
  (a bad favicon is dropped, the server stays online). A garbage reply is
  `Unreachable`.
- **Protocol fix.** `Status Response` was decoded with the 256-char string cap,
  which rejected any real server with a favicon. It now accepts 131 072 chars.
- **Not done.** SRV record lookup, per-server resource-pack mode, LAN discovery.
