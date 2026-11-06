# ADR-0053 — P12 takes a launcher session; Microsoft login moves to F1

Status: accepted (2026-11-05)

## Context

P12 bundled the Microsoft login (device code, Xbox Live, XSTS, Minecraft
services, keychain) with the online-mode protocol work. The vanilla 1.8.8
client has no Microsoft login: a launcher passes `--username`, `--uuid` and
`--accessToken`. F1 builds `umicraft-auth` and the multi-account manager, so a
single-account login written in P12 would be reworked there. The protocol work
(encryption, session join, proxies, resource packs) only needs a name, a UUID
and a token, wherever they come from.

## Decision

- P12 keeps NET-4/5 (encryption handshake, session join), NET-10/11 (proxy
  switching, server resource packs) and plugin channels.
- The client gets a `Session { username, uuid, access_token }` seam, filled
  from launch arguments or environment variables, as vanilla. No session
  means an offline session, as today. The token is never logged.
- ACC-4/6/7 (Microsoft flow, token refresh, keychain) and the authentication
  spike move to F1, which plugs the account manager into the same seam.

## Consequences

- P12 no longer carries external risk (Azure registration, Minecraft API
  access); P13 parity is judged on vanilla behaviour only.
- Testing online mode needs a valid token from an existing launcher.
- F1 grows by the Microsoft flow; the spike may still start at any time.
