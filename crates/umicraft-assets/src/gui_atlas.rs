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
    /// Rows of the packed jar textures; [`GuiAtlas::with_dynamic`] appends
    /// below them.
    static_height: u32,
}

/// The prefix of the region names that [`GuiAtlas::with_dynamic`] adds.
pub const DYNAMIC_PREFIX: &str = "dynamic/";

/// A runtime image (a server favicon) to append to a [`GuiAtlas`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicImage {
    /// The region name without [`DYNAMIC_PREFIX`].
    pub name: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Tightly packed RGBA8 pixels, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
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
            static_height: 1,
        }
    }

    /// A copy of this atlas with `images` appended as regions named
    /// `dynamic/<name>`, for textures that only exist at runtime.
    ///
    /// The jar textures keep their pixels (their coordinates are
    /// renormalised for the larger atlas, which is widened when an image is
    /// wider than it), earlier dynamic images are dropped, so calling this
    /// again with the current set is how a server favicon is replaced or
    /// removed. Images are shelf-packed below the jar part; one with the
    /// wrong pixel count is skipped.
    #[must_use]
    pub fn with_dynamic(&self, images: &[DynamicImage]) -> Self {
        let usable: Vec<&DynamicImage> = images
            .iter()
            .filter(|image| {
                image.width > 0
                    && image.pixels.len() == image.width as usize * image.height as usize * 4
            })
            .collect();
        let width = usable
            .iter()
            .map(|image| image.width)
            .fold(self.width, u32::max);
        let (mut x, mut y, mut row) = (0_u32, 0_u32, 0_u32);
        let mut places = Vec::with_capacity(usable.len());
        for image in &usable {
            if x + image.width > width {
                y += row;
                (x, row) = (0, 0);
            }
            places.push((x, y));
            x += image.width;
            row = row.max(image.height);
        }
        let height = self.static_height + y + row;
        let stride = width as usize * 4;
        let mut pixels = vec![0_u8; height as usize * stride];
        let old_stride = self.width as usize * 4;
        for (line, source) in self.pixels[..self.static_height as usize * old_stride]
            .chunks_exact(old_stride)
            .enumerate()
        {
            pixels[line * stride..line * stride + old_stride].copy_from_slice(source);
        }
        let rescale = [
            self.width as f32 / width as f32,
            self.height as f32 / height as f32,
        ];
        let mut regions: BTreeMap<String, GuiRegion> = self
            .regions
            .iter()
            .filter(|(name, _)| !name.starts_with(DYNAMIC_PREFIX))
            .map(|(name, region)| {
                let mut region = *region;
                region.u_min *= rescale[0];
                region.u_max *= rescale[0];
                region.v_min *= rescale[1];
                region.v_max *= rescale[1];
                (name.clone(), region)
            })
            .collect();
        for (image, (left, top)) in usable.iter().zip(places) {
            let top = self.static_height + top;
            for (line, source) in image
                .pixels
                .chunks_exact(image.width as usize * 4)
                .enumerate()
            {
                let start = (top as usize + line) * stride + left as usize * 4;
                pixels[start..start + source.len()].copy_from_slice(source);
            }
            regions.insert(
                format!("{DYNAMIC_PREFIX}{}", image.name),
                GuiRegion {
                    u_min: left as f32 / width as f32,
                    v_min: top as f32 / height as f32,
                    u_max: (left + image.width) as f32 / width as f32,
                    v_max: (top + image.height) as f32 / height as f32,
                    pixel_width: image.width,
                    pixel_height: image.height,
                },
            );
        }
        Self {
            width,
            height,
            pixels,
            regions,
            static_height: self.static_height,
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
            static_height: 1,
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
        static_height: atlas_height,
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

    fn dynamic(name: &str, side: u32, value: u8) -> DynamicImage {
        DynamicImage {
            name: name.to_string(),
            width: side,
            height: side,
            pixels: vec![value; (side * side * 4) as usize],
        }
    }

    #[test]
    fn dynamic_images_are_appended_below_the_jar_textures() {
        let base = pack_gui_atlas(vec![test_image("icons", 256, 256)]);
        let grown = base.with_dynamic(&[dynamic("a", 64, 7), dynamic("b", 64, 9)]);
        assert_eq!(grown.width(), base.width());
        assert_eq!(grown.height(), base.height() + 64);
        let a = grown.region("dynamic/a").expect("a");
        let b = grown.region("dynamic/b").expect("b");
        assert_eq!((a.pixel_width, a.pixel_height), (64, 64));
        assert!(b.u_min >= a.u_max - 1e-6, "same shelf, to the right");
        let first = (base.height() as usize * base.width() as usize) * 4;
        assert_eq!(grown.pixels()[first], 7);
        // The jar region keeps its pixels, renormalised for the taller atlas.
        let icons = grown.region("icons").expect("icons");
        let old = base.region("icons").expect("icons");
        assert!(
            (icons.v_max * grown.height() as f32 - old.v_max * base.height() as f32).abs() < 1e-3
        );
    }

    #[test]
    fn a_new_dynamic_set_replaces_the_old_one() {
        let base = pack_gui_atlas(vec![test_image("icons", 256, 256)]);
        let once = base.with_dynamic(&[dynamic("a", 64, 7)]);
        let twice = once.with_dynamic(&[dynamic("b", 64, 9)]);
        assert!(twice.region("dynamic/a").is_none());
        assert!(twice.region("dynamic/b").is_some());
        assert_eq!(twice.height(), once.height());
        let none = twice.with_dynamic(&[]);
        assert_eq!(none.height(), base.height());
        assert_eq!(none.pixels().len(), base.pixels().len());
    }

    #[test]
    fn a_wider_image_widens_the_atlas_and_keeps_the_jar_regions() {
        let base = pack_gui_atlas(vec![test_image("icons", 256, 256)]);
        let grown = base.with_dynamic(&[dynamic("wide", 512, 3)]);
        assert_eq!(grown.width(), 512);
        let icons = grown.region("icons").expect("icons");
        let old = base.region("icons").expect("icons");
        assert!((icons.u_max * 512.0 - old.u_max * 256.0).abs() < 1e-3);
        // The old rows moved into the wider stride unchanged.
        assert_eq!(grown.pixels()[0], base.pixels()[0]);
        assert_eq!(grown.pixels()[(256 * 512) * 4], 3);
    }

    #[test]
    fn bad_dynamic_images_are_skipped() {
        let base = pack_gui_atlas(vec![test_image("icons", 256, 256)]);
        let short = DynamicImage {
            pixels: vec![0; 3],
            ..dynamic("short", 8, 0)
        };
        let grown = base.with_dynamic(&[short]);
        assert!(grown
            .regions()
            .keys()
            .all(|name| !name.starts_with("dynamic/")));
    }
}
