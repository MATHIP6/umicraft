// SPDX-License-Identifier: MIT OR Apache-2.0

//! The third-party capes of players (ADR-0081): which player to look up, the
//! answers of the cape worker, and the GPU textures their frames become.
//!
//! The lookups run on the [`CapeResolver`] thread; the render thread only
//! asks and collects, as for the skins (`skins.rs`). The worker starts with
//! the first lookup, so a client with the Capes mod off never starts it and
//! never sends a request.

use std::collections::HashMap;
use std::time::Instant;

use umicraft_assets::capes::{CapeOwner, CapeProvider, CapeQuery, CapeResolver, CapeResult};
use umicraft_model::entity::TextureSlot;

/// How long each frame of an animated cape shows, in milliseconds (the
/// Fabric mod "Capes", `docs/sources.md` section "Capes").
const FRAME_MS: u64 = 100;

/// What the Capes mod asks for, read from it every frame
/// (`ModsRuntime::capes`). `Copy`, so reading it each frame never allocates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapeSettings {
    /// The provider of the local player's cape; `None` keeps the Mojang one.
    pub own: Option<CapeProvider>,
    /// Whether each provider is asked, in the order of [`CapeProvider::ALL`].
    pub enabled: [bool; CapeProvider::ALL.len()],
}

impl CapeSettings {
    /// Whether `provider`'s toggle is on.
    fn is_enabled(&self, provider: CapeProvider) -> bool {
        CapeProvider::ALL
            .iter()
            .zip(self.enabled)
            .any(|(p, on)| *p == provider && on)
    }

    /// The providers to ask for `uuid`: only the chosen one for the local
    /// player, and only while its toggle is on; every enabled one, in lookup
    /// order, for the others.
    fn providers_for(&self, uuid: u128, own_uuid: Option<u128>) -> Vec<CapeProvider> {
        if own_uuid == Some(uuid) {
            return self
                .own
                .filter(|own| self.is_enabled(*own))
                .into_iter()
                .collect();
        }
        CapeProvider::ALL
            .into_iter()
            .filter(|p| self.is_enabled(*p))
            .collect()
    }
}

/// Where the lookup of one player stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Asked, no answer yet.
    Pending,
    /// No provider has a cape for the player, or the player is not looked up.
    Absent,
    /// The frames are uploaded at `first..first + frames`.
    Ready { first: u32, frames: u32 },
}

/// The frame of an animated cape of `frames` frames shown at `now_ms`.
fn frame_at(now_ms: u64, frames: u32) -> u32 {
    let frames = u64::from(frames.max(1));
    // The modulo is below `frames`, a `u32`.
    u32::try_from((now_ms / FRAME_MS) % frames).unwrap_or(0)
}

/// The cape lookups of the session and their textures.
pub(super) struct CapeSet {
    /// The worker, started by the first lookup and dropped by a reset, which
    /// also drops the answers to the old settings still on their way.
    resolver: Option<CapeResolver>,
    settings: Option<CapeSettings>,
    own_uuid: Option<u128>,
    states: HashMap<u128, State>,
    /// Set by a reset: the next collect frees the uploaded frames.
    stale_textures: bool,
    next_index: u32,
    start: Instant,
    now_ms: u64,
}

impl CapeSet {
    /// An empty set; nothing runs until the first lookup.
    pub(super) fn new() -> Self {
        Self {
            resolver: None,
            settings: None,
            own_uuid: None,
            states: HashMap::new(),
            stale_textures: false,
            next_index: 0,
            start: Instant::now(),
            now_ms: 0,
        }
    }

    /// Takes the settings of the mod (`None` while it is off) and the UUID of
    /// the local player. A change of either starts every lookup over, so a
    /// new choice shows at once.
    pub(super) fn configure(&mut self, settings: Option<CapeSettings>, own_uuid: Option<u128>) {
        if settings == self.settings && own_uuid == self.own_uuid {
            return;
        }
        self.settings = settings;
        self.own_uuid = own_uuid;
        self.resolver = None;
        self.states.clear();
        self.stale_textures = true;
    }

    /// The texture index of the frame to draw for the player `uuid` named
    /// `name`, asking for the lookup on the first call. `None` while the mod
    /// is off, the lookup runs, or the player has no third-party cape.
    pub(super) fn lookup(&mut self, uuid: u128, name: &str) -> Option<u32> {
        let settings = self.settings.as_ref()?;
        match self.states.get(&uuid) {
            Some(State::Ready { first, frames }) => Some(first + frame_at(self.now_ms, *frames)),
            Some(_) => None,
            None => {
                let query = CapeQuery {
                    owner: CapeOwner {
                        uuid,
                        name: name.to_owned(),
                    },
                    providers: settings.providers_for(uuid, self.own_uuid),
                };
                if let Some(state) = self.ask(query) {
                    self.states.insert(uuid, state);
                }
                None
            }
        }
    }

    /// Hands `query` to the worker. `Absent` without a request when nobody
    /// is to be asked; `None` when the queue is full, to ask again next frame.
    fn ask(&mut self, query: CapeQuery) -> Option<State> {
        if query.providers.is_empty() || !query.owner.is_lookup_allowed() {
            return Some(State::Absent);
        }
        let resolver = self.resolver.get_or_insert_with(CapeResolver::spawn);
        resolver.request(query).then_some(State::Pending)
    }

    /// Advances the animation clock, frees the frames of a reset and uploads
    /// the capes that arrived, through `upload` (slot, RGBA, width, height).
    /// `remove` drops the textures of a reset.
    pub(super) fn collect(
        &mut self,
        mut upload: impl FnMut(TextureSlot, &[u8], u32, u32),
        remove: impl FnOnce(),
    ) {
        self.now_ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
        if std::mem::take(&mut self.stale_textures) {
            remove();
        }
        let Some(resolver) = &self.resolver else {
            return;
        };
        for result in resolver.poll() {
            Self::apply(&mut self.states, &mut self.next_index, result, &mut upload);
        }
    }

    /// Records one answer of the worker and uploads its frames.
    fn apply(
        states: &mut HashMap<u128, State>,
        next_index: &mut u32,
        result: CapeResult,
        upload: &mut impl FnMut(TextureSlot, &[u8], u32, u32),
    ) {
        if states.get(&result.uuid) != Some(&State::Pending) {
            return;
        }
        let state = match result.cape {
            Some(found) => {
                let first = *next_index;
                let image = found.image;
                for (offset, frame) in (0u32..).zip(&image.frames) {
                    upload(
                        TextureSlot::CustomCape(first + offset),
                        frame,
                        image.width,
                        image.height,
                    );
                }
                let frames = u32::try_from(image.frames.len()).unwrap_or(u32::MAX);
                *next_index = first.saturating_add(frames);
                tracing::debug!(
                    provider = found.provider.key(),
                    frames,
                    "third-party cape ready"
                );
                State::Ready { first, frames }
            }
            None => State::Absent,
        };
        states.insert(result.uuid, state);
    }
}

#[cfg(test)]
mod tests {
    use umicraft_assets::capes::{CapeImage, FoundCape};

    use super::*;

    /// A version-4 UUID, which may be looked up.
    const ONLINE: u128 = 0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5;
    /// A version-3 UUID (offline-mode server), never looked up.
    const OFFLINE: u128 = 0x069a_79f4_44e9_3726_a5be_fca9_0e38_aaf5;

    fn all_on(own: Option<CapeProvider>) -> CapeSettings {
        CapeSettings {
            own,
            enabled: [true; 4],
        }
    }

    fn cape(frames: usize) -> FoundCape {
        FoundCape {
            provider: CapeProvider::OptiFine,
            image: CapeImage {
                width: 64,
                height: 32,
                frames: vec![vec![0; 64 * 32 * 4]; frames],
            },
        }
    }

    /// Feeds an answer as the worker would and returns the uploaded slots.
    fn answer(set: &mut CapeSet, uuid: u128, found: Option<FoundCape>) -> Vec<TextureSlot> {
        let mut slots = Vec::new();
        let result = CapeResult { uuid, cape: found };
        CapeSet::apply(
            &mut set.states,
            &mut set.next_index,
            result,
            &mut |slot, _: &[u8], _, _| slots.push(slot),
        );
        slots
    }

    #[test]
    fn the_local_player_asks_only_the_chosen_and_enabled_provider() {
        let mut settings = all_on(Some(CapeProvider::LabyMod));
        let me = Some(ONLINE);
        assert_eq!(settings.providers_for(ONLINE, me), [CapeProvider::LabyMod]);
        assert_eq!(settings.providers_for(1, me), CapeProvider::ALL);
        settings.enabled[1] = false;
        assert!(settings.providers_for(ONLINE, me).is_empty());
        assert_eq!(
            settings.providers_for(1, me),
            [
                CapeProvider::OptiFine,
                CapeProvider::MinecraftCapes,
                CapeProvider::Cosmetica
            ]
        );
        assert!(
            all_on(None).providers_for(ONLINE, me).is_empty(),
            "Mojang cape"
        );
    }

    #[test]
    fn frames_change_every_100_ms_and_loop() {
        assert_eq!(frame_at(0, 3), 0);
        assert_eq!(frame_at(99, 3), 0);
        assert_eq!(frame_at(100, 3), 1);
        assert_eq!(frame_at(300, 3), 0);
        assert_eq!(frame_at(12_345, 1), 0);
        assert_eq!(frame_at(500, 0), 0, "a zero count does not divide by zero");
    }

    #[test]
    fn nothing_is_looked_up_while_the_mod_is_off() {
        let mut set = CapeSet::new();
        assert_eq!(set.lookup(ONLINE, "Notch"), None);
        assert!(set.states.is_empty());
        assert!(set.resolver.is_none(), "no worker, no request");
    }

    #[test]
    fn offline_players_and_players_without_providers_are_absent_without_a_request() {
        let mut set = CapeSet::new();
        set.configure(Some(all_on(None)), Some(ONLINE));
        assert_eq!(set.lookup(OFFLINE, "Steve"), None);
        assert_eq!(set.lookup(ONLINE, "Me"), None);
        assert_eq!(set.states.get(&OFFLINE), Some(&State::Absent));
        assert_eq!(set.states.get(&ONLINE), Some(&State::Absent));
        assert!(set.resolver.is_none());
    }

    #[test]
    fn an_answer_uploads_every_frame_and_the_lookup_animates_them() {
        let mut set = CapeSet::new();
        set.configure(Some(all_on(None)), None);
        set.states.insert(ONLINE, State::Pending);
        let slots = answer(&mut set, ONLINE, Some(cape(3)));
        let expected: Vec<_> = (0..3).map(TextureSlot::CustomCape).collect();
        assert_eq!(slots, expected);
        set.now_ms = 250;
        assert_eq!(set.lookup(ONLINE, "Notch"), Some(2));
        set.states.insert(7, State::Pending);
        assert_eq!(
            answer(&mut set, 7, Some(cape(1))),
            [TextureSlot::CustomCape(3)]
        );
    }

    #[test]
    fn a_missing_cape_is_remembered_and_unasked_answers_are_ignored() {
        let mut set = CapeSet::new();
        set.configure(Some(all_on(None)), None);
        set.states.insert(ONLINE, State::Pending);
        assert!(answer(&mut set, ONLINE, None).is_empty());
        assert_eq!(set.states.get(&ONLINE), Some(&State::Absent));
        assert!(
            answer(&mut set, 42, Some(cape(1))).is_empty(),
            "never asked"
        );
        assert!(!set.states.contains_key(&42));
    }

    #[test]
    fn a_settings_change_starts_over_and_frees_the_textures() {
        let mut set = CapeSet::new();
        set.configure(Some(all_on(None)), None);
        set.states.insert(ONLINE, State::Absent);
        let mut removed = false;
        set.collect(|_, _: &[u8], _, _| {}, || removed = true);
        assert!(removed, "the first configuration counts as a reset");

        set.configure(Some(all_on(None)), None);
        let mut removed = false;
        set.collect(|_, _: &[u8], _, _| {}, || removed = true);
        assert!(!removed, "same settings: nothing changes");
        assert!(set.states.contains_key(&ONLINE));

        set.configure(Some(all_on(Some(CapeProvider::OptiFine))), None);
        assert!(set.states.is_empty());
        let mut removed = false;
        set.collect(|_, _: &[u8], _, _| {}, || removed = true);
        assert!(removed);
    }
}
