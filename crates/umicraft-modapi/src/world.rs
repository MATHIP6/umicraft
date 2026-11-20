// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only questions about the world.

use core::ffi::c_void;
use std::marker::PhantomData;

use umicraft_modapi_sys::{UmiEntity, UmiStatus};

use crate::ffi::{entity_trampoline, str_of, Visit};
use crate::{host, ModError};

/// A block as the server sends it: an id and its metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    /// The block id (0 is air).
    pub id: u16,
    /// The metadata nibble, `0..=15`.
    pub meta: u8,
}

impl Block {
    /// Splits the wire state `id << 4 | meta`.
    fn from_state(state: u32) -> Self {
        Self {
            id: u16::try_from(state >> 4).unwrap_or(u16::MAX),
            meta: u8::try_from(state & 0xF).unwrap_or(0),
        }
    }
}

/// One entity of the world, lent to an [`World::entities`] visitor.
#[derive(Debug, Clone, Copy)]
pub struct Entity<'a> {
    raw: UmiEntity,
    _call: PhantomData<&'a ()>,
}

impl Entity<'_> {
    pub(crate) fn new(raw: UmiEntity) -> Self {
        Self {
            raw,
            _call: PhantomData,
        }
    }

    /// The server's entity id.
    pub fn id(&self) -> i32 {
        self.raw.id
    }

    /// Stable snake_case kind: `player`, `zombie`, `item`, ...
    pub fn kind(&self) -> &str {
        str_of(&self.raw.kind)
    }

    /// Position, in blocks.
    pub fn position(&self) -> [f64; 3] {
        self.raw.position
    }

    /// View angle in degrees.
    pub fn yaw(&self) -> f32 {
        self.raw.yaw
    }

    /// View angle in degrees.
    pub fn pitch(&self) -> f32 {
        self.raw.pitch
    }

    /// A player's name or the name tag, empty when there is none.
    pub fn name(&self) -> &str {
        str_of(&self.raw.name)
    }
}

/// Read-only world queries.
///
/// Cheap to copy and `Send + Sync`, but the host only answers on the game
/// thread inside a callback: a game-event intercept handler (for example
/// [`PostTick`](crate::PostTick)), a command, or a physics layer. Anywhere else
/// (observers, the HUD, other threads) it refuses with `HostRefused(UNSUPPORTED)`.
/// No permission is needed. Get one with [`InitCx::world`](crate::InitCx::world).
#[derive(Debug, Clone, Copy)]
pub struct World;

impl World {
    /// The block at `(x, y, z)`.
    ///
    /// # Errors
    /// - [`ModError::Unavailable`] when the chunk is not loaded.
    /// - [`ModError::HostRefused`] with `UNSUPPORTED` outside a callback on
    ///   the game thread (or before `init`).
    pub fn block(&self, x: i32, y: i32, z: i32) -> Result<Block, ModError> {
        let mut state = 0_u32;
        host::call(|host, me| (host.world_block)(me, x, y, z, &raw mut state))
            .and_then(ModError::check)?;
        Ok(Block::from_state(state))
    }

    /// Calls `visitor` for every entity but the local player; it returns
    /// `false` to stop. A panic in the visitor is caught and stops the visit.
    ///
    /// ```ignore
    /// world.entities(|entity| {
    ///     println!("{} at {:?}", entity.kind(), entity.position());
    ///     true
    /// })?;
    /// ```
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `UNSUPPORTED` outside a callback on the
    /// game thread (or before `init`).
    pub fn entities(&self, mut visitor: impl FnMut(&Entity<'_>) -> bool) -> Result<(), ModError> {
        let mut visit = Visit::new(&mut visitor);
        let user = (&raw mut visit).cast::<c_void>();
        let status =
            host::call(|host, me| (host.world_entities)(me, Some(entity_trampoline), user))?;
        // Stopping early is a success, whatever status the host gives it.
        if visit.stopped() || status == UmiStatus::OK {
            Ok(())
        } else {
            ModError::check(status)
        }
    }
}
