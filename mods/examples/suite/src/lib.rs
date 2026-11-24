// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: a suite of features in the mods menu (API 1.2).
//!
//! Shows how a mod that bundles several features gives each one a card of its own,
//! like the built-in mods:
//!
//! - a tab "Suite" (`InitCx::category`), created by the mod: a mod cannot put cards
//!   in the built-in tabs;
//! - a module "Tick counter" in that tab, whose widget is attached to it (the host
//!   hides the widget while the module is off) and whose card has a slider and a
//!   "Reset" button;
//! - a module "Greeter" under "External", off by default, that greets in the chat
//!   when it is switched on (`ModuleToggled`) and has a "Wave" button;
//! - an "About" button on the mod's own card.
//!
//! Buttons and switches arrive as `ButtonPressed` and `ModuleToggled` on the
//! render thread, for this mod only.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use umicraft_modapi::{
    umicraft_mod, Anchor, ButtonPressed, InitCx, Mod, ModError, ModuleSpec, ModuleToggled,
    Placement, PostTick,
};

/// The key of the tick counter module, as the events name it.
const COUNTER: &str = "counter";
/// The key of the greeter module.
const GREETER: &str = "greeter";

/// Everything lives in the closures.
struct Suite;

impl Mod for Suite {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let tab = cx.category("suite", "Suite")?;
        let counter = cx.module(
            ModuleSpec::new(COUNTER, "Tick counter")
                .description("Counts the game ticks since the last reset")
                .category(tab),
        )?;
        let greeter = cx.module(
            ModuleSpec::new(GREETER, "Greeter")
                .description("Says hello in the chat")
                .default_on(false),
        )?;

        // The counter: a widget tied to its module, a setting and a button on its card.
        let ticks = Arc::new(AtomicU64::new(0));
        let counted = Arc::clone(&ticks);
        cx.observe::<PostTick>(0, move |_| {
            if counter.is_enabled() {
                counted.fetch_add(1, Ordering::Relaxed);
            }
        })?;
        let shown = Arc::clone(&ticks);
        let mut card = cx.card(counter);
        let every = card.slider("every", "Count every", 1.0, 20.0, 1.0, 1.0)?;
        card.button("reset", "Counter", "Reset")?;
        cx.hud_widget(
            "ticks",
            "Ticks",
            Placement::new(Anchor::TopRight),
            move |hud| {
                // `every` is at least 1: the host clamps the slider to its range.
                let step = every.get().max(1.0) as u64;
                let ticks = shown.load(Ordering::Relaxed) / step;
                hud.line(&format!("ticks: {ticks}"), 0x00FF_FFFF);
            },
        )?;
        cx.card(counter).attach_widget("ticks")?;

        // The greeter: a button on its card, and a line when it is switched on.
        cx.card(greeter).button("wave", "Greeting", "Wave")?;
        cx.button("about", "This mod", "About")?;

        let chat = cx.chat();
        cx.observe::<ModuleToggled>(0, move |event| {
            if event.module() == GREETER && event.enabled() {
                chat.print("Hello! The greeter is on.");
            }
        })?;
        cx.observe::<ButtonPressed>(0, move |event| match (event.module(), event.key()) {
            (COUNTER, "reset") => ticks.store(0, Ordering::Relaxed),
            (GREETER, "wave") => chat.print("o/"),
            ("", "about") => chat.print("Suite: an example of tabs, modules and buttons."),
            _ => {}
        })?;
        Ok(Suite)
    }
}

umicraft_mod!(Suite);
