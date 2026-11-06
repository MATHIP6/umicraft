// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the server resource pack (NET-11): feeding the events of a
//! frame to [`ServerPackFlow`], carrying out its [`Effect`]s (downloading on a
//! worker thread, reloading the resources, answering the server) and drawing
//! the question.
//!
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use umicraft_assets::cache::AssetCache;
use umicraft_assets::server_pack::{fetch_server_pack_blocking, ValidatedOffer, VANILLA_MAX_BYTES};
use umicraft_core::lang::Lang;
use umicraft_protocol::packets::play::serverbound::ResourcePackResult;
use umicraft_ui::menus::{ConfirmOutcome, ConfirmScreen};
use umicraft_ui::{FontAtlas, Ui};

use super::server_pack::{Effect, ServerPackFlow};
use super::App;
use crate::game::GameEvent;
use crate::screen::Screen;
use crate::servers::ResourcePackMode;

/// A finished download: its generation and the cached pack, `None` on failure.
type Downloaded = (u64, Option<PathBuf>);

/// The server resource pack state of the window.
pub(super) struct ServerPackRuntime {
    flow: ServerPackFlow,
    downloads: Sender<Downloaded>,
    finished: Receiver<Downloaded>,
    /// Statuses the game thread has not accepted yet, oldest first.
    outbox: VecDeque<(String, ResourcePackResult)>,
    /// The answer to the question, until the next frame handles it.
    answer: Option<ConfirmOutcome>,
}

impl Default for ServerPackRuntime {
    fn default() -> Self {
        let (downloads, finished) = channel();
        Self {
            flow: ServerPackFlow::default(),
            downloads,
            finished,
            outbox: VecDeque::new(),
            answer: None,
        }
    }
}

impl ServerPackRuntime {
    /// Sets the pack mode of the server the next session connects to.
    pub(super) fn set_mode(&mut self, mode: ResourcePackMode) {
        self.flow.set_mode(mode);
    }

    /// The loaded server pack, the top-priority resource pack while it exists.
    pub(super) fn active_path(&self) -> Option<&Path> {
        self.flow.active_path()
    }

    /// Draws the question of `multiplayer.texturePrompt` (`GuiYesNo`) and
    /// keeps the answer for the next [`App::update_server_pack`].
    pub(super) fn show_prompt(&mut self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) {
        let question = ConfirmScreen::new(
            "multiplayer.texturePrompt.line1",
            lang.tr("multiplayer.texturePrompt.line2"),
            "gui.yes",
            "gui.no",
        );
        if let Some(answer) = question.show(ui, atlas, lang) {
            self.answer = Some(answer);
        }
    }

    /// Starts the download of `offer` on a short-lived worker thread.
    fn spawn_download(&self, generation: u64, offer: ValidatedOffer) {
        let results = self.downloads.clone();
        let failed = self.downloads.clone();
        let spawned = std::thread::Builder::new()
            .name("umicraft-server-pack".to_string())
            .spawn(move || {
                let _ = results.send((generation, download(&offer)));
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start the server pack download");
            let _ = failed.send((generation, None));
        }
    }
}

/// Gets the pack into the asset cache, logging why it failed.
fn download(offer: &ValidatedOffer) -> Option<PathBuf> {
    let cache = match AssetCache::in_data_dir() {
        Ok(cache) => cache,
        Err(error) => {
            tracing::warn!(%error, "no cache folder for the server resource pack");
            return None;
        }
    };
    match fetch_server_pack_blocking(offer, cache.root(), VANILLA_MAX_BYTES) {
        Ok(path) => Some(path),
        Err(error) => {
            tracing::warn!(%error, "could not get the server resource pack");
            None
        }
    }
}

impl App {
    /// Handles the server pack events of this frame: new offers, the answer
    /// to the question, finished downloads. Then sends the statuses that are
    /// due and opens the question once nothing else is on screen.
    pub(super) fn update_server_pack(&mut self) {
        let mut effects = Vec::new();
        let offers = self
            .world
            .as_mut()
            .map(|world| world.take_pack_requests())
            .unwrap_or_default();
        for request in offers {
            effects.extend(self.server_pack.flow.offer(request));
        }
        if let Some(answer) = self.server_pack.answer.take() {
            effects.extend(self.server_pack.flow.answer(answer == ConfirmOutcome::Yes));
        }
        while let Ok((generation, path)) = self.server_pack.finished.try_recv() {
            effects.extend(self.server_pack.flow.downloaded(generation, path));
        }
        for effect in effects {
            self.apply_pack_effect(effect);
        }
        self.send_pack_statuses();
        self.show_or_close_pack_prompt();
    }

    /// Drops the server pack with the session, as the reference client does
    /// on disconnect, and forgets statuses nobody can receive any more.
    pub(super) fn drop_server_pack(&mut self) {
        self.server_pack.outbox.clear();
        self.server_pack.answer = None;
        for effect in self.server_pack.flow.end_session() {
            self.apply_pack_effect(effect);
        }
    }

    fn apply_pack_effect(&mut self, effect: Effect) {
        match effect {
            Effect::Reply { hash, result } => self.server_pack.outbox.push_back((hash, result)),
            Effect::Download { generation, offer } => {
                self.server_pack.spawn_download(generation, offer);
            }
            Effect::Reload => self.reload_resources(),
        }
    }

    /// Hands the queued statuses to the game thread; a full inbox keeps the
    /// rest for the next frame.
    fn send_pack_statuses(&mut self) {
        while let Some((hash, result)) = self.server_pack.outbox.front().cloned() {
            if !self.send_event(GameEvent::ServerPackStatus { hash, result }) {
                break;
            }
            self.server_pack.outbox.pop_front();
        }
    }

    /// Opens the question when it is due and nothing else owns the screen
    /// (the server may offer while the connecting screen is up), and closes
    /// it once it is answered.
    fn show_or_close_pack_prompt(&mut self) {
        let prompting = self.server_pack.flow.is_prompting();
        if prompting && self.screen() == Screen::None {
            self.set_screen(Screen::PackPrompt);
        } else if !prompting && self.screen == Screen::PackPrompt {
            self.set_screen(Screen::None);
        }
    }
}
