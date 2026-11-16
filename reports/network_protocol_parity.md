# Network protocol parity: Umicraft against Java 1.8.9

Status: **initial read-only audit.** No code changed, nothing run against a server. Date: 2026-11-11.
Files: `umicraft-game/src/traffic.rs`, `umicraft-client/src/game.rs`, `network/command.rs`, `network/thread.rs`,
`umicraft-net/src/play/drive.rs`, `umicraft-protocol/src/packets/play/serverbound/*`.
Reference: `EntityPlayerSP.java`, `Minecraft.java`, `NetHandlerPlayClient.java`, `PlayerControllerMP.java`.

## What matches (confirmed)

- Per-tick output of a player on foot: sprint action, sneak action, one of C06/C04/C05/C03
  (`traffic.rs:211-262` against `EntityPlayerSP.java:189-274`). Riding: C05 then C0C. No tick without one
  movement packet, and nothing is sent while the player's chunk is not loaded (`game.rs:3665-3672`, same as
  `isBlockLoaded` at `EntityPlayerSP.java:170`).
- Attack and use are consumed inside `tick()` before the physics step and before the movement packet
  (`game.rs:3466-3467`), the order of `runTick` then entity update.
- The acknowledgement of S08 is a C06 with the resulting position and `onGround=false`; transaction replies go
  through the game thread so they stay in order with it (`game.rs:2962-2981`).
- Socket: `TCP_NODELAY`, commands read before the socket (`biased`), batches of up to 16 written in one flush
  (`connection.rs:71`, `drive.rs:113-134`).
- `PlayerLook` wire layout (f32 yaw, f32 pitch, bool) with a byte-level test.

## Confirmed divergences

### N1: movement packets can be overwritten (`game.rs:1195-1240`, `1314-1336`)

`CommandQueue::push` puts every C03/C04/C05/C06 into one `position` slot. `run` executes up to 3 catch-up
ticks (`MAX_CATCH_UP_TICKS`) and calls `queue.flush()` once afterwards. Tick 1's packet is replaced by tick 2's.
`ServerView` already recorded tick 1's position as reported, so the server never learns it. A full channel
keeps only the newest as well (`flush`: `self.position = try_send(..).err()`).
Vanilla sends one movement packet per tick, always. Consequences on a server: fewer flying packets than ticks
(timing-based checks), and a position change that spans two ticks of motion.
The comment above the struct explains the reason: a burst of packets could pass the 500-in-7-seconds limit of
PandaSpigot. A catch-up of at most 3 ticks is not such a burst; vanilla sends up to 10.
**Candidate fix:** movement packets go into the ordered queue; coalesce only `Input`. Test: three ticks, one
flush, assert three movement packets in order.

### N2: slot change, drop and pick-block leave at event time (`game.rs:1296-1302,1354,3090-3112`)

`handle_input` queues `HeldItemChange`, drop and pick-block, and the loop flushes in the same pass, between two
ticks. Vanilla handles the hotbar keys in `runTick` (`Minecraft.java:2052-2060`) and sends C09 from
`syncCurrentPlayItem`, called by `updateController` at the start of the tick (`Minecraft.java:1736`,
`PlayerControllerMP.java:375`); the drop is also in `runTick` (`:2080-2084`). So vanilla sends them in the same
instant as that tick's C03; Umicraft up to 50 ms before it.
This matches the earlier `Post` alert naming a held-item change **if** the Post check measures the gap between
an action and the movement packet. That check was not read, so this stays a *likely cause*.
**Candidate fix:** `handle_input` only records the requested slot (and the drop request); `tick()` emits C09
and C07 at its start, before `run_attack`, like `updateController`. Test: select a slot between two ticks, assert
nothing is sent until the next `tick`, then C09 comes before that tick's movement packet.

### N3: yaw on the wire is wrapped

See `rotation_parity.md` R1.

## Not audited yet

C07 digging sequence, C08 placement offsets (`cursor_*` are `u8` sixteenths, check against the float
`/16` of `PlayerControllerMP.onPlayerRightClick`), C0A swing order relative to C02, C0B ids, C0D close-window,
the click-window transaction numbering, packet IDs of the whole serverbound set, `VarInt` edge cases. Packet
counts per second under catch-up and under a full channel were not measured.

## Observations without a verdict

- No `MC|Brand` send was found by name search in the client crates (the grep only found the receiving side).
  Vanilla sends it in `handleJoinGame`. Check, and if the client declares itself, keep it honest.
- The `Post` and `held item change` alerts were seen once on one server; no packet capture of that session is
  available in the repository.

## Tests

Executed: baseline run (see `vanilla_parity_audit.md`). Not executed: any capture comparison
(`umicraft-trace-diff` needs a vanilla recording), any private server session.
