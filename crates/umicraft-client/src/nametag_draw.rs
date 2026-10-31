// SPDX-License-Identifier: MIT OR Apache-2.0

//! Name tags above players and named mobs.
//!
//! Vanilla draws the tag as a billboard in the world (MCP
//! `Render.renderLivingLabel`, `RendererLivingEntity.renderName`): the text
//! is 0.0266667 blocks per font pixel, on a 25 % black box, turned to face
//! the camera. Here the anchor is projected to the screen and the text drawn
//! by the UI with the scale perspective gives it; there is no depth test, so a
//! tag shows through terrain (vanilla fades it to a ghost there).

use std::sync::Arc;

use glam::{Vec3, Vec4};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_render::Camera;
use umicraft_ui::{Color, FontAtlas, Rect, TextStyle, Ui};

/// World size of one font pixel of a tag (MCP `0.016666668 * 1.6`).
const WORLD_PER_FONT_PIXEL: f32 = 0.026_666_67;
/// Blocks above the hitbox where the tag sits.
const ABOVE_HEAD: f32 = 0.5;
/// How much a crouching player's tag drops: the model sinks 0.125 and the
/// tag sits 0.25 lower (MCP `renderName`, `translate(0, 9.374999, 0)`).
const SNEAK_DROP: f32 = 0.375;
/// Distance in blocks beyond which a tag is not drawn, and the one for a
/// crouching player.
const TAG_RANGE: f32 = 64.0;
const SNEAK_TAG_RANGE: f32 = 32.0;
/// Opacity of the box behind the text.
const BOX_COLOR: Color = [0.0, 0.0, 0.0, 0.25];
const WHITE: Color = [1.0, 1.0, 1.0, 1.0];
/// Height of a font glyph box in font pixels.
const FONT_HEIGHT: f32 = 9.0;

/// A tag placed on the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedTag {
    /// The text with its formatting codes.
    pub text: Arc<str>,
    /// Screen position of the middle of the top edge of the text, in pixels.
    pub anchor: [f32; 2],
    /// Pixels per font pixel.
    pub scale: f32,
}

/// Where each visible tag goes on a `width` x `height` screen.
#[must_use]
pub fn place_tags(
    entities: &[EntityRenderSnapshot],
    camera: &Camera,
    width: f32,
    height: f32,
) -> Vec<PlacedTag> {
    let view_projection = camera.view_projection();
    let forward = camera.forward();
    let up = camera.horizontal_right().cross(forward);
    entities
        .iter()
        .filter_map(|entity| {
            let text = entity.name_tag.as_ref()?;
            let sneaking = entity.kind == EntityKind::Player && entity.flags.is_sneaking();
            let range = if sneaking { SNEAK_TAG_RANGE } else { TAG_RANGE };
            if entity.position.distance(camera.position) >= range {
                return None;
            }
            let lift = entity.kind.height() + ABOVE_HEAD;
            let lift = if sneaking { lift - SNEAK_DROP } else { lift };
            let anchor = entity.position + Vec3::Y * lift;
            let top = project(view_projection, anchor, width, height)?;
            let above = project(
                view_projection,
                anchor + up * WORLD_PER_FONT_PIXEL,
                width,
                height,
            )?;
            // A font pixel is as many screen pixels as `up` moves it.
            let scale = (top[1] - above[1]).abs();
            (scale > 0.0).then(|| match text.split_once('\n') {
                // `RenderPlayer.renderOffsetLivingLabel`: the score line sits
                // where the tag would, and the name is lifted over it by
                // 1.15 line heights.
                Some((name, score)) => vec![
                    PlacedTag {
                        text: Arc::from(score),
                        anchor: top,
                        scale,
                    },
                    PlacedTag {
                        text: Arc::from(name),
                        anchor: [top[0], top[1] - FONT_HEIGHT * 1.15 * scale],
                        scale,
                    },
                ],
                None => vec![PlacedTag {
                    text: Arc::clone(text),
                    anchor: top,
                    scale,
                }],
            })
        })
        .flatten()
        .collect()
}

/// The screen position of a world point, `None` behind the camera.
fn project(view_projection: glam::Mat4, point: Vec3, width: f32, height: f32) -> Option<[f32; 2]> {
    let clip = view_projection * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let (x, y) = (clip.x / clip.w, clip.y / clip.w);
    Some([(x * 0.5 + 0.5) * width, (0.5 - y * 0.5) * height])
}

/// Draws the tags: a dark box and the text over it.
pub fn draw_tags(ui: &mut Ui, font: &FontAtlas, tags: &[PlacedTag]) {
    for tag in tags {
        let runs = format_runs(&tag.text);
        let plain: String = runs.iter().map(|(text, _)| text.as_str()).collect();
        let width = font.measure(&plain, tag.scale);
        let [cx, top] = tag.anchor;
        let left = cx - width / 2.0;
        ui.draw().rect(
            Rect::new(
                left - tag.scale,
                top - tag.scale,
                width + 2.0 * tag.scale,
                FONT_HEIGHT * tag.scale,
            ),
            BOX_COLOR,
        );
        let mut x = left;
        for (text, color) in runs {
            ui.draw()
                .text([x, top], text.clone(), TextStyle::new(tag.scale, color));
            x += font.measure(&text, tag.scale);
        }
    }
}

/// The colours of the formatting codes `0`-`f` (MCP `EnumChatFormatting`).
const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0xAA],
    [0x00, 0xAA, 0x00],
    [0x00, 0xAA, 0xAA],
    [0xAA, 0x00, 0x00],
    [0xAA, 0x00, 0xAA],
    [0xFF, 0xAA, 0x00],
    [0xAA, 0xAA, 0xAA],
    [0x55, 0x55, 0x55],
    [0x55, 0x55, 0xFF],
    [0x55, 0xFF, 0x55],
    [0x55, 0xFF, 0xFF],
    [0xFF, 0x55, 0x55],
    [0xFF, 0x55, 0xFF],
    [0xFF, 0xFF, 0x55],
    [0xFF, 0xFF, 0xFF],
];

/// Splits a text with `§` codes into runs of one colour. `§r` and the style
/// codes (`k` to `o`) reset or are dropped; text starts white.
fn format_runs(text: &str) -> Vec<(String, Color)> {
    let mut runs: Vec<(String, Color)> = Vec::new();
    let mut color = WHITE;
    let mut current = String::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '§' {
            current.push(character);
            continue;
        }
        let Some(code) = chars.next() else {
            break;
        };
        let next = match code.to_ascii_lowercase() {
            digit @ ('0'..='9' | 'a'..='f') => {
                let index = digit.to_digit(16).unwrap_or(15) as usize;
                let [r, g, b] = PALETTE[index];
                Some([
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ])
            }
            'r' => Some(WHITE),
            // Styles do not change the colour; the font has no bold or italic.
            _ => None,
        };
        if let Some(next) = next {
            if !current.is_empty() {
                runs.push((std::mem::take(&mut current), color));
            }
            color = next;
        }
    }
    if !current.is_empty() {
        runs.push((current, color));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId};

    fn tagged(kind: EntityKind, position: Vec3, text: &str) -> EntityRenderSnapshot {
        let mut entity = EntityRenderSnapshot::new(EntityId(1), kind, position);
        entity.name_tag = Some(Arc::from(text));
        entity
    }

    /// A camera at the origin looking south (towards +Z).
    fn camera() -> Camera {
        let mut camera = Camera::new(Vec3::ZERO, std::f32::consts::PI, 0.0);
        camera.set_aspect(1280, 720);
        camera
    }

    #[test]
    fn a_tag_in_front_is_placed_above_the_head_and_scaled_by_distance() {
        let near = place_tags(
            &[tagged(EntityKind::Player, Vec3::new(0.0, 0.0, 5.0), "Bob")],
            &camera(),
            1280.0,
            720.0,
        );
        let far = place_tags(
            &[tagged(EntityKind::Player, Vec3::new(0.0, 0.0, 20.0), "Bob")],
            &camera(),
            1280.0,
            720.0,
        );
        assert_eq!(near.len(), 1);
        assert!((near[0].anchor[0] - 640.0).abs() < 1.0, "centred");
        assert!(
            near[0].anchor[1] < 360.0,
            "above the eye line: {:?}",
            near[0].anchor
        );
        assert!(
            (near[0].scale / far[0].scale - 4.0).abs() < 0.3,
            "1/d scale"
        );
    }

    #[test]
    fn tags_behind_the_camera_or_out_of_range_are_dropped() {
        let behind = tagged(EntityKind::Player, Vec3::new(0.0, 0.0, -5.0), "Bob");
        let far = tagged(EntityKind::Player, Vec3::new(0.0, 0.0, 70.0), "Bob");
        assert!(place_tags(&[behind, far], &camera(), 1280.0, 720.0).is_empty());
    }

    #[test]
    fn a_crouching_player_has_a_shorter_range() {
        let mut crouching = tagged(EntityKind::Player, Vec3::new(0.0, 0.0, 40.0), "Bob");
        crouching.flags = EntityFlags::new(0x02, 0x7F);
        let standing = tagged(EntityKind::Player, Vec3::new(0.0, 0.0, 40.0), "Bob");
        assert!(place_tags(&[crouching], &camera(), 1280.0, 720.0).is_empty());
        assert_eq!(place_tags(&[standing], &camera(), 1280.0, 720.0).len(), 1);
    }

    #[test]
    fn a_score_line_goes_under_the_name() {
        let tags = place_tags(
            &[tagged(
                EntityKind::Player,
                Vec3::new(0.0, 0.0, 5.0),
                "Bob\n12 Kills",
            )],
            &camera(),
            1280.0,
            720.0,
        );
        assert_eq!(tags.len(), 2);
        assert_eq!(&*tags[0].text, "12 Kills");
        assert_eq!(&*tags[1].text, "Bob");
        // The name is a line and 15 % higher up the screen than the score.
        let lift = tags[0].anchor[1] - tags[1].anchor[1];
        assert!((lift - FONT_HEIGHT * 1.15 * tags[0].scale).abs() < 1e-3);
    }

    #[test]
    fn an_entity_without_a_tag_has_none() {
        let mut entity = tagged(EntityKind::Player, Vec3::new(0.0, 0.0, 5.0), "Bob");
        entity.name_tag = None;
        assert!(place_tags(&[entity], &camera(), 1280.0, 720.0).is_empty());
    }

    #[test]
    fn formatting_codes_split_the_text_into_coloured_runs() {
        let runs = format_runs("§c[R] §rBob§7!");
        let texts: Vec<_> = runs.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts, ["[R] ", "Bob", "!"]);
        assert_eq!(runs[0].1, [1.0, 85.0 / 255.0, 85.0 / 255.0, 1.0]);
        assert_eq!(runs[1].1, WHITE);
        assert_eq!(runs[2].1[0], 170.0 / 255.0);
    }

    #[test]
    fn style_codes_and_a_dangling_marker_are_dropped() {
        let runs = format_runs("§lBold§");
        assert_eq!(runs, vec![("Bold".to_owned(), WHITE)]);
        assert!(format_runs("").is_empty());
    }
}
