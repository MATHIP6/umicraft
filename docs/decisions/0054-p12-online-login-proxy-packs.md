# ADR-0054 — P12: online login, proxy switching and server packs

Status: accepted (2026-11-05)

## Context

ADR-0053 limits P12 to a launcher session plus the protocol work: the
encryption handshake, server switching behind BungeeCord or Velocity, and
server resource packs. The code had a typed `OnlineModeRequired` error, the
packet codecs and the AES/CFB8 stream, but no login that used them, and
`network.rs` (800 lines) and `play/session.rs` (1300 lines) had no room to
grow.

## Decision

- **`umicraft-auth`** (new, no game dependency) owns `Session`, `Secret`
  (redacted `Debug`, no `Display`), the launch-argument reader and the
  `SessionServer` trait with its HTTP implementation. F1 extends this crate
  with the account manager.
- **`umicraft-net::login`** is a module directory: `mod.rs` runs the flow
  (`login_offline`, `login_online`), `encrypt.rs` answers the `Encryption
  Request` in the vanilla order: secret, session-server join, `Encryption
  Response`, then encryption. A refused join never reaches the game server.
  `NetError::Auth` carries no token.
- **The server hash** is `SHA1(server_id || secret || public_key)` as a signed
  hex string. The previous helper hashed the secret and the verify token, which
  no session server accepts; it is fixed with the published Notch/jeb_/simon
  vectors as regression tests.
- **Proxy switching (NET-10)** needs no reconnection: BungeeCord and Velocity
  send a new `Join Game` (and `Respawn`) on the same socket. The game thread
  already rebuilt its state on `Join Game`; the missing piece was that the
  play session sent `Client Settings` and `MC|Brand` only for the first one.
  Vanilla sends them from every `handleJoinGame`, so the backend behind the
  proxy learns them. The one-time gate is removed.
  The vanilla client sends no `REGISTER` on its own, so the plugin-channel
  handshake is just `MC|Brand`, which already existed.
- **Server resource packs (NET-11)**: the net session forwards the offer and
  answers `Declined` itself only when the server entry disables packs. The
  download, SHA-1 check and cache live in `umicraft_assets::server_pack`
  (vanilla `ResourcePackRepository` rules: 50 MiB, `<sha1>` or `legacy`
  cache name); the decision (prompt, enabled, disabled), the status replies
  and the reload live in the client window, outside the game thread.
- `network.rs` is split into `network/{mod,command,thread}`; new code goes in
  new modules.

## Consequences

- Online mode is tested against a fake server that really encrypts
  (`umicraft-testkit`), never against Mojang. Joining a real premium server
  needs a launcher token and is an owner check.
- The token appears only in the join request body. Tests assert that neither
  `Debug` output nor error text contains it.
