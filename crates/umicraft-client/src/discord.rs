// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Discord Rich Presence of the client (ADR-0084).
//!
//! The window tells [`Presence::show`] what the player is doing ([`Status`]); a
//! background thread keeps Discord's "Playing Umicraft" card in line with it.
//! Discord is optional: when it is not running, the thread retries quietly and
//! the client never notices. The IPC calls block, so they never run on the
//! render thread.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use discord_rich_presence::activity::{Activity, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use thiserror::Error;

use crate::screens::Screen;

/// The Discord application that owns the card (created in the Developer Portal;
/// the id is public, it appears in every Rich Presence payload).
const APPLICATION_ID: &str = "1572783795797622814";

/// How long the worker waits before it looks for Discord again after a failure.
const RETRY_EVERY: Duration = Duration::from_secs(15);

/// Why Discord could not be reached or updated.
#[derive(Debug, Error)]
pub enum DiscordError {
    /// The IPC socket is missing, closed or answered something unexpected.
    #[error("discord IPC: {0}")]
    Ipc(#[from] discord_rich_presence::error::Error),
}

/// What the player is doing, as far as the card says.
///
/// Deliberately coarse: no server address or name is shown (privacy by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// In the title screen, the server list or any other menu.
    Menu,
    /// A session is starting; the world is not shown yet.
    Connecting,
    /// In a game on a server.
    Playing,
}

impl Status {
    /// The status that goes with the current `screen`.
    ///
    /// `in_session` is whether a play session exists; a full-screen menu over
    /// it (the disconnected screen) counts as a menu, like
    /// `App::world_visible` decides what is drawn.
    #[must_use]
    pub const fn of(screen: Screen, in_session: bool) -> Self {
        match screen {
            Screen::Connecting => Self::Connecting,
            _ if in_session && !screen.is_menu() => Self::Playing,
            _ => Self::Menu,
        }
    }

    /// The line shown under the game name.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Menu => "In the menus",
            Self::Connecting => "Connecting to a server",
            Self::Playing => "Playing on a server",
        }
    }
}

/// What the worker needs from Discord, so the retry logic is tested without it.
trait Link {
    /// Opens the connection.
    fn connect(&mut self) -> Result<(), DiscordError>;
    /// Replaces the card with `status`, started at `started` (Unix seconds).
    fn show(&mut self, status: Status, started: i64) -> Result<(), DiscordError>;
}

/// The real Discord IPC.
struct Ipc(DiscordIpcClient);

impl Link for Ipc {
    fn connect(&mut self) -> Result<(), DiscordError> {
        Ok(self.0.connect()?)
    }

    fn show(&mut self, status: Status, started: i64) -> Result<(), DiscordError> {
        let activity = Activity::new()
            .details(status.text())
            .timestamps(Timestamps::new().start(started));
        Ok(self.0.set_activity(activity)?)
    }
}

/// The handle the window keeps; it feeds the worker thread.
pub struct Presence {
    tx: Sender<Status>,
    last: Option<Status>,
}

impl Presence {
    /// Starts the worker. The elapsed time on the card counts from this call.
    ///
    /// Without a worker (the thread could not start) the handle still works and
    /// drops every update.
    #[must_use]
    pub fn spawn() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let started = unix_now();
        let spawned = std::thread::Builder::new()
            .name("DiscordRpc".to_string())
            .spawn(move || {
                let client = DiscordIpcClient::new(APPLICATION_ID);
                run(&rx, &mut Ipc(client), started, RETRY_EVERY);
            });
        if let Err(e) = spawned {
            tracing::debug!("discord rpc thread not started: {e}");
        }
        Self { tx, last: None }
    }

    /// Shows `status`. Cheap enough to call every frame: only a change is sent.
    pub fn show(&mut self, status: Status) {
        if self.last != Some(status) {
            self.last = Some(status);
            // A closed channel means the worker is gone; there is nothing to do.
            let _ = self.tx.send(status);
        }
    }
}

/// Seconds since the Unix epoch; `0` if the clock is before it.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The worker loop: keeps the card equal to the last status it received,
/// connecting again every `retry` while Discord is out of reach.
///
/// Returns when the sending side is dropped.
fn run(rx: &Receiver<Status>, link: &mut impl Link, started: i64, retry: Duration) {
    let mut wanted: Option<Status> = None;
    let mut shown: Option<Status> = None;
    let mut connected = false;
    loop {
        match rx.recv_timeout(retry) {
            Ok(status) => wanted = Some(status),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let Some(status) = wanted.filter(|w| shown != Some(*w)) else {
            continue;
        };
        match publish(link, &mut connected, status, started) {
            Ok(()) => shown = Some(status),
            Err(e) => {
                tracing::debug!("discord rpc: {e}");
                connected = false;
            }
        }
    }
}

/// Connects if needed, then shows `status`.
fn publish(
    link: &mut impl Link,
    connected: &mut bool,
    status: Status,
    started: i64,
) -> Result<(), DiscordError> {
    if !*connected {
        link.connect()?;
        *connected = true;
    }
    link.show(status, started)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Fake {
        /// How many `connect` calls still fail.
        refusals: usize,
        connects: usize,
        shown: Arc<Mutex<Vec<Status>>>,
    }

    impl Link for Fake {
        fn connect(&mut self) -> Result<(), DiscordError> {
            self.connects += 1;
            if self.refusals > 0 {
                self.refusals -= 1;
                return Err(discord_rich_presence::error::Error::IPCNotFound.into());
            }
            Ok(())
        }

        fn show(&mut self, status: Status, _started: i64) -> Result<(), DiscordError> {
            self.shown.lock().expect("lock").push(status);
            Ok(())
        }
    }

    /// Feeds `statuses` to the worker, closes the channel and returns what it did.
    fn drive(statuses: &[Status], mut fake: Fake) -> (Vec<Status>, usize) {
        let (tx, rx) = crossbeam_channel::unbounded();
        for s in statuses {
            tx.send(*s).expect("send");
        }
        drop(tx);
        let shown = Arc::clone(&fake.shown);
        run(&rx, &mut fake, 0, Duration::from_millis(1));
        let shown = shown.lock().expect("lock").clone();
        (shown, fake.connects)
    }

    #[test]
    fn the_menus_are_menus_and_a_game_is_playing() {
        assert_eq!(Status::of(Screen::Title, false), Status::Menu);
        assert_eq!(Status::of(Screen::ServerList, false), Status::Menu);
        assert_eq!(Status::of(Screen::Connecting, true), Status::Connecting);
        assert_eq!(Status::of(Screen::None, true), Status::Playing);
        assert_eq!(Status::of(Screen::GameMenu, true), Status::Playing);
        assert_eq!(Status::of(Screen::Chat, true), Status::Playing);
    }

    #[test]
    fn the_disconnected_screen_is_a_menu_even_with_a_session() {
        assert_eq!(Status::of(Screen::Disconnected, true), Status::Menu);
    }

    #[test]
    fn a_game_screen_without_a_session_is_a_menu() {
        assert_eq!(Status::of(Screen::None, false), Status::Menu);
    }

    #[test]
    fn the_worker_connects_once_and_shows_the_last_status() {
        let (shown, connects) = drive(&[Status::Menu, Status::Connecting], Fake::default());
        assert_eq!(shown.last(), Some(&Status::Connecting));
        assert_eq!(connects, 1);
    }

    #[test]
    fn the_worker_retries_while_discord_is_closed() {
        let fake = Fake {
            refusals: 2,
            ..Fake::default()
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        tx.send(Status::Menu).expect("send");
        let mut fake = fake;
        let shown = Arc::clone(&fake.shown);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(tx);
        });
        run(&rx, &mut fake, 0, Duration::from_millis(1));
        stopper.join().expect("join");
        assert_eq!(*shown.lock().expect("lock"), vec![Status::Menu]);
        assert_eq!(fake.connects, 3);
    }

    #[test]
    fn presence_sends_only_changes() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut presence = Presence { tx, last: None };
        presence.show(Status::Menu);
        presence.show(Status::Menu);
        presence.show(Status::Playing);
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [Status::Menu, Status::Playing]
        );
    }
}
