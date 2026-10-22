// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-core
//!
//! Minimal runtime for Umicraft (spec §5.1): a registry of static [`Module`]s,
//! a typed [`ServiceRegistry`], an [`EventBus`], a fixed-timestep [`Scheduler`],
//! and TOML-backed [`ConfigStore`]s. All gameplay features hook in as modules
//! through these primitives, so mods (M15) can replace or wrap built-ins.
//!
//! Untrusted input — files, mod manifests, (from M1) server packets — is never
//! allowed to surface as a panic: fallible operations return typed errors (see
//! [`error`]).
//!
//! ```
//! use umicraft_core::{Core, Module, ModuleCtx, ModuleId, error::CoreResult};
//!
//! # struct Hello { id: ModuleId }
//! # impl Module for Hello {
//! #     fn id(&self) -> &ModuleId { &self.id }
//! #     fn on_load(&mut self, _ctx: &mut ModuleCtx<'_>) -> CoreResult<()> {
//! #         Ok(())
//! #     }
//! # }
//! let mut core = Core::new();
//! core.add_module(Box::new(Hello { id: ModuleId::new("hello") })).unwrap();
//! core.start().unwrap();
//! ```

pub mod config;
pub mod error;
pub mod events;
pub mod module;
pub mod scheduler;
pub mod service;

pub use config::ConfigStore;
pub use error::{CoreError, CoreResult};
pub use events::{Event, EventBus, EventCx};
pub use module::{Core, Module, ModuleCtx, ModuleId, ModuleRegistry};
pub use scheduler::{PostTick, PreTick, Scheduler};
pub use service::{ServiceInfo, ServiceRegistry};
