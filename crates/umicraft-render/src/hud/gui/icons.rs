// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `icons.png` layer: the [`Canvas`] the HUD sprites are placed on, the
//! order of the layer (crosshair, boss bar, status rows, bar above the
//! hotbar) and the boss bar.

use umicraft_ui::screens::{gui_scale_option, ScaledResolution};
use umicraft_ui::{DrawList, FontAtlas, Rect, SpriteSheet};

use super::crosshair::draw_crosshair;
use super::vitals::draw_vitals;
use super::xp::draw_bottom_bar;
use crate::hud::HudState;

/// A texture region in pixels: `(u, v, width, height)`.
pub(super) type Region = (u32, u32, u32, u32);

/// The draw list seen in GUI units, as the reference `Gui.drawTexturedModalRect`
/// sees the screen: integer positions, `scale` pixels per unit.
pub(super) struct Canvas<'a> {
    list: &'a mut DrawList,
    /// Viewport width in GUI units (`ScaledResolution.getScaledWidth`).
    pub(super) width: i32,
    /// Viewport height in GUI units.
    pub(super) height: i32,
    /// Physical pixels per GUI unit.
    pub(super) scale: f32,
}

impl<'a> Canvas<'a> {
    /// A canvas over `list` for a physical `viewport`.
    pub(super) fn new(list: &'a mut DrawList, viewport: (u32, u32)) -> Self {
        // The "GUI Scale" option caps the scale, as for the rest of the HUD
        // (`GuiIngame` builds its `ScaledResolution` from `gameSettings.guiScale`).
        let resolution = ScaledResolution::new(viewport.0, viewport.1, gui_scale_option());
        Self {
            list,
            width: resolution.scaled_width as i32,
            height: resolution.scaled_height as i32,
            scale: resolution.scale_factor as f32,
        }
    }

    /// The draw list, for text.
    pub(super) fn list(&mut self) -> &mut DrawList {
        self.list
    }

    /// Draws `region` of `sheet` with its top-left corner at `(x, y)`.
    pub(super) fn sprite(&mut self, sheet: &SpriteSheet, region: Region, x: i32, y: i32) {
        let (u, v, w, h) = region;
        let dst = Rect::new(
            x as f32 * self.scale,
            y as f32 * self.scale,
            w as f32 * self.scale,
            h as f32 * self.scale,
        );
        self.list.image(sheet.sub(u, v, w, h), dst, [1.0; 4]);
    }

    /// Like [`Canvas::sprite`] with the inverting blend of the crosshair.
    pub(super) fn sprite_inverted(&mut self, sheet: &SpriteSheet, region: Region, x: i32, y: i32) {
        let (u, v, w, h) = region;
        let dst = Rect::new(
            x as f32 * self.scale,
            y as f32 * self.scale,
            w as f32 * self.scale,
            h as f32 * self.scale,
        );
        self.list.image_inverted(sheet.sub(u, v, w, h), dst);
    }
}

/// Draws every `icons.png` element of the HUD.
pub(super) fn draw_icons_layer(
    list: &mut DrawList,
    state: &HudState,
    viewport: (u32, u32),
    font: &FontAtlas,
    icons: &SpriteSheet,
) {
    {
        let mut canvas = Canvas::new(list, viewport);
        if state.crosshair {
            draw_crosshair(&mut canvas, icons);
        }
        draw_bottom_bar(&mut canvas, state, font, icons);
        if let Some(vitals) = &state.vitals {
            draw_vitals(&mut canvas, vitals, icons);
        }
    }
}
