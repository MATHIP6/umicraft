// SPDX-License-Identifier: MIT OR Apache-2.0

//! A [`ScreenSpec`] placed in a viewport.

use crate::geom::Rect;

use super::decoration::{Decoration, SpriteRect};
use super::scale::gui_scale;
use super::{ScreenKind, ScreenSpec};

/// Item area edge, in unscaled pixels.
const ITEM_SIZE: f32 = 16.0;

/// A screen scaled and centred like vanilla `GuiContainer`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenLayout {
    /// The static description this layout was built from.
    pub spec: ScreenSpec,
    /// The auto GUI scale factor.
    pub scale: u32,
    /// The visible texture area in physical pixels.
    pub background: Rect,
    /// `(window slot index, 16x16 item area scaled)` per slot, in window order.
    pub slots: Vec<(usize, Rect)>,
}

impl ScreenLayout {
    /// Places `kind` in `viewport`.
    ///
    /// The scale is the vanilla auto scale of the viewport size and the GUI
    /// is centred (`guiLeft = (width - xSize) / 2`).
    #[must_use]
    pub fn new(viewport: Rect, kind: ScreenKind) -> Self {
        let spec = kind.spec();
        let scale = gui_scale(viewport.width as u32, viewport.height as u32);
        let s = scale as f32;
        let (w, h) = (spec.size.0 as f32 * s, spec.size.1 as f32 * s);
        let background = Rect::new(
            viewport.x + ((viewport.width - w) * 0.5).floor(),
            viewport.y + ((viewport.height - h) * 0.5).floor(),
            w,
            h,
        );
        let slots = spec
            .slots
            .iter()
            .map(|slot| {
                let rect = Rect::new(
                    background.x + slot.x as f32 * s,
                    background.y + slot.y as f32 * s,
                    ITEM_SIZE * s,
                    ITEM_SIZE * s,
                );
                (slot.index, rect)
            })
            .collect();
        Self {
            spec,
            scale,
            background,
            slots,
        }
    }

    /// The window slot under `point`, if any.
    ///
    /// Like vanilla `isMouseOverSlot` the hit area is the item area plus a
    /// one-pixel (scaled) border, so adjacent slots leave no dead gap.
    #[must_use]
    pub fn slot_at(&self, point: (f32, f32)) -> Option<usize> {
        let border = self.scale as f32;
        self.slots
            .iter()
            .find(|(_, rect)| {
                point.0 >= rect.x - border
                    && point.0 < rect.x + rect.width + border
                    && point.1 >= rect.y - border
                    && point.1 < rect.y + rect.height + border
            })
            .map(|(index, _)| *index)
    }

    /// Whether `point` is inside the background. A click outside sends slot
    /// -999 to the server (vanilla `GuiContainer.mouseClicked`, `isOutside`).
    #[must_use]
    pub fn point_inside(&self, point: (f32, f32)) -> bool {
        let bg = self.background;
        point.0 >= bg.x
            && point.0 < bg.x + bg.width
            && point.1 >= bg.y
            && point.1 < bg.y + bg.height
    }

    /// The background pieces to draw, with scaled destinations.
    #[must_use]
    pub fn background_sprites(&self) -> Vec<SpriteRect> {
        self.sprites(&self.spec.background)
    }

    /// Scaled sprite rectangles for dynamic `decorations`.
    #[must_use]
    pub fn sprites(&self, decorations: &[Decoration]) -> Vec<SpriteRect> {
        let origin = (self.background.x, self.background.y);
        decorations
            .iter()
            .filter_map(|d| d.sprite_rect(origin, self.scale))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::new(0.0, 0.0, 1280.0, 720.0)
    }

    #[test]
    fn is_centred_and_scaled() {
        let layout = ScreenLayout::new(viewport(), ScreenKind::Player);
        assert_eq!(layout.scale, 3);
        assert_eq!(layout.background, Rect::new(376.0, 111.0, 528.0, 498.0));
    }

    #[test]
    fn slot_rects_are_scaled_item_areas() {
        let layout = ScreenLayout::new(viewport(), ScreenKind::Player);
        assert_eq!(
            layout.slots[0],
            (0, Rect::new(376.0 + 432.0, 111.0 + 108.0, 48.0, 48.0))
        );
    }

    #[test]
    fn slots_sit_inside_the_background() {
        for kind in [
            ScreenKind::Player,
            ScreenKind::Chest { rows: 6 },
            ScreenKind::Hopper,
        ] {
            let layout = ScreenLayout::new(viewport(), kind);
            let bg = layout.background;
            for (i, r) in &layout.slots {
                assert!(r.x >= bg.x && r.y >= bg.y, "{kind:?} {i}");
                assert!(r.x + r.width <= bg.x + bg.width, "{kind:?} {i}");
                assert!(r.y + r.height <= bg.y + bg.height, "{kind:?} {i}");
            }
        }
    }

    #[test]
    fn slot_at_hits_the_window_index() {
        let layout = ScreenLayout::new(viewport(), ScreenKind::Furnace);
        for (index, rect) in layout.slots.clone() {
            let centre = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
            assert_eq!(layout.slot_at(centre), Some(index));
        }
        let (_, out) = layout.slots[2];
        assert_eq!(layout.slot_at((out.x - 4.0, out.y - 4.0)), None);
        assert_eq!(layout.slot_at((out.x - 1.0, out.y)), Some(2));
    }

    #[test]
    fn point_inside_is_half_open_on_the_background() {
        let layout = ScreenLayout::new(viewport(), ScreenKind::Hopper);
        let bg = layout.background;
        assert!(layout.point_inside((bg.x, bg.y)));
        assert!(!layout.point_inside((bg.x - 1.0, bg.y)));
        assert!(!layout.point_inside((bg.x + bg.width, bg.y)));
        assert!(!layout.point_inside((bg.x, bg.y + bg.height)));
    }

    #[test]
    fn chest_background_sprites_are_contiguous() {
        let layout = ScreenLayout::new(viewport(), ScreenKind::Chest { rows: 3 });
        let [top, bottom] = layout.background_sprites()[..] else {
            panic!("expected two sprites");
        };
        assert_eq!(top.src, (0, 0, 176, 71));
        assert_eq!(bottom.src, (0, 126, 176, 96));
        assert_eq!(top.dst.y + top.dst.height, bottom.dst.y);
    }

    #[test]
    fn furnace_decorations_are_placed_relative_to_the_gui() {
        let layout = ScreenLayout::new(viewport(), ScreenKind::Furnace);
        let deco = ScreenKind::Furnace.decorations(&[100, 100, 0, 200]);
        let sprites = layout.sprites(&deco);
        assert_eq!(sprites.len(), 2);
        assert_eq!(sprites[0].dst.x, layout.background.x + 56.0 * 3.0);
    }
}
