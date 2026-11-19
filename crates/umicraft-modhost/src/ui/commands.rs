// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client commands (`/name args`) that mods register (F3.4).
//!
//! The [`CommandTable`] is meant for the game thread: the client calls
//! [`CommandTable::run`] on every chat line before it would be sent, and a line that
//! is not one of an enabled mod's commands comes back as
//! [`CommandOutcome::NotACommand`] so the client handles it as before (a server
//! command, for instance). Handlers run synchronously under the 1 ms budget.

use std::ops::Bound;
use std::sync::{Arc, Weak};

use umicraft_modapi_sys::{UmiCommandFn, UmiStatus};

use super::shared::HostShared;
use crate::budget::BUDGET;
use crate::context::ModContext;
use crate::events::umi_str;
use crate::ffi::UserPtr;
use crate::hook::run_hook;

/// A registered command. The owner is weak: the table lives in the host-wide state
/// that every context points to, so a strong pointer back would be a cycle.
#[derive(Debug, Clone)]
pub(crate) struct CommandEntry {
    pub(crate) owner: Weak<ModContext>,
    pub(crate) usage: Arc<str>,
    pub(crate) run: UmiCommandFn,
    pub(crate) user: UserPtr,
}

impl CommandEntry {
    /// The owner, if it is still alive and enabled.
    fn enabled_owner(&self) -> Option<Arc<ModContext>> {
        self.owner.upgrade().filter(|owner| owner.is_enabled())
    }
}

/// What [`CommandTable::run`] did with a chat line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Not a command of an enabled mod: the caller treats the line as usual.
    NotACommand,
    /// The command ran and succeeded.
    Ran,
    /// The command refused its arguments (status `ERROR`): the usage line was
    /// printed and the mod stays enabled. A typo of the player is not a broken mod.
    Rejected,
    /// The command broke (panic, another status or budget); its mod is now disabled.
    /// The line was meant for the mod, so the caller should not send it on.
    Failed,
}

/// Runs and completes the commands of every mod of a host.
///
/// `Send + Clone`: the client moves a clone to the game thread. The table is shared
/// with the host, so it always reflects the enable switches of the mods.
#[derive(Debug, Clone)]
pub struct CommandTable {
    shared: Arc<HostShared>,
}

/// Splits `/name rest` into the name and the arguments (without the separator).
fn split_command(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('/')?;
    Some(
        rest.split_once(char::is_whitespace)
            .map_or((rest, ""), |(name, args)| (name, args.trim_start())),
    )
}

impl CommandTable {
    pub(crate) fn new(shared: Arc<HostShared>) -> Self {
        Self { shared }
    }

    /// Runs `line` if it is `/<name> [args]` for a command of an enabled mod.
    ///
    /// The handler is budgeted like an intercept handler. `ERROR` means the
    /// arguments were refused: the usage is printed and [`CommandOutcome::Rejected`]
    /// returned. Any other failure fails its mod and returns [`CommandOutcome::Failed`].
    pub fn run(&self, line: &str) -> CommandOutcome {
        let Some((name, args)) = split_command(line) else {
            return CommandOutcome::NotACommand;
        };
        // Cloned out so the table's lock is not held while foreign code runs.
        let entry = self.shared.table().get(name).cloned();
        let Some((entry, owner)) = entry.and_then(|e| e.enabled_owner().map(|o| (e, o))) else {
            return CommandOutcome::NotACommand;
        };
        let mut rejected = false;
        let ok = run_hook(
            &owner,
            owner.command_counters(),
            BUDGET,
            format_args!("its command `/{name}`"),
            || match (entry.run)(umi_str(args), entry.user.as_ptr()) {
                UmiStatus::ERROR => {
                    rejected = true;
                    UmiStatus::OK
                }
                status => status,
            },
        );
        match (ok, rejected) {
            (false, _) => CommandOutcome::Failed,
            (true, true) => {
                // A full outbox drops the line; the count is already kept there.
                let _ = self.shared.print(format!("Usage: /{name} {}", entry.usage));
                CommandOutcome::Rejected
            }
            (true, false) => CommandOutcome::Ran,
        }
    }

    /// The names of the enabled mods' commands that start with `prefix`, sorted and
    /// without the slash, for tab completion. `prefix` is the text typed after the
    /// slash; a leading `/` is accepted and ignored.
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.strip_prefix('/').unwrap_or(prefix);
        self.shared
            .table()
            .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
            .take_while(|(name, _)| name.starts_with(prefix))
            .filter(|(_, entry)| entry.enabled_owner().is_some())
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// The usage text of `/name`, if it belongs to an enabled mod.
    pub fn usage(&self, name: &str) -> Option<String> {
        let name = name.strip_prefix('/').unwrap_or(name);
        let table = self.shared.table();
        let entry = table.get(name)?;
        entry.enabled_owner()?;
        Some(entry.usage.to_string())
    }
}

#[cfg(test)]
mod tests;
