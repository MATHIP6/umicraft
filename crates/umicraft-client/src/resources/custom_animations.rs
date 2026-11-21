// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom texture animations of the packs (OptiFine `TextureAnimations`,
//! ADR-0088/0089): `anim/**/*.properties` files that play a picture of stacked
//! frames into a rectangle of a block texture.
//!
//! Only block-atlas sprites can be targeted: the block array holds one layer
//! per sprite, so a frame is composited into a CPU copy of the target's layer
//! and the whole layer is uploaded, the way the vanilla animations are. Other
//! targets (entity and GUI textures) are skipped with a warning.
//!
//! The pictures are read from the whole packs, not from the kept files of
//! [`GameFiles`]: block textures are not among those.

use umicraft_assets::skin::decode_png_bytes;
use umicraft_assets::ResourcePack;
use umicraft_packext::anim::{AnimSpec, FrameClock};
use umicraft_packext::Properties;

use super::game_files::GameFiles;

/// A rectangle of a layer, in layer pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// One custom animation: the layer it plays into and its frames.
#[derive(Debug, Clone)]
pub struct CustomAnimation {
    layer: u32,
    layer_width: u32,
    /// The layer as it shows now (RGBA8), updated in place on every step.
    pixels: Vec<u8>,
    rect: Rect,
    /// Each frame of the source scaled to `rect`, RGBA8.
    frames: Vec<Vec<u8>>,
    clock: FrameClock,
    /// The first frame is drawn on the first tick, before the clock runs.
    fresh: bool,
}

impl CustomAnimation {
    /// The atlas layer this animation writes.
    #[must_use]
    pub fn layer(&self) -> u32 {
        self.layer
    }

    /// Advances one game tick. Returns the new pixels of the whole layer when
    /// they changed: on the first call and whenever a step ends.
    pub fn tick(&mut self) -> Option<&[u8]> {
        let frame = if self.fresh {
            self.fresh = false;
            self.clock.current_frame()
        } else {
            self.clock.tick()?
        };
        self.blit(frame);
        Some(&self.pixels)
    }

    /// Copies `frame` into the rectangle; a frame the picture lacks leaves the
    /// layer as it is.
    fn blit(&mut self, frame: u32) {
        let Some(source) = self.frames.get(frame as usize) else {
            return;
        };
        let row_bytes = self.rect.width as usize * 4;
        for (row, source_row) in source.chunks_exact(row_bytes).enumerate() {
            let start = ((self.rect.y as usize + row) * self.layer_width as usize
                + self.rect.x as usize)
                * 4;
            self.pixels[start..start + row_bytes].copy_from_slice(source_row);
        }
    }
}

/// `value` scaled from a picture of `from` pixels to one of `to`.
fn scale(value: u32, from: u32, to: u32) -> u32 {
    (u64::from(value) * u64::from(to) / u64::from(from)) as u32
}

/// The rectangle of `spec` in a layer of `layer` pixels, when the target
/// picture of `target` pixels contains it.
fn layer_rect(spec: &AnimSpec, target: (u32, u32), layer: (u32, u32)) -> Option<Rect> {
    let fits =
        spec.x.checked_add(spec.width)? <= target.0 && spec.y.checked_add(spec.height)? <= target.1;
    if !fits {
        return None;
    }
    let x0 = scale(spec.x, target.0, layer.0);
    let y0 = scale(spec.y, target.1, layer.1);
    let x1 = scale(spec.x + spec.width, target.0, layer.0);
    let y1 = scale(spec.y + spec.height, target.1, layer.1);
    (x1 > x0 && y1 > y0).then_some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// A picture of `from` pixels resampled to `to` pixels, nearest neighbour.
fn resample(rgba: &[u8], from: (u32, u32), to: (u32, u32)) -> Vec<u8> {
    let mut out = Vec::with_capacity(to.0 as usize * to.1 as usize * 4);
    for y in 0..to.1 {
        let source_y = scale(y, to.1, from.1);
        for x in 0..to.0 {
            let start = (source_y as usize * from.0 as usize + scale(x, to.0, from.0) as usize) * 4;
            out.extend_from_slice(&rgba[start..start + 4]);
        }
    }
    out
}

/// Every frame of `source` (`spec.width` wide, `spec.height` rows per frame)
/// resampled to `rect`.
fn scaled_frames(source: &[u8], spec: &AnimSpec, rect: Rect, frames: u32) -> Vec<Vec<u8>> {
    let frame_bytes = spec.width as usize * spec.height as usize * 4;
    source
        .chunks_exact(frame_bytes)
        .take(frames as usize)
        .map(|frame| resample(frame, (spec.width, spec.height), (rect.width, rect.height)))
        .collect()
}

/// Where the pictures come from: the packs, highest priority first, then the
/// verified jar.
struct Sources<'a> {
    files: &'a GameFiles,
    packs: &'a [ResourcePack],
}

impl Sources<'_> {
    /// The PNG at the full virtual `path`, decoded to `(rgba, width, height)`.
    fn png(&self, path: &str) -> Option<(Vec<u8>, u32, u32)> {
        let bytes = self
            .packs
            .iter()
            .rev()
            .find_map(|pack| pack.get(path).map(std::borrow::Cow::Borrowed))
            .or_else(|| self.files.read(path))?;
        decode_png_bytes(&bytes).ok()
    }
}

/// Builds the animation of one properties file, or says why it cannot.
fn build(
    sources: &Sources<'_>,
    path: &str,
    bytes: &[u8],
    layer_of: &dyn Fn(&str) -> Option<u32>,
    layer_size: (u32, u32),
) -> Result<CustomAnimation, String> {
    let props = Properties::parse(bytes).map_err(|error| error.to_string())?;
    let spec = AnimSpec::parse(&props, path).map_err(|error| error.to_string())?;
    let key = sprite_key(&spec.to).ok_or("the target is not a block texture")?;
    let layer = layer_of(&key).ok_or("the target is not a sprite of the block atlas")?;
    let (source, source_width, source_height) = sources
        .png(&spec.from)
        .ok_or_else(|| format!("source picture {} not found", spec.from))?;
    if source_width != spec.width {
        return Err(format!(
            "source is {source_width} pixels wide, the frames {}",
            spec.width
        ));
    }
    let steps = spec
        .schedule(source_height)
        .map_err(|error| error.to_string())?;
    let (target, target_width, target_height) = sources
        .png(&spec.to)
        .ok_or_else(|| format!("target picture {} not found", spec.to))?;
    let rect = layer_rect(&spec, (target_width, target_height), layer_size)
        .ok_or("the rectangle is outside the target texture")?;
    Ok(CustomAnimation {
        layer,
        layer_width: layer_size.0,
        pixels: resample(&target, (target_width, target_height), layer_size),
        rect,
        frames: scaled_frames(&source, &spec, rect, source_height / spec.height),
        clock: FrameClock::new(steps),
        fresh: true,
    })
}

/// The atlas key of a target path: `assets/minecraft/textures/blocks/x.png`
/// is the sprite `blocks/x`. `None` for anything outside `textures/blocks/`.
fn sprite_key(target: &str) -> Option<String> {
    let rest = target.strip_prefix("assets/minecraft/textures/")?;
    let key = rest.strip_suffix(".png")?;
    key.starts_with("blocks/").then(|| key.to_string())
}

/// The custom animations of the `packs` (ascending priority; `files` holds
/// their kept files over the jar) for a block array whose layers are
/// `layer_size` pixels and which `layer_of` indexes by sprite key. A file that
/// cannot be used is logged and skipped.
#[must_use]
pub fn load(
    files: &GameFiles,
    packs: &[ResourcePack],
    layer_of: &dyn Fn(&str) -> Option<u32>,
    layer_size: (u32, u32),
) -> Vec<CustomAnimation> {
    let sources = Sources { files, packs };
    files
        .packs()
        .list_feature("anim/")
        .into_iter()
        .filter(|(relative, _)| relative.ends_with(".properties"))
        .filter_map(|(_, file)| {
            build(&sources, file.path, file.bytes, layer_of, layer_size)
                .map_err(|reason| {
                    tracing::warn!(path = %file.path, %reason, "skipping a custom animation");
                })
                .ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use umicraft_assets::{PackFiles, ResourcePack};

    use super::*;

    /// An RGBA PNG of `width` x `height` whose pixels come from `color`.
    fn png(width: u32, height: u32, color: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&color(x, y));
            }
        }
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(&pixels).expect("pixels");
        drop(writer);
        bytes
    }

    /// One pack holding `entries`, and the files kept from it.
    fn pack_of(entries: Vec<(&str, Vec<u8>)>) -> (GameFiles, Vec<ResourcePack>) {
        let map: BTreeMap<String, Vec<u8>> = entries
            .into_iter()
            .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes))
            .collect();
        let packs = vec![ResourcePack::from_parts("test", None, None, map)];
        (
            GameFiles::new(PackFiles::from_pack_refs(&packs), None),
            packs,
        )
    }

    fn load_from(
        pack: &(GameFiles, Vec<ResourcePack>),
        layer_size: (u32, u32),
    ) -> Vec<CustomAnimation> {
        load(&pack.0, &pack.1, &layer_of, layer_size)
    }

    fn layer_of(key: &str) -> Option<u32> {
        (key == "blocks/stone").then_some(7)
    }

    const PROPS: &[u8] = b"from=./frames.png\nto=textures/blocks/stone.png\n\
        x=0\ny=0\nw=2\nh=2\nduration=2\n";

    /// A 4x4 grey stone with two 2x2 frames, red then green, stacked.
    fn stone_files(properties: &[u8]) -> (GameFiles, Vec<ResourcePack>) {
        let frames = png(2, 4, |_, y| {
            if y < 2 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            }
        });
        pack_of(vec![
            ("optifine/anim/stone.properties", properties.to_vec()),
            ("optifine/anim/frames.png", frames),
            (
                "textures/blocks/stone.png",
                png(4, 4, |_, _| [9, 9, 9, 255]),
            ),
        ])
    }

    fn pixel(layer: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * width + x) * 4) as usize;
        layer[start..start + 4].try_into().expect("four bytes")
    }

    #[test]
    fn the_first_tick_draws_frame_zero_into_the_rectangle_only() {
        let mut loaded = load_from(&stone_files(PROPS), (4, 4));
        assert_eq!(loaded.len(), 1);
        let animation = &mut loaded[0];
        assert_eq!(animation.layer(), 7);
        let layer = animation.tick().expect("first picture").to_vec();
        assert_eq!(pixel(&layer, 4, 1, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(&layer, 4, 2, 0), [9, 9, 9, 255]);
        assert_eq!(pixel(&layer, 4, 0, 3), [9, 9, 9, 255]);
    }

    #[test]
    fn a_step_lasts_its_duration_then_the_next_frame_shows() {
        let mut loaded = load_from(&stone_files(PROPS), (4, 4));
        let animation = &mut loaded[0];
        animation.tick();
        assert!(animation.tick().is_none());
        let layer = animation.tick().expect("step ended").to_vec();
        assert_eq!(pixel(&layer, 4, 0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn a_bigger_layer_scales_the_rectangle_and_the_picture() {
        let mut loaded = load_from(&stone_files(PROPS), (8, 8));
        let layer = loaded[0].tick().expect("picture").to_vec();
        assert_eq!(layer.len(), 8 * 8 * 4);
        assert_eq!(pixel(&layer, 8, 3, 3), [255, 0, 0, 255]);
        assert_eq!(pixel(&layer, 8, 4, 0), [9, 9, 9, 255]);
    }

    #[test]
    fn unusable_files_are_skipped() {
        let entity = b"from=./frames.png\nto=textures/entity/pig/pig.png\nx=0\ny=0\nw=2\nh=2\n";
        assert!(load_from(&stone_files(entity), (4, 4)).is_empty());
        let outside = b"from=./frames.png\nto=textures/blocks/stone.png\nx=3\ny=0\nw=2\nh=2\n";
        assert!(load_from(&stone_files(outside), (4, 4)).is_empty());
        let wide = b"from=./frames.png\nto=textures/blocks/stone.png\nx=0\ny=0\nw=4\nh=2\n";
        assert!(load_from(&stone_files(wide), (4, 4)).is_empty());
        let unknown = b"from=./frames.png\nto=textures/blocks/dirt.png\nx=0\ny=0\nw=2\nh=2\n";
        assert!(load_from(&stone_files(unknown), (4, 4)).is_empty());
    }

    #[test]
    fn the_animated_textures_switch_freezes_the_custom_animations() {
        use crate::resources::texture_animator::{AnimationFilter, TextureAnimator};
        use std::time::Duration;

        let run = |filter: AnimationFilter| {
            let custom = load_from(&stone_files(PROPS), (4, 4));
            let mut animator = TextureAnimator::default().with_custom(custom);
            animator.set_filter(filter);
            let mut writes = Vec::new();
            animator.advance(Duration::from_millis(50), |_| true, |l, _| writes.push(l));
            writes
        };
        assert_eq!(run(AnimationFilter::default()), [7]);
        let off = AnimationFilter {
            custom: false,
            ..AnimationFilter::default()
        };
        assert!(run(off).is_empty());
    }

    #[test]
    fn sprite_keys_only_for_block_textures() {
        assert_eq!(
            sprite_key("assets/minecraft/textures/blocks/a/b.png").as_deref(),
            Some("blocks/a/b")
        );
        assert_eq!(sprite_key("assets/minecraft/textures/gui/icons.png"), None);
        assert_eq!(sprite_key("assets/minecraft/textures/blocks/x.jpg"), None);
    }
}
