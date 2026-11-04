# ADR-0044 — P10 menus and the session lifetime

Status: accepted (2026-11-04)

## Context

Until P10 the windowed client needed `<host> <port>`, started one `Session`
before the window existed and exited when the server kicked the player. The
menu screens (ADR-0042), the server book and pinger (ADR-0041) and the
options pages (ADR-0043) were finished but nothing showed them. P10 asks that
a user can start the client, add a server, connect, disconnect and quit
without the command line.

## Decisions

- **Start.** `<host> <port>` is optional in windowed mode. With both, the
  window connects at start (tests, scripts and packet recording keep working);
  without them it opens on the title screen. A lone host is an error.
  Headless and screenshot sessions still require both.
- **Screens.** `Screen` gains `Title`, `ServerList`, `ServerEdit`,
  `ConfirmDelete`, `Connecting`, `Disconnected` and `Stats`. Escape leads
  back one level (list to title, dialogs and connecting and disconnected to
  the list, statistics to the game menu); the options return to the screen
  they were opened from (`App.options_origin`: title or game menu), so
  `after_escape` takes that origin.
- **`MenuFlow` is pure.** It owns the menu screens, the `ServerBook`, the
  `Pinger` and the row cache, and answers each frame with a `MenuCommand`
  (`Connect`, `OpenOptions`, `Quit`, `GoTo`). Book edits are applied and
  saved inside it; the window never touches the book. Entering the list or
  pressing Refresh pings every entry again; results are polled every frame
  and mapped to `RowStatus` in `menu_rows`. A reorder swaps the cached rows,
  so the pings already received stay on their servers. Without a pinger
  thread the rows stay "pinging".
- **Window split.** `window.rs` became `window/`. `session_control` (start,
  end, lifecycle decision, `App::send_event`), `menu_screens`,
  `options_control`, `chat_control` and `inventory_control` hold the glue
  that used to live in one 2000-line file; the decisions that need no window
  (`lifecycle_step`, `network_config`) are free functions with tests.
- **Session lifetime.** `App.session` is `Option<Session>`; every send goes
  through `App::send_event`, which is a no-op without a session.
  `Connecting` becomes `Playing` when the game thread publishes the first
  placed player: `GameStatus.placed`, set by the first `Player Position and
  Look` and published at once, not with the 1 s status. A kick or connection
  error (`world.ended()`) no longer ends the program: it drops the session and
  shows `Disconnected` with the reason; "Back to server list" returns to the
  list. The pause menu's Disconnect and Escape or Cancel while connecting do
  the same without a reason. `exit_error` is only for fatal startup errors.
- **Dropping never blocks the render thread.** `Drop for Session` joins the
  game and network threads, and the network thread can sit in a connect
  attempt for seconds. Ending a session therefore moves it to a short-lived
  `umicraft-session-drop` thread. Only the program exit still drops inline.
- **A new session starts clean.** `WorldView::clear_session` forgets the
  meshes, camera history, HUD, inventory, statistics and end reason; the mesh
  workers and the terrain stay.
- **Statistics.** The pause menu's Statistics button sends `Client Status`
  request-stats (action 1, `GameEvent::RequestStats`). The game thread already
  merged `Statistics` (0x37) into the server state; it now forwards the
  values by name (`RenderEvent::Stats`) and the window shows the general ones
  (`stat.<id>`, sorted) with the vanilla formats of `StatBase`: grouped
  integers, distances in cm, m or km, times from ticks, damage in tenths.
  The key of a row is the packet's name, which is the language key; an
  unknown key shows as itself. Achievements stay inert (out of P10); the
  block, item and mob tabs are not built.
- **Last direct address.** Saved as `[multiplayer] last_server` in
  `options.toml` and used to fill the direct-connect field.
- **Offline name.** `--name` (default `umicraft`) is the offline name of every
  session; accounts are F1.
- **Offline texts.** The title and server list keys are in the built-in
  language table, so a first run without downloaded assets shows words.
- **Panorama seam.** `MenuFlow::show` returns the optional `PanoramaView` the
  title screen computes; until ADR-0045 the window ignored it and `PanoramaTextures` stayed
  `None`, so the default dirt background is drawn. The GPU cube pass (six
  textures from `umicraft_assets::panorama`, a perspective pass, no blur) is
  the next lot and only has to consume `MenuFrame::panorama` (done, ADR-0045).
- **Demo.** `--hud-demo --hud-view title` and `--hud-view servers` render the
  two screens offscreen without a window (three sample entries, not pinged).

## Not done

- Singleplayer (drawn disabled); LAN entries, server
  icons, SRV lookup; the connecting screen always shows "Connecting to the
  server..." (no separate "Logging in" stage, no status line); achievements;
  statistics tabs for blocks, items and mobs; no refresh of the statistics
  while the screen is open.
- Nothing here was run in a real window by the author's tooling: the lifecycle
  is covered by unit tests of its decisions and by the existing integration
  tests, and the owner has to click through it (ROADMAP P10 status).
