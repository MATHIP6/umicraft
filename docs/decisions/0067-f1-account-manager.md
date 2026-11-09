# ADR-0067 — F1 account manager architecture

Status: accepted (2026-11-09)

## Context

F1 adds a multi-account manager (offline and Microsoft accounts), reachable from
the server list, plugged into the P12 `Session` seam (ADR-0053). The owner has no
Azure application yet.

## Decisions

- **Layers.** `umicraft-auth` holds all logic (accounts, offline UUID, store,
  vault, Microsoft flow, manager) and no UI. `umicraft_ui::menus::accounts` is a
  pure immediate-mode screen (ADR-0042 style) fed with `AccountRow` view models.
  `umicraft-client` wires them (`menu_flow/accounts.rs`, background login task).
  No file above ~300 lines; one Microsoft step per file.
- **Storage.** `accounts.toml` in the config folder holds metadata only (id, kind,
  name, uuid, selected). Tokens live behind a `SecretVault` trait: system keychain
  (`keyring`, MIT OR Apache-2.0) with an in-memory fallback when no keychain
  exists (then Microsoft accounts need a new login each run).
- **Offline UUID.** Standard `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`
  (MD5, version 3), as vanilla servers compute it.
- **HTTP seam.** Every Microsoft step goes through an `HttpTransport` trait so the
  flow is tested against canned responses.
- **Live switch.** The selected account is read at every connection; the launcher
  flags (`--username`...) still win when given.
- **Azure client id.** Until the owner registers an app, the Microsoft flow uses
  the Prism Launcher client id (`c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb`, scope
  `XboxLive.SignIn XboxLive.offline_access`), by owner decision of 2026-11-09. It is
  a single constant, overridable by `UMICRAFT_MSA_CLIENT_ID`.

## Consequences and risks

- Prism's CMake file says its key is for builds that accept Microsoft's terms of
  use, and to change it when rebranding; it grants no explicit permission to other
  apps. Microsoft may revoke it, and Umicraft's use is therefore **not a release
  option**: a own Azure app (with Minecraft API approval, https://aka.ms/mce-reviewappid)
  must replace it before any public release. Tracked in ROADMAP F1.
- Sources: see `docs/sources.md` (F1 rows).
