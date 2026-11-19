// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `Mod` trait and the context handed to it during `init`.

use umicraft_modapi_sys::{UmiHost, UmiModHandle, UmiMode};

use crate::event::{Control, Event};
use crate::ffi::{trampoline, Handler, OwnedHandler};
use crate::{Logger, ModError};

/// A mod: built once by the host, told when it is shut down.
pub trait Mod: Send + Sized + 'static {
    /// Builds the mod and registers its handlers. An `Err` makes the host
    /// refuse to load the mod; handlers registered before it are dropped.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError>;

    /// Called once before the library is unloaded, while handlers are still
    /// alive. The default does nothing.
    fn shutdown(&mut self) {}
}

/// What a mod may do during [`Mod::init`]; it is only valid for that call.
#[derive(Debug)]
pub struct InitCx<'a> {
    host: &'a UmiHost,
    me: UmiModHandle,
    handlers: Vec<OwnedHandler>,
}

impl<'a> InitCx<'a> {
    pub(crate) fn new(host: &'a UmiHost, me: UmiModHandle) -> Self {
        Self {
            host,
            me,
            handlers: Vec::new(),
        }
    }

    pub(crate) fn into_handlers(self) -> Vec<OwnedHandler> {
        self.handlers
    }

    /// A logger that stays valid after `init` returns.
    pub fn logger(&self) -> Logger {
        Logger::new(self.host.log, self.me)
    }

    /// Watches events of type `E`; lower `priority` runs first.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] when the host rejects the subscription.
    pub fn observe<E: Event>(
        &mut self,
        priority: i32,
        mut handler: impl FnMut(&E) + Send + 'static,
    ) -> Result<(), ModError> {
        self.subscribe::<E>(
            UmiMode::OBSERVE,
            priority,
            Box::new(move |event, _control| handler(event)),
        )
    }

    /// Takes part in events of type `E`: the handler may [`Control::cancel`]
    /// them when the event allows it. Lower `priority` runs first.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] when the host rejects the subscription.
    pub fn intercept<E: Event>(
        &mut self,
        priority: i32,
        handler: impl FnMut(&mut E, &mut Control) + Send + 'static,
    ) -> Result<(), ModError> {
        self.subscribe::<E>(UmiMode::INTERCEPT, priority, Box::new(handler))
    }

    fn subscribe<E: Event>(
        &mut self,
        mode: UmiMode,
        priority: i32,
        handler: Handler<E>,
    ) -> Result<(), ModError> {
        let owned = OwnedHandler::new(handler);
        let status = (self.host.subscribe)(
            self.me,
            E::ID,
            mode,
            priority,
            Some(trampoline::<E>),
            owned.user(),
        );
        if !status.is_ok() {
            // `owned` drops here: the host did not keep the pointer.
            return Err(ModError::HostRefused(status));
        }
        self.handlers.push(owned);
        Ok(())
    }
}
