// SPDX-License-Identifier: MIT OR Apache-2.0

//! Umicraft audio engine (milestone P11).
//!
//! The game never plays a sound itself. It sends [`SoundRequest`]s through an
//! [`AudioHandle`], a bounded channel that drops requests instead of blocking,
//! to the audio thread ([`thread`]) which owns everything else: the
//! [`SoundIndex`] (`sounds.json`), the PCM cache, the vanilla attenuation and
//! source limits, and the mixing [`backend`].
//!
//! The crate depends on neither `umicraft-game` nor `umicraft-net`; callers
//! translate their events into [`SoundRequest`]s. See ADR-0050 and
//! `docs/threading.md`.

pub mod attenuation;
pub mod backend;
pub mod cache;
pub mod category;
pub mod decode;
pub mod engine;
pub mod error;
pub mod handle;
pub mod index;
pub mod limits;
pub mod music;
pub mod request;
pub mod stream;
pub mod thread;

pub use category::{CategoryVolumes, SoundCategory};
pub use error::AudioError;
pub use handle::{channel, AudioHandle, AudioMessage, MusicActivity};
pub use index::{SeededRng, SoundIndex, SoundRng};
pub use request::{Listener, SoundRequest};
