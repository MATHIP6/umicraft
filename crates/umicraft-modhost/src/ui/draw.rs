// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the `hud_*` host functions do to a widget's content, with their validation.
//!
//! The `ffi` module decodes the raw arguments (pointer, UTF-8) and calls these safe
//! functions. A mod cannot make the HUD misbehave from here: numbers must be finite,
//! sizes non-negative, opacity and text scale are clamped, text is cut to a
//! bounded length and a widget has a bounded number of items (ADR-0086, F3.4).

use umicraft_builtin_mods::{HudCircle, HudContent, HudSegment};
use umicraft_modapi_sys::UmiStatus;

use super::{MAX_HUD_ITEMS, MAX_HUD_TEXT_BYTES};

/// Largest size a widget may reserve, in GUI pixels.
const MAX_RESERVE: f32 = 4096.0;
/// Range of the scale of a free label, so a mod cannot draw invisible or screen-size text.
const LABEL_SCALE: (f32, f32) = (0.25, 4.0);
/// Range of the thickness of a segment, in GUI pixels (ADR-0094; the lower end
/// went from 0.5 to 0.05 in API 1.4, ADR-0097).
const SEGMENT_THICKNESS: (f32, f32) = (0.05, 8.0);
/// Widest ring of a circle, in GUI pixels.
const MAX_RING: f32 = 8.0;
/// Corners a polygon may have (API 1.4).
pub(crate) const POLYGON_POINTS: (usize, usize) = (3, 64);
/// Clips a widget may nest (API 1.4).
const MAX_CLIPS: usize = 8;
/// Width of the edges of an outline, in GUI pixels.
const OUTLINE_WIDTH: f32 = 1.0;

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

/// Keeps `0xRRGGBB`; the alpha byte a C mod may leave in the high bits is dropped.
fn rgb(color: u32) -> u32 {
    color & 0x00ff_ffff
}

/// Cuts `text` to at most [`MAX_HUD_TEXT_BYTES`] without splitting a character.
fn clip(text: &str) -> &str {
    if text.len() <= MAX_HUD_TEXT_BYTES {
        return text;
    }
    let mut end = MAX_HUD_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `OK` while `used` items leave room for one more, `DENIED` otherwise.
fn room(used: usize) -> Result<(), UmiStatus> {
    room_for(used, 1)
}

/// `OK` while `used` items leave room for `more`, `DENIED` otherwise.
fn room_for(used: usize, more: usize) -> Result<(), UmiStatus> {
    if used + more <= MAX_HUD_ITEMS {
        Ok(())
    } else {
        Err(UmiStatus::DENIED)
    }
}

fn status(result: Result<(), UmiStatus>) -> UmiStatus {
    result.err().unwrap_or(UmiStatus::OK)
}

/// `hud_line`: adds a text line.
pub(crate) fn line(content: &mut HudContent, text: &str, color: u32) -> UmiStatus {
    status(room(content.lines().len()).map(|()| {
        content.line(rgb(color)).push_str(clip(text));
    }))
}

/// `hud_label`: adds free text centred on `(x, y)`.
pub(crate) fn label(
    content: &mut HudContent,
    at: [f32; 2],
    text: &str,
    color: u32,
    alpha: f32,
    scale: f32,
) -> UmiStatus {
    if !all_finite(&[at[0], at[1], alpha, scale]) {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.labels().len()).map(|()| {
        let scale = scale.clamp(LABEL_SCALE.0, LABEL_SCALE.1);
        let text_out = content.label(at, rgb(color), alpha.clamp(0.0, 1.0), scale);
        text_out.push_str(clip(text));
    }))
}

/// `hud_cell`: adds a boxed cell; `rect` is `[x, y, width, height]`.
pub(crate) fn cell(
    content: &mut HudContent,
    rect: [f32; 4],
    pressed: bool,
    text: &str,
    color: u32,
) -> UmiStatus {
    if !all_finite(&rect) || rect[2] < 0.0 || rect[3] < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.cells().len()).map(|()| {
        content.cell(rect, pressed, rgb(color)).push_str(clip(text));
    }))
}

/// `hud_tick`: adds a filled rectangle; `rect` is `[x, y, width, height]`.
pub(crate) fn tick(content: &mut HudContent, rect: [f32; 4], color: u32, alpha: f32) -> UmiStatus {
    if !all_finite(&[rect[0], rect[1], rect[2], rect[3], alpha]) || rect[2] < 0.0 || rect[3] < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.ticks().len()).map(|()| {
        content.tick(rect, rgb(color), alpha.clamp(0.0, 1.0));
    }))
}

/// `hud_outline`: adds the one-pixel outline of `rect` (`[x, y, width, height]`),
/// as four rectangles that count against the rectangle cap.
pub(crate) fn outline(
    content: &mut HudContent,
    rect: [f32; 4],
    color: u32,
    alpha: f32,
) -> UmiStatus {
    let [x, y, width, height] = rect;
    if !all_finite(&[x, y, width, height, alpha]) || width < 0.0 || height < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room_for(content.ticks().len(), 4).map(|()| {
        let (color, alpha) = (rgb(color), alpha.clamp(0.0, 1.0));
        let edge = OUTLINE_WIDTH.min(width).min(height);
        let inner = (height - 2.0 * edge).max(0.0);
        for side in [
            [x, y, width, edge],
            [x, y + height - edge, width, edge],
            [x, y + edge, edge, inner],
            [x + width - edge, y + edge, edge, inner],
        ] {
            content.tick(side, color, alpha);
        }
    }))
}

/// `hud_segment`: adds a line from `from` to `to`, its thickness clamped.
pub(crate) fn segment(
    content: &mut HudContent,
    [from, to]: [[f32; 2]; 2],
    color: u32,
    alpha: f32,
    thickness: f32,
) -> UmiStatus {
    if !all_finite(&[from[0], from[1], to[0], to[1], alpha, thickness]) {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.segments().len()).map(|()| {
        content.segment(HudSegment {
            from,
            to,
            thickness: thickness.clamp(SEGMENT_THICKNESS.0, SEGMENT_THICKNESS.1),
            color: rgb(color),
            alpha: alpha.clamp(0.0, 1.0),
            clip: None,
        });
    }))
}

/// `hud_circle`: adds a disc (`thickness <= 0`) or a ring at most [`MAX_RING`]
/// wide.
pub(crate) fn circle(
    content: &mut HudContent,
    center: [f32; 2],
    radius: f32,
    color: u32,
    alpha: f32,
    thickness: f32,
) -> UmiStatus {
    if !all_finite(&[center[0], center[1], radius, alpha, thickness]) || radius < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.circles().len()).map(|()| {
        content.circle(HudCircle {
            center,
            radius: radius.min(MAX_RESERVE),
            thickness: thickness.clamp(0.0, MAX_RING),
            color: rgb(color),
            alpha: alpha.clamp(0.0, 1.0),
            clip: None,
        });
    }))
}

/// `hud_polygon`: adds a filled polygon through `points` (3 to 64 of them).
pub(crate) fn polygon(
    content: &mut HudContent,
    points: &[[f32; 2]],
    color: u32,
    alpha: f32,
) -> UmiStatus {
    let (fewest, most) = POLYGON_POINTS;
    if !(fewest..=most).contains(&points.len())
        || !alpha.is_finite()
        || !points.iter().all(|point| all_finite(point))
    {
        return UmiStatus::INVALID_ARGUMENT;
    }
    status(room(content.polygons().len()).map(|()| {
        content.polygon(points.iter().copied(), rgb(color), alpha.clamp(0.0, 1.0));
    }))
}

/// `hud_clip_push`: opens a clip; `DENIED` past [`MAX_CLIPS`].
pub(crate) fn clip_push(content: &mut HudContent, rect: [f32; 4]) -> UmiStatus {
    if !all_finite(&rect) || rect[2] < 0.0 || rect[3] < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    if content.clip_depth() >= MAX_CLIPS {
        return UmiStatus::DENIED;
    }
    content.push_clip(rect);
    UmiStatus::OK
}

/// `hud_clip_pop`: closes the innermost clip; `INVALID_ARGUMENT` when none is open.
pub(crate) fn clip_pop(content: &mut HudContent) -> UmiStatus {
    if content.pop_clip() {
        UmiStatus::OK
    } else {
        UmiStatus::INVALID_ARGUMENT
    }
}

/// `hud_reserve`: keeps the widget at least `width` x `height` pixels big.
pub(crate) fn reserve(content: &mut HudContent, width: f32, height: f32) -> UmiStatus {
    if !all_finite(&[width, height]) || width < 0.0 || height < 0.0 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    content.reserve = [width.min(MAX_RESERVE), height.min(MAX_RESERVE)];
    UmiStatus::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_recorded_with_masked_colours_and_clamped_opacity() {
        let mut c = HudContent::default();
        assert_eq!(line(&mut c, "fps", 0xff_12_34_56), UmiStatus::OK);
        assert_eq!(
            (c.lines()[0].text.as_str(), c.lines()[0].color),
            ("fps", 0x12_34_56)
        );
        label(&mut c, [1.0, 2.0], "x", 0xffffff, 5.0, 100.0);
        assert_eq!(
            (c.labels()[0].alpha, c.labels()[0].scale),
            (1.0, LABEL_SCALE.1)
        );
        cell(&mut c, [0.0, 0.0, 10.0, 10.0], true, "W", 1);
        assert!(c.cells()[0].pressed);
        tick(&mut c, [0.0, 0.0, 1.0, 1.0], 2, -1.0);
        assert_eq!(c.ticks()[0].alpha, 0.0);
        assert_eq!(reserve(&mut c, 50.0, 99999.0), UmiStatus::OK);
        assert_eq!(c.reserve, [50.0, MAX_RESERVE]);
    }

    #[test]
    fn non_finite_and_negative_numbers_are_refused() {
        let mut c = HudContent::default();
        let bad = UmiStatus::INVALID_ARGUMENT;
        assert_eq!(label(&mut c, [f32::NAN, 0.0], "x", 0, 1.0, 1.0), bad);
        assert_eq!(label(&mut c, [0.0, 0.0], "x", 0, f32::INFINITY, 1.0), bad);
        assert_eq!(cell(&mut c, [0.0, 0.0, -1.0, 1.0], false, "x", 0), bad);
        assert_eq!(tick(&mut c, [0.0, 0.0, 1.0, f32::NAN], 0, 1.0), bad);
        assert_eq!(reserve(&mut c, -1.0, 1.0), bad);
        assert_eq!(reserve(&mut c, f32::NAN, 1.0), bad);
        assert!(c.is_empty());
    }

    #[test]
    fn an_outline_is_four_edges_inside_its_rectangle() {
        let mut c = HudContent::default();
        assert_eq!(
            outline(&mut c, [10.0, 20.0, 30.0, 40.0], 0xff_00ff00, 2.0),
            UmiStatus::OK
        );
        let rects: Vec<_> = c.ticks().iter().map(|t| t.rect).collect();
        assert_eq!(
            rects,
            [
                [10.0, 20.0, 30.0, 1.0],
                [10.0, 59.0, 30.0, 1.0],
                [10.0, 21.0, 1.0, 38.0],
                [39.0, 21.0, 1.0, 38.0],
            ]
        );
        assert!(c
            .ticks()
            .iter()
            .all(|t| t.color == 0x00ff00 && t.alpha == 1.0));
        let bad = UmiStatus::INVALID_ARGUMENT;
        assert_eq!(outline(&mut c, [0.0, 0.0, -1.0, 1.0], 0, 1.0), bad);
        assert_eq!(outline(&mut c, [f32::NAN, 0.0, 1.0, 1.0], 0, 1.0), bad);
    }

    #[test]
    fn an_outline_needs_room_for_its_four_edges() {
        let mut c = HudContent::default();
        for _ in 0..MAX_HUD_ITEMS - 3 {
            tick(&mut c, [0.0; 4], 0, 1.0);
        }
        assert_eq!(
            outline(&mut c, [0.0, 0.0, 4.0, 4.0], 0, 1.0),
            UmiStatus::DENIED
        );
        assert_eq!(c.ticks().len(), MAX_HUD_ITEMS - 3, "nothing half drawn");
    }

    #[test]
    fn a_segment_is_recorded_with_its_thickness_clamped() {
        let mut c = HudContent::default();
        assert_eq!(
            segment(&mut c, [[0.0, 1.0], [5.0, 6.0]], 0xff_123456, 0.5, 100.0),
            UmiStatus::OK
        );
        assert_eq!(
            segment(&mut c, [[0.0, 1.0], [5.0, 6.0]], 0, 0.5, 0.0),
            UmiStatus::OK
        );
        let [wide, thin] = c.segments() else {
            panic!("two segments");
        };
        assert_eq!(
            (wide.from, wide.to, wide.color),
            ([0.0, 1.0], [5.0, 6.0], 0x123456)
        );
        assert_eq!(
            (wide.thickness, thin.thickness),
            (SEGMENT_THICKNESS.1, SEGMENT_THICKNESS.0)
        );
    }

    #[test]
    fn a_circle_is_a_disc_or_a_clamped_ring() {
        let mut c = HudContent::default();
        assert_eq!(
            circle(&mut c, [5.0, 5.0], 4.0, 0xff_00ff00, 0.5, -1.0),
            UmiStatus::OK
        );
        assert_eq!(circle(&mut c, [5.0, 5.0], 4.0, 0, 2.0, 50.0), UmiStatus::OK);
        let [disc, ring] = c.circles() else {
            panic!("two circles");
        };
        assert_eq!(
            (disc.thickness, disc.color, disc.alpha),
            (0.0, 0x00ff00, 0.5)
        );
        assert_eq!((ring.thickness, ring.alpha), (MAX_RING, 1.0));
        let bad = UmiStatus::INVALID_ARGUMENT;
        assert_eq!(circle(&mut c, [0.0; 2], -1.0, 0, 1.0, 0.0), bad);
        assert_eq!(circle(&mut c, [f32::NAN, 0.0], 1.0, 0, 1.0, 0.0), bad);
    }

    #[test]
    fn a_polygon_needs_three_to_sixty_four_finite_points() {
        let mut c = HudContent::default();
        let triangle = [[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]];
        assert_eq!(polygon(&mut c, &triangle, 0xffffff, 1.0), UmiStatus::OK);
        assert_eq!(c.polygons()[0].points, triangle);
        let bad = UmiStatus::INVALID_ARGUMENT;
        assert_eq!(polygon(&mut c, &triangle[..2], 0, 1.0), bad);
        assert_eq!(polygon(&mut c, &[[0.0; 2]; 65], 0, 1.0), bad);
        assert_eq!(polygon(&mut c, &[[0.0, f32::INFINITY]; 3], 0, 1.0), bad);
        assert_eq!(polygon(&mut c, &[[0.0; 2]; 64], 0, 1.0), UmiStatus::OK);
    }

    #[test]
    fn clips_nest_up_to_the_bound_and_a_stray_pop_is_refused() {
        let mut c = HudContent::default();
        assert_eq!(clip_pop(&mut c), UmiStatus::INVALID_ARGUMENT);
        assert_eq!(
            clip_push(&mut c, [0.0, 0.0, -1.0, 1.0]),
            UmiStatus::INVALID_ARGUMENT
        );
        for _ in 0..MAX_CLIPS {
            assert_eq!(clip_push(&mut c, [0.0, 0.0, 9.0, 9.0]), UmiStatus::OK);
        }
        assert_eq!(clip_push(&mut c, [0.0; 4]), UmiStatus::DENIED);
        tick(&mut c, [1.0, 1.0, 2.0, 2.0], 0, 1.0);
        assert_eq!(c.ticks()[0].clip, Some([0.0, 0.0, 9.0, 9.0]));
        assert_eq!(clip_pop(&mut c), UmiStatus::OK);
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let mut c = HudContent::default();
        let text = "\u{30c4}".repeat(MAX_HUD_TEXT_BYTES);
        line(&mut c, &text, 0);
        let kept = &c.lines()[0].text;
        assert!(kept.len() <= MAX_HUD_TEXT_BYTES && kept.len() > MAX_HUD_TEXT_BYTES - 3);
    }

    #[test]
    fn a_widget_has_a_bounded_number_of_items() {
        let mut c = HudContent::default();
        for _ in 0..MAX_HUD_ITEMS {
            assert_eq!(line(&mut c, "x", 0), UmiStatus::OK);
            assert_eq!(tick(&mut c, [0.0; 4], 0, 1.0), UmiStatus::OK);
        }
        assert_eq!(line(&mut c, "x", 0), UmiStatus::DENIED);
        assert_eq!(tick(&mut c, [0.0; 4], 0, 1.0), UmiStatus::DENIED);
    }
}
