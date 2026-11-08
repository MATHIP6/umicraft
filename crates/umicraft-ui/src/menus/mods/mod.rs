// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods menu, opened with the right Shift key.
//!
//! Laid out like the Lunar Client menu and kept small: over the blurred game,
//! with no panel, the logo, the "UMICRAFT" wordmark under it and one **Mods**
//! button, with a muted hint below. A close cross sits in the top right corner
//! of the screen, as on the title screen. The renderer blurs the game behind
//! it; this screen only adds a light dark sheet. The button only reports
//! [`ModsOutcome::OpenMods`] for now; the mods themselves do not exist yet,
//! this is the interface.

mod animation;
mod button;
mod layout;
mod style;
#[cfg(test)]
mod tests;

pub use layout::{mods_layout, ModsLayout};

use layout::WORDMARK_SCALE;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::menus::common::Gui;
use crate::menus::title::modern::wordmark::{draw_logo, draw_wordmark};
use crate::menus::title::modern::{lowered, scaled};
use crate::modern::theme::with_opacity;
use crate::modern::{draw_text, theme, Align, HoverFade, ModernButton};
use crate::text::FontAtlas;

use animation::{BLUR, LOGO, LOGO_RISE, LOGO_START_SCALE, OPEN_TICKS, SHEET};
use button::mods_button;
use style::{BACKDROP, CLOSE_CROSS};

/// What the player did on the mods menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModsOutcome {
    /// The close cross was clicked.
    Close,
    /// The Mods button was clicked.
    OpenMods,
}

/// The mods menu state: the hover fades of its two buttons and how far the
/// opening animation has come. The flow makes a new one each time the menu
/// opens, so the animation plays again.
#[derive(Debug, Clone, Default)]
pub struct ModsScreen {
    fades: [HoverFade; 2],
    hovered: [bool; 2],
    /// Ticks since the menu opened, up to [`OPEN_TICKS`].
    ticks: u8,
}

impl ModsScreen {
    /// Creates the menu with both buttons at rest.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ticks since the menu opened, `partial_tick` of the way into the current
    /// one; it stands still once the animation is over.
    fn elapsed(&self, partial_tick: f32) -> f32 {
        let running = self.ticks < OPEN_TICKS;
        let partial = if running {
            partial_tick.clamp(0.0, 1.0)
        } else {
            0.0
        };
        f32::from(self.ticks) + partial
    }

    /// How blurred the game behind should be, `0.0` to `1.0`: the renderer
    /// fades the blur in with it.
    #[must_use]
    pub fn blur_strength(&self, partial_tick: f32) -> f32 {
        BLUR.at(self.elapsed(partial_tick))
    }

    /// Advances the opening animation and the hover fades by one game tick.
    pub fn tick(&mut self) {
        self.ticks = (self.ticks + 1).min(OPEN_TICKS);
        for (fade, hovered) in self.fades.iter_mut().zip(self.hovered) {
            fade.tick(hovered);
        }
    }

    /// Draws one frame and returns what the player clicked. `brand` is the
    /// brand atlas and `partial_tick` how far the tick has come.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        brand: &FontAtlas,
        lang: &Lang,
        partial_tick: f32,
    ) -> Option<ModsOutcome> {
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = mods_layout(viewport);

        let elapsed = self.elapsed(partial_tick);
        let logo_in = LOGO.at(elapsed);

        let sheet = with_opacity(BACKDROP, SHEET.at(elapsed));
        ui.draw().rect(viewport, sheet);
        let logo_box = lowered(layout.logo, LOGO_RISE, gui.scale, logo_in);
        let logo_box = scaled(
            logo_box,
            LOGO_START_SCALE + (1.0 - LOGO_START_SCALE) * logo_in,
        );
        let logo = ui.menu_art().brand.logo;
        draw_logo(ui, logo, logo_box, logo_in);
        let name_scale = gui.scale * WORDMARK_SCALE;
        draw_wordmark(ui, brand, layout.wordmark, name_scale, 1.0);
        let hint = lang.tr("umicraft.mods.hint");
        let hint_size = theme::TEXT_SIZE_SMALL * gui.scale;
        draw_text(
            ui,
            brand,
            hint,
            (layout.hint[0], layout.hint[1]),
            (hint_size, theme::TEXT_MUTED, Align::Center),
        );

        ui.push_id("mods_menu");
        let levels = self.fades.map(|fade| fade.level(partial_tick));
        let mods_id = ui.id("mods");
        let close = ModernButton::new(ui.id("close"), layout.close, CLOSE_CROSS);
        let label = lang.tr("umicraft.mods.button");
        let responses = [
            mods_button(
                ui,
                brand,
                (mods_id, layout.mods),
                (gui.scale, levels[0]),
                label,
            ),
            close.show(ui, brand, gui.scale, levels[1]),
        ];
        ui.pop_id();

        self.hovered = responses.map(|response| response.hovered);
        if responses[0].clicked {
            Some(ModsOutcome::OpenMods)
        } else if responses[1].clicked {
            Some(ModsOutcome::Close)
        } else {
            None
        }
    }
}
