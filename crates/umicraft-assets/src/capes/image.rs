// SPDX-License-Identifier: MIT OR Apache-2.0

//! Decoding a downloaded cape into RGBA frames the renderer can upload.
//!
//! Capes from the services come in three shapes: the classic 64x32 image, a
//! smaller or larger one (LabyMod's is 22x17, HD capes are `64n x 32n`), and
//! an animated strip of `w x w/2` frames stacked downward. Static images are
//! padded into the smallest `64*2^n x 32*2^n` canvas so the cape model's
//! normalised UVs stay valid at any resolution (ADR-0081; `docs/sources.md`,
//! section "Capes").

use super::error::CapeError;
use super::provider::CapeProvider;

/// Largest width or height accepted, in pixels.
const MAX_DIMENSION: u32 = 2048;
/// Most frames an animated cape may have.
const MAX_FRAMES: u32 = 64;
/// The first four pixels (ABGR) of LabyMod's "no cape" image.
const LABYMOD_PLACEHOLDER: [u32; 4] = [0xff30_5f81, 0xff30_5f90, 0xff30_5f90, 0xff30_5f90];

/// A decoded cape: one or more RGBA8 frames of the same size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapeImage {
    /// Width of every frame in pixels.
    pub width: u32,
    /// Height of every frame in pixels.
    pub height: u32,
    /// The frames, each `width * height * 4` bytes, top row first. A static
    /// cape has exactly one.
    pub frames: Vec<Vec<u8>>,
}

/// Decodes a cape PNG.
///
/// A static cape is padded (transparent, top-left aligned) into the smallest
/// `64*2^n x 32*2^n` image that holds it. An animated one is cut into frames
/// of `width x width/2` and left as it is.
///
/// Returns `Ok(None)` for LabyMod's placeholder image, which means "this
/// player has no cape".
///
/// # Errors
///
/// [`CapeError::Decode`] for a PNG that does not decode and
/// [`CapeError::BadSize`] for a size no cape can have.
pub fn decode_cape(
    png: &[u8],
    animated: bool,
    provider: CapeProvider,
) -> Result<Option<CapeImage>, CapeError> {
    let (rgba, width, height) = decode_rgba(png)?;
    if provider == CapeProvider::LabyMod && is_labymod_placeholder(&rgba) {
        return Ok(None);
    }
    let image = if animated {
        split_frames(&rgba, width, height)?
    } else {
        pad_static(&rgba, width, height)
    };
    Ok(Some(image))
}

/// Decodes any PNG color type to RGBA8, refusing a size outside
/// `1..=2048` *before* the pixels are allocated.
fn decode_rgba(png: &[u8]) -> Result<(Vec<u8>, u32, u32), CapeError> {
    let decode_error = |error: png::DecodingError| CapeError::Decode(error.to_string());
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    // EXPAND turns palette, tRNS and low bit depths into 8-bit samples (the
    // LabyMod cape is an 8-bit palette image); STRIP_16 drops 16-bit samples.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let (width, height) = (reader.info().width, reader.info().height);
    check_size(width, height)?;
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let frame = reader.next_frame(&mut buffer).map_err(decode_error)?;
    buffer.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => expand(&buffer, 3, |p| [p[0], p[1], p[2], 255]),
        png::ColorType::Grayscale => expand(&buffer, 1, |p| [p[0], p[0], p[0], 255]),
        png::ColorType::GrayscaleAlpha => expand(&buffer, 2, |p| [p[0], p[0], p[0], p[1]]),
        png::ColorType::Indexed => {
            return Err(CapeError::Decode("palette was not expanded".to_owned()))
        }
    };
    Ok((rgba, width, height))
}

fn expand(samples: &[u8], stride: usize, to_rgba: impl Fn(&[u8]) -> [u8; 4]) -> Vec<u8> {
    samples.chunks_exact(stride).flat_map(to_rgba).collect()
}

fn check_size(width: u32, height: u32) -> Result<(), CapeError> {
    if (1..=MAX_DIMENSION).contains(&width) && (1..=MAX_DIMENSION).contains(&height) {
        Ok(())
    } else {
        Err(CapeError::BadSize { width, height })
    }
}

/// True when the first four pixels, read as ABGR integers, are LabyMod's
/// "no cape" marker.
fn is_labymod_placeholder(rgba: &[u8]) -> bool {
    let mut pixels = rgba
        .chunks_exact(4)
        .map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]));
    LABYMOD_PLACEHOLDER
        .iter()
        .all(|expected| pixels.next() == Some(*expected))
}

/// Copies the image into the top-left corner of the smallest `64*2^n x
/// 32*2^n` transparent canvas that contains it.
fn pad_static(rgba: &[u8], width: u32, height: u32) -> CapeImage {
    let mut scale = 1;
    while 64 * scale < width || 32 * scale < height {
        scale *= 2;
    }
    let (padded_width, padded_height) = (64 * scale, 32 * scale);
    let mut frame = vec![0; padded_width as usize * padded_height as usize * 4];
    let (row, padded_row) = (width as usize * 4, padded_width as usize * 4);
    for (y, source_row) in rgba.chunks_exact(row).enumerate() {
        frame[y * padded_row..y * padded_row + row].copy_from_slice(source_row);
    }
    CapeImage {
        width: padded_width,
        height: padded_height,
        frames: vec![frame],
    }
}

/// Cuts a vertical strip into `width x width/2` frames; rows left over after
/// the last whole frame are dropped.
fn split_frames(rgba: &[u8], width: u32, height: u32) -> Result<CapeImage, CapeError> {
    let bad_size = CapeError::BadSize { width, height };
    if width < 2 || !width.is_multiple_of(2) {
        return Err(bad_size);
    }
    let frame_height = width / 2;
    let count = height / frame_height;
    if count == 0 || count > MAX_FRAMES {
        return Err(bad_size);
    }
    let frame_bytes = width as usize * frame_height as usize * 4;
    Ok(CapeImage {
        width,
        height: frame_height,
        frames: rgba
            .chunks_exact(frame_bytes)
            .take(count as usize)
            .map(<[u8]>::to_vec)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capes::test_support::{gray_png, palette_png_22x17, rgba_png, solid};

    const OPTIFINE: CapeProvider = CapeProvider::OptiFine;

    fn decode(png: &[u8], animated: bool) -> Result<Option<CapeImage>, CapeError> {
        decode_cape(png, animated, OPTIFINE)
    }

    fn pixel(image: &CapeImage, frame: usize, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        let data = &image.frames[frame];
        [data[at], data[at + 1], data[at + 2], data[at + 3]]
    }

    #[test]
    fn a_classic_cape_is_unchanged() {
        let png = rgba_png(64, 32, &solid(64, 32, [1, 2, 3, 255]));
        let image = decode(&png, false).unwrap().unwrap();
        assert_eq!((image.width, image.height, image.frames.len()), (64, 32, 1));
        assert_eq!(pixel(&image, 0, 63, 31), [1, 2, 3, 255]);
    }

    #[test]
    fn a_small_cape_is_padded_top_left_with_transparency() {
        let png = rgba_png(22, 17, &solid(22, 17, [9, 8, 7, 255]));
        let image = decode(&png, false).unwrap().unwrap();
        assert_eq!((image.width, image.height), (64, 32));
        assert_eq!(pixel(&image, 0, 0, 0), [9, 8, 7, 255]);
        assert_eq!(pixel(&image, 0, 21, 16), [9, 8, 7, 255]);
        assert_eq!(pixel(&image, 0, 22, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&image, 0, 0, 17), [0, 0, 0, 0]);
    }

    #[test]
    fn an_hd_cape_is_padded_to_the_next_power_of_two_canvas() {
        let png = rgba_png(92, 44, &solid(92, 44, [1, 1, 1, 255]));
        let image = decode(&png, false).unwrap().unwrap();
        assert_eq!((image.width, image.height), (128, 64));
        assert_eq!(pixel(&image, 0, 91, 43), [1, 1, 1, 255]);
        assert_eq!(pixel(&image, 0, 92, 43), [0, 0, 0, 0]);
    }

    #[test]
    fn the_canvas_grows_for_whichever_side_is_larger() {
        let tall = rgba_png(10, 33, &solid(10, 33, [1, 1, 1, 255]));
        let image = decode(&tall, false).unwrap().unwrap();
        assert_eq!((image.width, image.height), (128, 64));
        let exact = rgba_png(128, 64, &solid(128, 64, [1, 1, 1, 255]));
        let image = decode(&exact, false).unwrap().unwrap();
        assert_eq!((image.width, image.height), (128, 64));
    }

    #[test]
    fn a_one_pixel_cape_is_padded() {
        let png = rgba_png(1, 1, &[5, 5, 5, 255]);
        let image = decode(&png, false).unwrap().unwrap();
        assert_eq!((image.width, image.height), (64, 32));
    }

    #[test]
    fn sizes_outside_the_limits_are_refused() {
        for (w, h) in [(2049, 32), (64, 2049)] {
            let png = rgba_png(w, h, &solid(w, h, [0, 0, 0, 255]));
            assert_eq!(
                decode(&png, false),
                Err(CapeError::BadSize {
                    width: w,
                    height: h
                })
            );
        }
        let largest = rgba_png(2048, 1024, &solid(2048, 1024, [0, 0, 0, 255]));
        assert!(decode(&largest, false).unwrap().is_some());
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            decode(b"not a png", false),
            Err(CapeError::Decode(_))
        ));
        assert!(matches!(decode(&[], true), Err(CapeError::Decode(_))));
    }

    #[test]
    fn a_palette_png_decodes_to_its_colors() {
        let image = decode(&palette_png_22x17(), false).unwrap().unwrap();
        assert_eq!((image.width, image.height), (64, 32));
        assert_eq!(pixel(&image, 0, 0, 0), [10, 20, 30, 255]);
        assert_eq!(pixel(&image, 0, 1, 0), [200, 100, 50, 255]);
        assert_eq!(pixel(&image, 0, 21, 16), [200, 100, 50, 255]);
    }

    #[test]
    fn gray_and_rgb_images_get_an_opaque_alpha() {
        let gray = gray_png(64, 32, 77);
        let image = decode(&gray, false).unwrap().unwrap();
        assert_eq!(pixel(&image, 0, 3, 3), [77, 77, 77, 255]);
    }

    #[test]
    fn the_labymod_placeholder_is_no_cape() {
        let mut pixels = solid(64, 32, [0, 0, 0, 255]);
        // ABGR 0xff305f81: r 0x81, g 0x5f, b 0x30, a 0xff, then 0x90 three times.
        pixels[..16].copy_from_slice(&[
            0x81, 0x5f, 0x30, 0xff, 0x90, 0x5f, 0x30, 0xff, 0x90, 0x5f, 0x30, 0xff, 0x90, 0x5f,
            0x30, 0xff,
        ]);
        let png = rgba_png(64, 32, &pixels);
        assert_eq!(decode_cape(&png, false, CapeProvider::LabyMod), Ok(None));
        // Only LabyMod uses that marker.
        assert!(decode_cape(&png, false, CapeProvider::OptiFine)
            .unwrap()
            .is_some());
    }

    #[test]
    fn a_near_placeholder_is_a_cape() {
        let mut pixels = solid(64, 32, [0, 0, 0, 255]);
        pixels[..12].copy_from_slice(&[
            0x81, 0x5f, 0x30, 0xff, 0x90, 0x5f, 0x30, 0xff, 0x90, 0x5f, 0x30, 0xff,
        ]);
        let png = rgba_png(64, 32, &pixels);
        assert!(decode_cape(&png, false, CapeProvider::LabyMod)
            .unwrap()
            .is_some());
    }

    #[test]
    fn a_tiny_image_cannot_be_the_placeholder() {
        let png = rgba_png(1, 2, &[0x81, 0x5f, 0x30, 0xff, 0x90, 0x5f, 0x30, 0xff]);
        assert!(decode_cape(&png, false, CapeProvider::LabyMod)
            .unwrap()
            .is_some());
    }

    fn strip(width: u32, frames: u32) -> Vec<u8> {
        let frame_height = width / 2;
        let mut pixels = Vec::new();
        for frame in 0..frames {
            pixels.extend(solid(width, frame_height, [frame as u8, 0, 0, 255]));
        }
        rgba_png(width, frame_height * frames, &pixels)
    }

    #[test]
    fn an_animated_strip_is_cut_into_frames_without_padding() {
        let image = decode(&strip(22, 3), true).unwrap().unwrap();
        assert_eq!((image.width, image.height, image.frames.len()), (22, 11, 3));
        for frame in 0..3 {
            assert_eq!(pixel(&image, frame, 5, 5), [frame as u8, 0, 0, 255]);
        }
    }

    #[test]
    fn a_one_frame_strip_is_a_valid_animation() {
        let image = decode(&strip(64, 1), true).unwrap().unwrap();
        assert_eq!((image.width, image.height, image.frames.len()), (64, 32, 1));
    }

    #[test]
    fn a_partial_last_frame_is_dropped() {
        let png = rgba_png(20, 25, &solid(20, 25, [4, 4, 4, 255]));
        let image = decode(&png, true).unwrap().unwrap();
        assert_eq!((image.height, image.frames.len()), (10, 2));
    }

    #[test]
    fn a_strip_shorter_than_one_frame_is_refused() {
        let png = rgba_png(64, 31, &solid(64, 31, [0, 0, 0, 255]));
        assert_eq!(
            decode(&png, true),
            Err(CapeError::BadSize {
                width: 64,
                height: 31
            })
        );
    }

    #[test]
    fn an_odd_width_strip_is_refused() {
        let png = rgba_png(63, 64, &solid(63, 64, [0, 0, 0, 255]));
        assert!(matches!(decode(&png, true), Err(CapeError::BadSize { .. })));
        let one = rgba_png(1, 4, &solid(1, 4, [0, 0, 0, 255]));
        assert!(matches!(decode(&one, true), Err(CapeError::BadSize { .. })));
    }

    #[test]
    fn at_most_sixty_four_frames() {
        assert_eq!(
            decode(&strip(8, 64), true).unwrap().unwrap().frames.len(),
            64
        );
        assert!(matches!(
            decode(&strip(8, 65), true),
            Err(CapeError::BadSize { .. })
        ));
    }
}
