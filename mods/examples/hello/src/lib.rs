// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: hello.
//!
//! The smallest useful Umicraft mod, meant to be copied as a starting point. It shows
//! the three things most mods do: log through the host, observe an event, and
//! intercept an event. See `mods/examples/README.md` for how to build and install it.

use umicraft_modapi::{umicraft_mod, InitCx, Mod, ModError, PostTick, PreTick};

/// Priority of the `PostTick` handler; lower values run first.
const POST_TICK_PRIORITY: i32 = 10;

/// The mod's state, kept alive by the host between `init` and `shutdown`.
struct Hello {
    log: umicraft_modapi::Logger,
}

impl Mod for Hello {
    /// Runs once when the client loads the library: say hello and subscribe.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        // A `Logger` is `Copy` and outlives `init`, so handlers may capture it.
        let log = cx.logger();
        log.info("hello from umicraft");

        // Observers get a read-only copy of the event and never delay the game
        // thread. State lives in the closure: no globals needed.
        let mut ticks = 0_u64;
        cx.observe::<PreTick>(0, move |event| {
            ticks += 1;
            if ticks % 200 == 1 {
                log.debug(&format!("pre-tick #{} (game tick {})", ticks, event.tick()));
            }
        })?;

        // Interceptors run synchronously, in priority order, and may cancel events
        // that allow it. This one only looks; `control` would be used to cancel.
        cx.intercept::<PostTick>(POST_TICK_PRIORITY, move |event, _control| {
            if event.tick() % 200 == 0 {
                log.trace(&format!("post-tick {}", event.tick()));
            }
        })?;

        Ok(Self { log })
    }

    /// Runs once when the mod is disabled or the client exits.
    fn shutdown(&mut self) {
        self.log.info("goodbye from umicraft");
    }
}

umicraft_mod!(Hello);
