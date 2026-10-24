# ADR-0010 — One play session, without I/O, behind a cancel-safe loop

- Status: Implemented
- Date: 2026-10-24
- Deciders: project owner
- Supersedes: the network-thread row of ADR-0005 (`PlaySession` ownership)
- Related: ADR-0005 (game thread), spec NET-5/6/9/13

## Context

Two play loops existed. `umicraft-net::client::PlaySession` decoded 9
clientbound ids, answered keepalives and teleports and sent the entry packets;
the headless binary and the `umicraft-testkit` integration tests used it. The
graphical client could not, because it skipped the M3 packets, so
`umicraft-client::network` re-implemented the dispatch for 16 ids, the
keepalive answer, the entry packets and the relative-flag resolution. The
copies had drifted (`en_us` hard-coded on one side, teleport answered on one
side only), and the integration tests verified the loop the client does not
run. M8–M10 will add dozens of packets.

The old `PlaySession::next` also wrote on the socket (the keepalive answer)
inside the future it returned. The client reads the socket inside a
`tokio::select!` that also polls the game thread's commands; cancelling a
future in the middle of a write leaves half a frame on the socket and an
advanced cipher, which corrupts the stream. The client avoided the problem
only because it did not use `PlaySession`.

## Decision

1. **`PlaySession` performs no I/O.** It lives in `umicraft-net::play` and
   exposes `handle(id, &body) -> Result<Reaction, NetError>`. It is
   synchronous, owns no socket and is tested packet by packet.
2. **`Reaction` does not allocate.** It holds an `Option<PlayEvent>`, at most
   two serverbound replies stored inline, and an optional kick reason. Only the
   first `Join Game` needs two replies (the entry packets).
3. **One event vocabulary.** `PlayEvent` covers every packet the game thread
   consumes; chunk payloads are boxed so the bounded inbox stays small. The
   client's `NetEvent` is removed; transport failures are reported by the
   loop, not as events.
4. **Entry packets come from `PlayOptions`** (locale, view distance, chat
   colours, brand), in the reaction to the first `Join Game`.
5. **The placement is not answered by the session.** The session resolves the
   relative flags and emits `PlayEvent::Placement`; the game thread
   acknowledges it after it applied the correction (ADR-0005 §A.5). The old
   automatic teleport answer is removed.
6. **Unknown ids produce an empty reaction** with a trace; a malformed body of
   a handled id is an `Err`, which ends the session cleanly (NET-13).
7. **`drive` is the one async loop.** `drive(connection, session, commands,
   counters, on_event) -> Result<SessionEnd, NetError>` selects between the
   socket and a `tokio::sync::mpsc::Receiver<PlayServerbound>`. **Nothing is
   written to the socket from inside the `select!`**: only
   `Connection::read_packet` (cancel-safe: its only await point is the socket
   read, and bytes are buffered after the read completed) and `recv` are
   raced; replies and commands are written after the select released the
   connection.
8. **The session ends with a value.** `SessionEnd::Kicked(reason)` on a play
   `Disconnect`, `SessionEnd::CommandsClosed` when the caller dropped the
   command sender. Errors are I/O or protocol failures.
9. **`SessionCounters`** (atomics: packets received, packets sent, keepalives
   answered) replace the client's `Arc<AtomicU64>` and `PlaySummary`; the
   caller owns them, the overlay reads them live.
10. **The commands are wire packets.** `drive` takes `PlayServerbound`; the
    client keeps `ClientCommand` and converts it on the game thread when the
    queue accepts it, so the position coalescing is unchanged.

The old async `PlaySession::next`, `Connection::run_play` and `PlaySummary`
are deleted without a compatibility layer.

## Consequences

- One place decodes play packets; the headless binary, the testkit and the
  client run the same loop, so the integration tests cover the shipped code.
- The thread layout of ADR-0005 is unchanged: the network thread still owns
  the runtime and the socket, events still cross the bounded inbox of 256.
- A caller that stops a session early may drop the `drive` future; this is
  safe only because the connection is dropped with it. Code that keeps using
  the connection must close the command channel instead.
- World packets still carry protocol structs in `PlayEvent`; the game thread
  translates them into world updates (ADR-0011).
