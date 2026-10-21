// SPDX-License-Identifier: MIT OR Apache-2.0

//! Service registry (spec §5.4).
//!
//! Services are named, versioned interfaces a module *provides*. Another module
//! may *replace* or *wrap* it; plain re-provides fail loudly instead of quietly
//! shadowing. Values are stored type-erased but keyed by their concrete type at
//! provide time, so `get` is a checked downcast, not a guess.
//!
//! Wrapping is how mods customize behavior (e.g. a HUD layer around `hud.frame`),
//! while replacement is reserved for full-service swaps — the conflict policy the
//! spec demands (two accidental replacements must fail, not fight silently).

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use tracing::{debug, info};

use crate::error::CoreError;
use crate::module::ModuleId;

/// Registry of named, typed services provided by modules.
#[derive(Debug, Default)]
pub struct ServiceRegistry {
    entries: HashMap<String, ServiceEntry>,
}

/// Who provides a service, in which version, and what wraps it.
#[derive(Debug)]
pub struct ServiceInfo {
    /// Module that last provided/replaced the service value.
    pub provider: ModuleId,
    /// Provider-declared interface version (monotonic per service).
    pub version: u32,
    /// Modules that wrapped the service, innermost first.
    pub wrappers: Vec<ModuleId>,
    /// Concrete stored type, via `type_name` (diagnostics only).
    pub type_name: &'static str,
}

impl ServiceInfo {
    /// Module that last provided/replaced the service value.
    #[must_use]
    pub fn provider(&self) -> &ModuleId {
        &self.provider
    }

    /// Provider-declared interface version.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Wrap chain, innermost first.
    #[must_use]
    pub fn wrappers(&self) -> &[ModuleId] {
        &self.wrappers
    }

    /// Concrete stored type name, for debug overlays (UI-4) and error paths.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
}

#[derive(Debug)]
struct ServiceEntry {
    value: Box<dyn Any + Send>,
    type_id: TypeId,
    type_name: &'static str,
    provider: ModuleId,
    version: u32,
    wrappers: Vec<ModuleId>,
}

impl ServiceRegistry {
    /// Provides a new service under `name`.
    ///
    /// `version` lets consumers detect interface drift. The stored value is an
    /// [`Arc<T>`], so modules can share it beyond this registry.
    ///
    /// # Errors
    /// [`CoreError::ServiceConflict`] when `name` is already taken.
    pub fn provide<T: Any + Send + Sync>(
        &mut self,
        name: &str,
        provider: ModuleId,
        version: u32,
        value: Arc<T>,
    ) -> Result<(), CoreError> {
        if let Some(existing) = self.get_info(name) {
            return Err(CoreError::ServiceConflict {
                service: name.to_string(),
                existing: existing.provider.to_string(),
            });
        }
        info!(service = name, provider = %provider, "service provided");
        self.entries
            .insert(name.to_string(), Self::entry(provider, version, value));
        Ok(())
    }

    /// Replaces an existing service wholesale; wrap chains of the old value
    /// are discarded (they wrapped what is now gone).
    ///
    /// # Errors
    /// [`CoreError::UnknownService`] when `name` was never provided.
    pub fn replace<T: Any + Send + Sync>(
        &mut self,
        name: &str,
        provider: ModuleId,
        version: u32,
        value: Arc<T>,
    ) -> Result<(), CoreError> {
        if !self.entries.contains_key(name) {
            return Err(CoreError::UnknownService(name.to_string()));
        }
        info!(service = name, provider = %provider, "service replaced");
        self.entries
            .insert(name.to_string(), Self::entry(provider, version, value));
        Ok(())
    }

    /// Wraps the current service value with `f`, appending `provider` to the
    /// wrap chain. Wrapping composes on the previous value, innermost first.
    ///
    /// # Errors
    /// [`CoreError::UnknownService`] when the service does not exist, or
    /// [`CoreError::ServiceTypeMismatch`] when it holds a different type than `T`.
    pub fn wrap<T, F>(&mut self, name: &str, provider: ModuleId, f: F) -> Result<(), CoreError>
    where
        T: Any + Send + Sync,
        F: FnOnce(Arc<T>) -> Arc<T>,
    {
        let entry = self
            .entries
            .get_mut(name)
            .ok_or_else(|| CoreError::UnknownService(name.to_string()))?;
        let inner = Self::arc_of(name, entry)?;
        // The type id/name stay unchanged: wrapping preserves the interface.
        entry.value = Box::new(f(inner));
        entry.wrappers.push(provider);
        debug!(
            service = name,
            wraps = entry.wrappers.len(),
            "service wrapped"
        );
        Ok(())
    }

    /// Fetches the service value as `Arc<T>`.
    ///
    /// # Errors
    /// [`CoreError::UnknownService`] when the name is missing, or
    /// [`CoreError::ServiceTypeMismatch`] when the stored type differs from `T`.
    pub fn get<T: Any + Send + Sync>(&self, name: &str) -> Result<Arc<T>, CoreError> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| CoreError::UnknownService(name.to_string()))?;
        Self::arc_of(name, entry)
    }

    /// Metadata about a registered service, for debug overlays and conflict tests.
    #[must_use]
    pub fn get_info(&self, name: &str) -> Option<ServiceInfo> {
        let entry = self.entries.get(name)?;
        Some(ServiceInfo {
            provider: entry.provider.clone(),
            version: entry.version,
            wrappers: entry.wrappers.clone(),
            type_name: entry.type_name,
        })
    }

    fn entry<T: Any + Send + Sync>(
        provider: ModuleId,
        version: u32,
        value: Arc<T>,
    ) -> ServiceEntry {
        ServiceEntry {
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
            value: Box::new(value),
            provider,
            version,
            wrappers: Vec::new(),
        }
    }

    fn arc_of<T: Any + Send + Sync>(name: &str, entry: &ServiceEntry) -> Result<Arc<T>, CoreError> {
        if entry.type_id != TypeId::of::<T>() {
            return Err(CoreError::ServiceTypeMismatch {
                service: name.to_string(),
                expected: std::any::type_name::<T>(),
                found: entry.type_name,
            });
        }
        // Unreachable in practice: the TypeId check bounds the stored value.
        // Any divergence is caller error, so it degrades to a typed error.
        match entry.value.downcast_ref::<Arc<T>>() {
            Some(arc) => Ok(arc.clone()),
            None => Err(CoreError::ServiceTypeMismatch {
                service: name.to_string(),
                expected: std::any::type_name::<T>(),
                found: entry.type_name,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provide_and_get_round_trip() {
        let mut registry = ServiceRegistry::default();
        let value: Arc<String> = Arc::new("mesher".to_string());
        registry
            .provide("render.mesher", ModuleId::new("renderer"), 1, value)
            .unwrap();

        let fetched: Arc<String> = registry.get("render.mesher").unwrap();
        assert_eq!(&*fetched, "mesher");
        let info = registry.get_info("render.mesher").unwrap();
        assert_eq!(info.provider(), &ModuleId::new("renderer"));
        assert_eq!(info.version(), 1);
        assert!(info.wrappers().is_empty());
    }

    #[test]
    fn duplicate_provide_conflicts() {
        let mut registry = ServiceRegistry::default();
        registry
            .provide("hud.frame", ModuleId::new("hud"), 1, Arc::new(7u32))
            .unwrap();
        let second = registry.provide("hud.frame", ModuleId::new("hud2"), 1, Arc::new(8u32));
        assert!(
            matches!(&second, Err(CoreError::ServiceConflict { service, existing }) if service == "hud.frame" && existing == "hud")
        );
    }

    #[test]
    fn unknown_and_mismatch_errors() {
        let mut registry = ServiceRegistry::default();
        registry
            .provide(
                "physics",
                ModuleId::new("game"),
                1,
                Arc::new(String::from("walk")),
            )
            .unwrap();

        let unknown = registry.get::<usize>("nope");
        assert!(matches!(unknown, Err(CoreError::UnknownService(_))));

        let mismatch = registry.get::<u64>("physics");
        assert!(
            matches!(&mismatch, Err(CoreError::ServiceTypeMismatch { expected, .. }) if *expected == "u64")
        );
    }

    #[test]
    fn replace_changes_provider_and_value() {
        let mut registry = ServiceRegistry::default();
        registry
            .provide("gravity", ModuleId::new("physics"), 1, Arc::new(0.08f64))
            .unwrap();
        registry
            .replace(
                "gravity",
                ModuleId::new("physics_tweak"),
                2,
                Arc::new(0.2f64),
            )
            .unwrap();
        assert_eq!(*registry.get::<f64>("gravity").unwrap(), 0.2f64);
        assert_eq!(
            registry.get_info("gravity").unwrap().provider(),
            &ModuleId::new("physics_tweak")
        );
    }

    #[test]
    fn wrap_composes_innermost_first() {
        let mut registry = ServiceRegistry::default();
        registry
            .provide(
                "label",
                ModuleId::new("ui"),
                1,
                Arc::new(String::from("hud")),
            )
            .unwrap();
        registry
            .wrap("label", ModuleId::new("example_hud"), |old| {
                Arc::new(format!("[{old}]"))
            })
            .unwrap();
        assert_eq!(&*registry.get::<String>("label").unwrap(), "[hud]");
        assert_eq!(registry.get_info("label").unwrap().wrappers().len(), 1);
    }

    use std::sync::atomic::AtomicUsize;

    // `AtomicUsize` is imported for the unknown/mismatch tests only.
    #[test]
    fn unknown_service_reports_missing_name() {
        let registry = ServiceRegistry::default();
        assert!(matches!(
            registry.get::<AtomicUsize>("nope"),
            Err(CoreError::UnknownService(_))
        ));
    }
}
