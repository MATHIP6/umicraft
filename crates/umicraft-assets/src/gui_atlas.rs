// SPDX-License-Identifier: MIT OR Apache-2.0

//! RGBA atlas of GUI textures decoded from the client jar.
//!
//! The atlas packs every `textures/gui/*.png` and `textures/gui/container/*.png`
//! into a single RGBA8 image so the renderer needs only one GPU texture and one
//! bind group for all GUI backgrounds. Each named region stores its normalised
//! UV rectangle and its original pixel size, which the inventory layout uses to
//! position slots at the correct offsets.

use std::collections::BTreeMap;

/// One named region inside the [`GuiAtlas`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiRegion {
    /// Normalised UV of the top-left corner.
    pub u_min: f32,
    /// Normalised UV of the top-left corner.
    pub v_min: f32,
    /// Normalised UV of the bottom-right corner.
    pub u_max: f32,
    /// Normalised UV of the bottom-right corner.
    pub v_max: f32,
    /// Original pixel width of this texture.
    pub pixel_width: u32,
    /// Original pixel height of this texture.
    pub pixel_height: u32,
}

impl GuiRegion {
    /// Returns the `[u_min, v_min, u_max, v_max]` array for the draw list.
    #[must_use]
    pub fn src(&self) -> [f32; 4] {
        [self.u_min, self.v_min, self.u_max, self.v_max]
    }

    /// Returns the UV rect for a sub-region defined in original pixel coords.
    ///
    /// `x`, `y`, `w`, `h` are in the texture's own pixel space. This is used
    /// to sample individual slot sprites or sub-regions of a GUI background.
    #[must_use]
    pub fn sub_region(&self, x: u32, y: u32, w: u32, h: u32) -> [f32; 4] {
        let u_span = self.u_max - self.u_min;
        let v_span = self.v_max - self.v_min;
        let pw = self.pixel_width as f32;
        let ph = self.pixel_height as f32;
        [
            self.u_min + x as f32 / pw * u_span,
            self.v_min + y as f32 / ph * v_span,
            self.u_min + (x + w) as f32 / pw * u_span,
            self.v_min + (y + h) as f32 / ph * v_span,
        ]
    }
}

/// An RGBA8 atlas of GUI textures with named regions.
#[derive(Debug, Clone)]
pub struct GuiAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    regions: BTreeMap<String, GuiRegion>,
}

impl GuiAtlas {
    /// An empty atlas with no textures.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
            regions: BTreeMap::new(),
        }
    }

    /// Atlas width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Atlas height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The tightly packed RGBA8 pixels, row-major, top to bottom.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Looks up a named GUI texture region.
    ///
    /// Names are the jar-relative path without prefix or extension, e.g.
    /// `"container/inventory"`, `"container/generic_27"`, `"widgets"`.
    #[must_use]
    pub fn region(&self, name: &str) -> Option<&GuiRegion> {
        self.regions.get(name)
    }

    /// All named regions.
    #[must_use]
    pub fn regions(&self) -> &BTreeMap<String, GuiRegion> {
        &self.regions
    }

    /// True when the atlas has no textures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// A decoded GUI PNG before atlas packing.
pub(crate) struct GuiImage {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Packs decoded GUI images into a single RGBA8 atlas.
///
/// Uses a simple shelf packing: images are sorted tallest-first and placed
/// left to right in rows. The atlas width is the next power of two that fits
/// the widest image.
pub(crate) fn pack_gui_atlas(mut images: Vec<GuiImage>) -> GuiAtlas {
    if images.is_empty() {
        return GuiAtlas {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
            regions: BTreeMap::new(),
        };
    }

    images.sort_by(|a, b| b.height.cmp(&a.height).then(b.width.cmp(&a.width)));

    let max_width = images.iter().map(|i| i.width).max().unwrap_or(1);
    let atlas_width = max_width.next_power_of_two().max(256);

    let mut regions = BTreeMap::new();
    let mut cursor_x: u32 = 0;
    let mut cursor_y: u32 = 0;
    let mut row_height: u32 = 0;

    // First pass: compute positions.
    struct Placement {
        x: u32,
        y: u32,
    }
    let mut placements = Vec::with_capacity(images.len());

    for image in &images {
        if cursor_x + image.width > atlas_width {
            cursor_y += row_height;
            cursor_x = 0;
            row_height = 0;
        }
        placements.push(Placement {
            x: cursor_x,
            y: cursor_y,
        });
        row_height = row_height.max(image.height);
        cursor_x += image.width;
    }

    let atlas_height = (cursor_y + row_height).next_power_of_two().max(1);
    let pixel_count = (atlas_width as usize)
        .checked_mul(atlas_height as usize)
        .and_then(|n| n.checked_mul(4))
        .unwrap_or(0);
    let mut pixels = vec![0_u8; pixel_count];

    // Second pass: blit and record regions.
    for (image, placement) in images.iter().zip(&placements) {
        let dst_stride = atlas_width as usize * 4;
        let src_stride = image.width as usize * 4;
        for row in 0..image.height as usize {
            let dst_offset = (placement.y as usize + row) * dst_stride + placement.x as usize * 4;
            let src_offset = row * src_stride;
            if let (Some(dst), Some(src)) = (
                pixels.get_mut(dst_offset..dst_offset + src_stride),
                image.pixels.get(src_offset..src_offset + src_stride),
            ) {
                dst.copy_from_slice(src);
            }
        }

        let aw = atlas_width as f32;
        let ah = atlas_height as f32;
        regions.insert(
            image.name.clone(),
            GuiRegion {
                u_min: placement.x as f32 / aw,
                v_min: placement.y as f32 / ah,
                u_max: (placement.x + image.width) as f32 / aw,
                v_max: (placement.y + image.height) as f32 / ah,
                pixel_width: image.width,
                pixel_height: image.height,
            },
        );
    }

    GuiAtlas {
        width: atlas_width,
        height: atlas_height,
        pixels,
        regions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(name: &str, width: u32, height: u32) -> GuiImage {
        GuiImage {
            name: name.to_string(),
            width,
            height,
            pixels: vec![255; (width * height * 4) as usize],
        }
    }

    #[test]
    fn empty_input_produces_a_1x1_atlas() {
        let atlas = pack_gui_atlas(Vec::new());
        assert!(atlas.is_empty());
        assert_eq!(atlas.width(), 1);
        assert_eq!(atlas.height(), 1);
    }

    #[test]
    fn a_single_image_fills_its_region() {
        let atlas = pack_gui_atlas(vec![test_image("test", 176, 166)]);
        assert_eq!(atlas.regions().len(), 1);
        let region = atlas.region("test").unwrap();
        assert_eq!(region.pixel_width, 176);
        assert_eq!(region.pixel_height, 166);
        assert!(region.u_min >= 0.0);
        assert!(region.v_min >= 0.0);
        assert!(region.u_max <= 1.0);
        assert!(region.v_max <= 1.0);
    }

    #[test]
    fn multiple_images_pack_without_overlap() {
        let images = vec![
            test_image("a", 176, 166),
            test_image("b", 176, 166),
            test_image("c", 176, 222),
        ];
        let atlas = pack_gui_atlas(images);
        assert_eq!(atlas.regions().len(), 3);

        let regions: Vec<_> = atlas.regions().values().collect();
        for (i, a) in regions.iter().enumerate() {
            for b in &regions[i + 1..] {
                let overlap_x = a.u_min < b.u_max && a.u_max > b.u_min;
                let overlap_y = a.v_min < b.v_max && a.v_max > b.v_min;
                assert!(!(overlap_x && overlap_y), "regions overlap: {a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn sub_region_maps_correctly() {
        let region = GuiRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.5,
            v_max: 0.5,
            pixel_width: 256,
            pixel_height: 256,
        };
        let sub = region.sub_region(128, 128, 128, 128);
        assert!((sub[0] - 0.25).abs() < 1e-5);
        assert!((sub[1] - 0.25).abs() < 1e-5);
        assert!((sub[2] - 0.5).abs() < 1e-5);
        assert!((sub[3] - 0.5).abs() < 1e-5);
    }
}
