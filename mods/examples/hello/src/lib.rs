// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: hello.
//!
//! A starting point to copy. It shows the things most mods do: log through the
//! host, observe and intercept events, rewrite text, and declare what the
//! player sees (settings, a HUD widget, a key binding, a command). See
//! `mods/examples/README.md` for how to build and install it.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

use umicraft_modapi::{
    umicraft_mod, Anchor, Chat, ChatSend, InitCx, KeyPressed, Mod, ModError, Placement, PostTick,
    PreTick,
};

/// Priority of the `PostTick` handler; lower values run first.
const POST_TICK_PRIORITY: i32 = 10;

/// What the player types, and what is sent instead.
const SHRUG_TOKEN: &str = ":shrug:";
const SHRUG: &str = "\u{af}\\_(\u{30c4})_/\u{af}";

/// Default key of the greeting binding; the player can rebind it.
const GREET_KEY: &str = "KeyH";

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

        let mut ticks = 0_u64;
        // Observers get a read-only copy of the event and never delay the game
        // thread. State lives in the closure: no globals needed. The count is
        // shared with the HUD widget below through a plain atomic.
        let tick_count = Arc::new(AtomicU64::new(0));
        let observed = Arc::clone(&tick_count);
        cx.observe::<PreTick>(0, move |event| {
            ticks += 1;
            observed.store(ticks, Relaxed);
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

        // Intercepting a chat line lets the mod rewrite it before it is sent.
        // `set_text` can fail (a host may refuse), so log instead of ignoring it.
        cx.intercept::<ChatSend>(0, move |event, _control| {
            if event.text().contains(SHRUG_TOKEN) {
                let shrugged = event.text().replace(SHRUG_TOKEN, SHRUG);
                if let Err(error) = event.set_text(&shrugged) {
                    log.warn(&error.to_string());
                }
            }
        })?;

        register_ui(cx, tick_count)?;
        Ok(Self { log })
    }

    /// Runs once when the mod is disabled or the client exits.
    fn shutdown(&mut self) {
        self.log.info("goodbye from umicraft");
    }
}

/// What the player can see and change: settings in the mods menu, a HUD
/// widget, a key binding and a command.
fn register_ui(cx: &mut InitCx<'_>, tick_count: Arc<AtomicU64>) -> Result<(), ModError> {
    // Settings are listed in the mods menu; the returned handles read the
    // player's current choice, from any thread.
    let show = cx.toggle("show_ticks", "Show tick counter", true)?;
    let color = cx.color("tick_color", "Counter colour", 0x55FF55)?;

    // A widget draws a few lines each frame; the player can move it in the
    // HUD editor, `Placement` is only the starting point.
    let placement = Placement::new(Anchor::TopLeft).offset(4.0, 40.0);
    cx.hud_widget("ticks", "Tick counter", placement, move |hud| {
        if show.get() {
            let ticks = tick_count.load(Relaxed);
            hud.line(&format!("ticks: {ticks}"), color.get());
        }
    })?;

    // A key binding is listed on the controls page and arrives as an event;
    // there is no way to watch other keys.
    let chat = cx.chat();
    cx.key("greet", "Say hello", GREET_KEY)?;
    cx.observe::<KeyPressed>(0, move |event| {
        if event.key() == "greet" {
            chat.print("hello from the key binding");
        }
    })?;

    // `/hello <name>` runs on the client; the line never reaches the server.
    cx.command("hello", "/hello <name>", move |name| greet(chat, name))
}

/// Prints the greeting, or fails so the host reports the missing name.
fn greet(chat: Chat, name: &str) -> Result<(), ModError> {
    if name.is_empty() {
        return Err(ModError::msg("usage: /hello <name>"));
    }
    chat.print(&format!("hello, {name}"));
    Ok(())
}

umicraft_mod!(Hello);
