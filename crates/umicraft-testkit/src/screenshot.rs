// SPDX-License-Identifier: MIT OR Apache-2.0

//! Screenshot comparison for the parity harness.
//!
//! A Umicraft screenshot is compared against a reference taken by the owner
//! with the vanilla 1.8.8 client. Reference images contain Mojang assets and
//! must therefore stay **outside** the repository (spec §3); this module only
//! ever sees them as files passed at run time, and its tests use synthetic
//! images generated in code.
//!
//! Both images must have the same size: GUI scale and field of view depend on
//! the resolution, so screenshots have to be taken at the same one.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use png::{BitDepth, ColorType, Transformations};

/// Errors raised while loading, saving or comparing screenshots.
#[derive(Debug, thiserror::Error)]
pub enum ScreenshotError {
    /// A file could not be read or written.
    #[error("screenshot I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The PNG data could not be decoded.
    #[error("PNG decode error: {0}")]
    Decode(#[from] png::DecodingError),
    /// The PNG data could not be encoded.
    #[error("PNG encode error: {0}")]
    Encode(#[from] png::EncodingError),
    /// The two images do not have the same dimensions.
    #[error("size mismatch: reference {reference:?} vs candidate {candidate:?}")]
    SizeMismatch {
        /// `(width, height)` of the reference image.
        reference: (u32, u32),
        /// `(width, height)` of the candidate image.
        candidate: (u32, u32),
    },
    /// The PNG uses a color type this module cannot convert to RGBA8.
    #[error("unsupported PNG color type {0:?}")]
    UnsupportedColorType(ColorType),
    /// The RGBA buffer length does not match `width * height * 4`.
    #[error("RGBA buffer holds {actual} bytes, expected {expected}")]
    BadBuffer {
        /// Required byte count.
        expected: usize,
        /// Actual byte count.
        actual: usize,
    },
}

/// An axis-aligned pixel rectangle; `x`/`y` is the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Rect {
    /// Whether the pixel `(x, y)` lies inside the rectangle.
    #[must_use]
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x
            && y >= self.y
            && u64::from(x) < u64::from(self.x) + u64::from(self.width)
            && u64::from(y) < u64::from(self.y) + u64::from(self.height)
    }
}

/// An 8-bit RGBA image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA bytes, `width * height * 4` long.
    pub rgba: Vec<u8>,
}

impl Image {
    /// Creates an image filled with one RGBA color.
    #[must_use]
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        let pixels = width as usize * height as usize;
        Self {
            width,
            height,
            rgba: color.iter().copied().cycle().take(pixels * 4).collect(),
        }
    }

    /// Loads a PNG file and converts it to RGBA8.
    ///
    /// Grayscale, grayscale+alpha, RGB and RGBA are accepted at any bit
    /// depth; palettes are expanded.
    ///
    /// # Errors
    ///
    /// I/O and decoding failures, or [`ScreenshotError::UnsupportedColorType`]
    /// if the decoder yields a layout other than the ones above.
    pub fn load(path: &Path) -> Result<Self, ScreenshotError> {
        let mut decoder = png::Decoder::new(BufReader::new(File::open(path)?));
        decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buffer)?;
        let data = &buffer[..frame.buffer_size()];
        let rgba = match frame.color_type {
            ColorType::Rgba => data.to_vec(),
            ColorType::Rgb => data
                .chunks_exact(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            ColorType::GrayscaleAlpha => data
                .chunks_exact(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            other => return Err(ScreenshotError::UnsupportedColorType(other)),
        };
        Ok(Self {
            width: frame.width,
            height: frame.height,
            rgba,
        })
    }

    /// Saves the image as an RGBA8 PNG.
    ///
    /// # Errors
    ///
    /// [`ScreenshotError::BadBuffer`] if `rgba` has the wrong length, plus
    /// I/O and encoding failures.
    pub fn save(&self, path: &Path) -> Result<(), ScreenshotError> {
        let expected = self.width as usize * self.height as usize * 4;
        if self.rgba.len() != expected {
            return Err(ScreenshotError::BadBuffer {
                expected,
                actual: self.rgba.len(),
            });
        }
        let mut encoder =
            png::Encoder::new(BufWriter::new(File::create(path)?), self.width, self.height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.write_header()?.write_image_data(&self.rgba)?;
        Ok(())
    }
}

/// How two screenshots are compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOptions {
    /// A pixel counts as equal when every compared channel differs by at most
    /// this much. Absorbs dithering and driver rounding differences.
    pub channel_tolerance: u8,
    /// Ignore the alpha channel (screenshots are opaque in practice).
    pub ignore_alpha: bool,
    /// Regions excluded from the comparison, e.g. the F3 overlay or the chat.
    pub masks: Vec<Rect>,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            channel_tolerance: 8,
            ignore_alpha: true,
            masks: Vec::new(),
        }
    }
}

/// Acceptance limits for a [`DiffReport`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Threshold {
    /// Largest accepted share of differing pixels, in `0.0..=1.0`.
    pub max_differing_ratio: f64,
    /// Largest accepted mean absolute channel error, in `0.0..=255.0`.
    pub max_mean_abs_error: f64,
}

impl Threshold {
    /// Proposed in ADR-0019, pending owner approval (ROADMAP §8.1): at most
    /// 1 % differing pixels and a mean absolute error of at most 2.0.
    pub const PROPOSED: Self = Self {
        max_differing_ratio: 0.01,
        max_mean_abs_error: 2.0,
    };
}

/// Result of comparing two screenshots.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffReport {
    /// Pixels that were compared (not masked out).
    pub compared_pixels: u64,
    /// Compared pixels with a channel error above the tolerance.
    pub differing_pixels: u64,
    /// Mean per-channel absolute error over compared pixels, `0..=255`.
    pub mean_abs_error: f64,
    /// Largest per-channel absolute error seen.
    pub max_channel_error: u8,
    /// Smallest rectangle containing every differing pixel.
    pub bounding_box: Option<Rect>,
}

impl DiffReport {
    /// Share of compared pixels that differ; `0.0` when nothing was compared.
    #[must_use]
    pub fn differing_ratio(&self) -> f64 {
        if self.compared_pixels == 0 {
            0.0
        } else {
            self.differing_pixels as f64 / self.compared_pixels as f64
        }
    }

    /// Whether the report satisfies `threshold`.
    #[must_use]
    pub fn within(&self, threshold: &Threshold) -> bool {
        self.differing_ratio() <= threshold.max_differing_ratio
            && self.mean_abs_error <= threshold.max_mean_abs_error
    }
}

/// Compares `candidate` against `reference`.
///
/// Returns the report and a visualization: the reference in grey with the
/// differing pixels in red.
///
/// # Errors
///
/// [`ScreenshotError::SizeMismatch`] if the dimensions differ, or
/// [`ScreenshotError::BadBuffer`] if a pixel buffer has the wrong length.
pub fn compare(
    reference: &Image,
    candidate: &Image,
    options: &DiffOptions,
) -> Result<(DiffReport, Image), ScreenshotError> {
    if (reference.width, reference.height) != (candidate.width, candidate.height) {
        return Err(ScreenshotError::SizeMismatch {
            reference: (reference.width, reference.height),
            candidate: (candidate.width, candidate.height),
        });
    }
    let expected = reference.width as usize * reference.height as usize * 4;
    for image in [reference, candidate] {
        if image.rgba.len() != expected {
            return Err(ScreenshotError::BadBuffer {
                expected,
                actual: image.rgba.len(),
            });
        }
    }

    let channels = if options.ignore_alpha { 3 } else { 4 };
    let mut visual = vec![0_u8; expected];
    let mut compared = 0_u64;
    let mut differing = 0_u64;
    let mut error_sum = 0_u64;
    let mut max_error = 0_u8;
    let mut bounds: Option<(u32, u32, u32, u32)> = None;

    let pixels = reference
        .rgba
        .chunks_exact(4)
        .zip(candidate.rgba.chunks_exact(4))
        .zip(visual.chunks_exact_mut(4));
    for (index, ((expected_px, actual_px), out)) in pixels.enumerate() {
        let x = (index % reference.width as usize) as u32;
        let y = (index / reference.width as usize) as u32;
        let grey = grey_of(expected_px);
        out.copy_from_slice(&[grey, grey, grey, 255]);
        if options.masks.iter().any(|mask| mask.contains(x, y)) {
            continue;
        }
        compared += 1;
        let mut pixel_max = 0_u8;
        for channel in 0..channels {
            let error = expected_px[channel].abs_diff(actual_px[channel]);
            error_sum += u64::from(error);
            pixel_max = pixel_max.max(error);
        }
        max_error = max_error.max(pixel_max);
        if pixel_max > options.channel_tolerance {
            differing += 1;
            out.copy_from_slice(&[255, 0, 0, 255]);
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }

    let samples = compared * channels as u64;
    let report = DiffReport {
        compared_pixels: compared,
        differing_pixels: differing,
        mean_abs_error: if samples == 0 {
            0.0
        } else {
            error_sum as f64 / samples as f64
        },
        max_channel_error: max_error,
        bounding_box: bounds.map(|(x0, y0, x1, y1)| Rect {
            x: x0,
            y: y0,
            width: x1 - x0 + 1,
            height: y1 - y0 + 1,
        }),
    };
    let visual = Image {
        width: reference.width,
        height: reference.height,
        rgba: visual,
    };
    Ok((report, visual))
}

/// Rec. 601 luma of an RGBA pixel, in integer arithmetic.
fn grey_of(pixel: &[u8]) -> u8 {
    let luma = 299 * u32::from(pixel[0]) + 587 * u32::from(pixel[1]) + 114 * u32::from(pixel[2]);
    (luma / 1000) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey_image() -> Image {
        Image::filled(8, 4, [100, 100, 100, 255])
    }

    fn set(image: &mut Image, x: u32, y: u32, color: [u8; 4]) {
        let at = ((y * image.width + x) * 4) as usize;
        image.rgba[at..at + 4].copy_from_slice(&color);
    }

    #[test]
    fn identical_images_have_no_diff() {
        let (report, _) = compare(&grey_image(), &grey_image(), &DiffOptions::default()).unwrap();
        assert_eq!(report.compared_pixels, 32);
        assert_eq!(report.differing_pixels, 0);
        assert_eq!(report.mean_abs_error, 0.0);
        assert_eq!(report.max_channel_error, 0);
        assert_eq!(report.bounding_box, None);
        assert!(report.within(&Threshold::PROPOSED));
    }

    #[test]
    fn change_beyond_tolerance_is_reported() {
        let mut candidate = grey_image();
        set(&mut candidate, 3, 2, [200, 100, 100, 255]);
        let (report, visual) = compare(&grey_image(), &candidate, &DiffOptions::default()).unwrap();
        assert_eq!(report.differing_pixels, 1);
        assert_eq!(report.max_channel_error, 100);
        assert_eq!(
            report.bounding_box,
            Some(Rect {
                x: 3,
                y: 2,
                width: 1,
                height: 1
            })
        );
        assert!((report.differing_ratio() - 1.0 / 32.0).abs() < 1e-12);
        assert!((report.mean_abs_error - 100.0 / 96.0).abs() < 1e-12);
        let at = ((2 * 8 + 3) * 4) as usize;
        assert_eq!(&visual.rgba[at..at + 4], &[255, 0, 0, 255]);
        assert_eq!(&visual.rgba[0..4], &[100, 100, 100, 255]);
    }

    #[test]
    fn change_within_tolerance_is_ignored() {
        let mut candidate = grey_image();
        set(&mut candidate, 0, 0, [108, 92, 100, 255]);
        let (report, _) = compare(&grey_image(), &candidate, &DiffOptions::default()).unwrap();
        assert_eq!(report.differing_pixels, 0);
        assert_eq!(report.max_channel_error, 8);
        assert!(report.mean_abs_error > 0.0);
    }

    #[test]
    fn mask_excludes_region() {
        let mut candidate = grey_image();
        set(&mut candidate, 1, 1, [0, 0, 0, 255]);
        let options = DiffOptions {
            masks: vec![Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            }],
            ..DiffOptions::default()
        };
        let (report, _) = compare(&grey_image(), &candidate, &options).unwrap();
        assert_eq!(report.compared_pixels, 32 - 8);
        assert_eq!(report.differing_pixels, 0);
    }

    #[test]
    fn size_mismatch_is_an_error() {
        let other = Image::filled(4, 4, [0; 4]);
        let error = compare(&grey_image(), &other, &DiffOptions::default()).unwrap_err();
        assert!(matches!(error, ScreenshotError::SizeMismatch { .. }));
    }

    #[test]
    fn alpha_is_ignored_by_default_but_not_on_request() {
        let mut candidate = grey_image();
        set(&mut candidate, 0, 0, [100, 100, 100, 0]);
        let (ignored, _) = compare(&grey_image(), &candidate, &DiffOptions::default()).unwrap();
        assert_eq!(ignored.differing_pixels, 0);
        let strict = DiffOptions {
            ignore_alpha: false,
            ..DiffOptions::default()
        };
        let (counted, _) = compare(&grey_image(), &candidate, &strict).unwrap();
        assert_eq!(counted.differing_pixels, 1);
    }

    #[test]
    fn png_round_trip() {
        let mut image = grey_image();
        set(&mut image, 5, 3, [1, 2, 3, 4]);
        let path = std::env::temp_dir().join(format!(
            "umicraft-screenshot-{}-{:?}.png",
            std::process::id(),
            std::thread::current().id()
        ));
        image.save(&path).unwrap();
        let loaded = Image::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(loaded, image);
    }

    #[test]
    fn rgb_png_is_converted_to_opaque_rgba() {
        let path = std::env::temp_dir().join(format!(
            "umicraft-screenshot-rgb-{}-{:?}.png",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut encoder = png::Encoder::new(BufWriter::new(File::create(&path).unwrap()), 1, 1);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[10, 20, 30])
            .unwrap();
        let loaded = Image::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(loaded.rgba, vec![10, 20, 30, 255]);
    }

    #[test]
    fn threshold_within_and_outside() {
        let mut candidate = grey_image();
        set(&mut candidate, 0, 0, [0, 0, 0, 255]);
        let (report, _) = compare(&grey_image(), &candidate, &DiffOptions::default()).unwrap();
        let loose = Threshold {
            max_differing_ratio: 0.05,
            max_mean_abs_error: 5.0,
        };
        assert!(report.within(&loose));
        assert!(!report.within(&Threshold::PROPOSED));
    }
}
