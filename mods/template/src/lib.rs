// SPDX-License-Identifier: MIT OR Apache-2.0

//! Template mod: the smallest useful Umicraft mod.
//!
//! It registers one setting (a toggle in the mods menu) and one observer that
//! writes a log line once a second while the game runs, then says goodbye when
//! the client exits. Copy the folder, rename it and grow from here; see
//! `README.md` next to this crate.

// `umicraft_modapi` is the library every mod uses to talk to the client.
// `umicraft_mod!` below turns a type into a mod the client can load.
use umicraft_modapi::{umicraft_mod, InitCx, Logger, Mod, ModError, PostTick};

/// The game runs 20 ticks per second, so this is once a second.
const TICKS_PER_LOG: u64 = 20;

/// Your mod. Whatever you store here lives from `init` until `shutdown`.
struct MyMod {
    /// Used to write to the client's log; a `Logger` is cheap to copy.
    log: Logger,
}

impl Mod for MyMod {
    /// Called once when the client loads the library. Register everything
    /// the mod does here: the host refuses registrations made later.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let log = cx.logger();
        log.info("my_mod loaded");

        // A setting shows up in the mods menu. `chatty` is its key (stable,
        // used in the saved config), the second text is its label, and `true`
        // is the default. `enabled.get()` always returns the player's current choice.
        let enabled = cx.toggle("chatty", "Log every second", true)?;

        // An observer is told about an event on a worker thread of its own, so
        // it can never slow the game down. `PostTick` fires after each of the
        // 20 game ticks per second; `0` is the priority (lower runs first).
        cx.observe::<PostTick>(0, move |event| {
            // `event.tick()` counts ticks from 0; act on every 20th one.
            if enabled.get() && event.tick() % TICKS_PER_LOG == 0 {
                log.info(&format!("tick {}", event.tick()));
            }
        })?;

        Ok(Self { log })
    }

    /// Called once when the client exits (disabling a mod only stops its
    /// handlers; the library stays loaded until then). Release what you hold.
    fn shutdown(&mut self) {
        self.log.info("my_mod says goodbye");
    }
}

// Exports the C entry points the client looks for, wrapping `MyMod`.
umicraft_mod!(MyMod);
