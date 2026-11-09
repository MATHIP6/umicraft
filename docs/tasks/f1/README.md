# F1 — Account manager & live switching

Design: ADR-0067 (architecture), ADR-0068 (chooser screen).

## Layout

| Where | What |
|---|---|
| `umicraft-auth::account/offline/store/vault/manager` | Accounts, offline UUID, `accounts.toml`, secret vault, `AccountManager` |
| `umicraft-auth::microsoft/*` | One file per step: device code, refresh, Xbox, XSTS, Minecraft, chain |
| `umicraft-auth::reqwest_transport`, `keyring_vault` | Real HTTP and system keychain (fallback: `MemoryVault`) |
| `umicraft_ui::menus::accounts` | Pure chooser screen (rows, dialogs, device-code dialog) |
| `umicraft-client::accounts` | `AccountsFlow`: manager + screen + background jobs; `Identity` for the connection |

## Behaviour

- Server list, top right: the active account's name; click opens the chooser.
- Play selects the account and goes back to the list; the next join uses it
  (live switch). A Microsoft session is renewed in the background as soon as the
  account is selected; joining before it is ready reopens the chooser with a message.
- Launcher flags (`--username`, `--uuid`, `--accessToken`) still win.
- Offline accounts join without a session under their own name.

## Owner checklist (needs a real Microsoft account)

1. Add account, Microsoft: the code and link show; approving finishes the login.
2. Restart: the account is still listed and its session renews without a prompt.
3. Join an online-mode server as the Microsoft account; join an offline one as an offline account.
4. Delete an account: its refresh token leaves the keychain.
5. Run with `RUST_LOG=trace` and search the output for the token: no match.

## Open

- Heads are a flat placeholder (no skin fetch yet).
- Errors from the auth crate are shown in English.
- No `--hud-view accounts` demo.
- Client id: stopgap, see ADR-0067.
