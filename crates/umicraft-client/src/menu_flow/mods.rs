// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods menu inside the menu flow: its blur and its frame. The design is
//! in `umicraft_ui::menus::mods`; the keys and the shortcut that open it are in
//! `window/mods_menu.rs`.

use umicraft_core::lang::Lang;
use umicraft_ui::menus::ModsOutcome;
use umicraft_ui::Ui;

use super::{MenuCommand, MenuContext, MenuFlow};
use crate::screen::Screen;

impl MenuFlow {
    /// How blurred the game behind the mods menu is, `0.0` to `1.0`: it fades
    /// in while the menu opens.
    #[must_use]
    pub fn mods_blur(&self) -> f32 {
        self.mods.blur_strength(self.partial_tick())
    }

    /// The mods menu over the game. Its Mods button leads nowhere yet.
    pub(super) fn show_mods(
        &mut self,
        ui: &mut Ui,
        lang: &Lang,
        context: &MenuContext<'_>,
    ) -> Option<MenuCommand> {
        let partial_tick = self.partial_tick();
        match self.mods.show(ui, context.brand_font, lang, partial_tick)? {
            ModsOutcome::Close => Some(MenuCommand::GoTo(Screen::None)),
            ModsOutcome::OpenMods => None,
        }
    }
}
