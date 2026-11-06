// SPDX-License-Identifier: MIT OR Apache-2.0

//! Signs (MCP `TileEntitySignRenderer`).
//!
//! The plate and its post are the `SignModel`. The four lines are drawn on a
//! plane in front of the board with the font of the UI: one quad per glyph,
//! laid out in font pixels (x right, y down) as `drawString` does and mapped
//! to the block by [`sign_text_transform`].
//!
//! The reference draws each line as the first line of the component wrapped to
//! 90 pixels (`GuiUtilRenderComponents.func_178908_a`), centred on the
//! plane's origin, in black. Here the wrap is a trim to 90 pixels, which is
//! the same for text that fits and cuts a longer line at the same place as
//! its first wrapped line unless the wrap would break it at a space.
//! The reference has no distance of its own for the text: it is drawn with
//! the tile entity, so the cut of `mod.rs` applies to both.

use std::collections::HashMap;

use glam::Vec3;
use umicraft_game::server::block_entities::Pos;
use umicraft_mesh::{BlockEntityKind, BlockEntityRef};
use umicraft_model::entity::{
    sign_text_transform, standing_sign_degrees, wall_sign_degrees, ModelVertex, SignModel,
    TextureSlot,
};
use umicraft_render::EntityVertex;
use umicraft_ui::FontAtlas;

use super::{push_at_block, BlockEntityScene};
use crate::chat;

/// The widest a line is drawn, in font pixels (the `90` of the reference).
const MAX_LINE_WIDTH: f32 = 90.0;
/// Pixels between the tops of two lines (`j * 10`).
const LINE_SPACING: f32 = 10.0;
/// Lines of a sign.
const LINES: usize = 4;

/// Draws the sign at `entry`, with its text when the lines are known.
pub(super) fn add_sign(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let standing = BlockEntityKind::of(entry.state) == Some(BlockEntityKind::StandingSign);
    let degrees = rotation_degrees(standing, entry.state.metadata());
    push_at_block(
        batches.entry(TextureSlot::Sign).or_default(),
        SignModel::emit(standing, degrees),
        entry,
    );
    let [x, y, z] = entry.position;
    let pos: Pos = (x, y, z);
    let (Some(font), Some(lines)) = (scene.font, scene.signs.get(&pos)) else {
        return;
    };
    let text = text_vertices(font, lines, standing, degrees);
    if !text.is_empty() {
        push_at_block(
            batches.entry(TextureSlot::SignText).or_default(),
            text,
            entry,
        );
    }
}

/// The turn of the sign from its block metadata.
fn rotation_degrees(standing: bool, metadata: u8) -> f32 {
    if standing {
        standing_sign_degrees(metadata)
    } else {
        wall_sign_degrees(metadata)
    }
}

/// The glyph quads of the four lines, in block space.
fn text_vertices(
    font: &FontAtlas,
    lines: &[String; LINES],
    standing: bool,
    degrees: f32,
) -> Vec<ModelVertex> {
    let plane = sign_text_transform(standing, degrees);
    let normal = plane
        .transform_vector3(Vec3::Z)
        .normalize_or_zero()
        .to_array();
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let text = trim_to_width(font, &chat::flatten(line), MAX_LINE_WIDTH);
        let left = -(font.measure(&text, 1.0) as i32 / 2) as f32;
        let top = index as f32 * LINE_SPACING - (LINES * 5) as f32;
        let mut pen = left;
        for character in text.chars() {
            if let Some(glyph) = font.glyph_or_fallback(character) {
                if !character.is_whitespace() {
                    push_glyph(&mut out, glyph_rect(font, glyph, pen, top), &plane, normal);
                }
                // The pen moves by what drawing returns, `(int) f`.
                pen += glyph.pen.trunc();
            }
        }
    }
    out
}

/// A glyph placed in the text plane: corners `[x0, y0, x1, y1]` and the UVs
/// `[u0, v0, u1, v1]` of the same rectangle in the atlas.
type GlyphRect = ([f32; 4], [f32; 4]);

/// The rectangle of `glyph` with its pen at `(x, y)`.
fn glyph_rect(font: &FontAtlas, glyph: &umicraft_ui::Glyph, x: f32, y: f32) -> GlyphRect {
    let width = (glyph.uv_max[0] - glyph.uv_min[0]) * font.width as f32 * glyph.scale;
    let height = (glyph.uv_max[1] - glyph.uv_min[1]) * font.height as f32 * glyph.scale;
    (
        [x, y, x + width, y + height],
        [
            glyph.uv_min[0],
            glyph.uv_min[1],
            glyph.uv_max[0],
            glyph.uv_max[1],
        ],
    )
}

/// Appends the two triangles of a glyph rectangle.
fn push_glyph(
    out: &mut Vec<ModelVertex>,
    (rect, uv): GlyphRect,
    plane: &glam::Mat4,
    normal: [f32; 3],
) {
    let corner = |x: f32, y: f32, u: f32, v: f32| ModelVertex {
        position: plane.transform_point3(Vec3::new(x, y, 0.0)).to_array(),
        normal,
        uv: [u, v],
    };
    let [x0, y0, x1, y1] = rect;
    let [u0, v0, u1, v1] = uv;
    let (a, b) = (corner(x0, y0, u0, v0), corner(x1, y0, u1, v0));
    let (c, d) = (corner(x1, y1, u1, v1), corner(x0, y1, u0, v1));
    out.extend([a, b, c, a, c, d]);
}

/// The longest start of `text` that is at most `width` pixels wide
/// (`FontRenderer.trimStringToWidth`, which stops before a character that
/// would pass the width).
fn trim_to_width(font: &FontAtlas, text: &str, width: f32) -> String {
    let mut used = 0.0;
    let mut kept = String::new();
    for character in text.chars() {
        used += font.advance(character, 1.0);
        if used > width {
            break;
        }
        kept.push(character);
    }
    kept
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::SignTable;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    const STANDING_SIGN: u16 = 63;
    const WALL_SIGN: u16 = 68;

    fn entry(id: u16, metadata: u8) -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(id, metadata),
            same_neighbour: umicraft_mesh::SameNeighbours::default(),
            light: LightLevel { block: 0, sky: 15 },
        }
    }

    fn lines(texts: [&str; 4]) -> SignTable {
        SignTable::from([((4, 70, 9), texts.map(|text| format!("\"{text}\"")))])
    }

    fn draw(
        entry: &BlockEntityRef,
        signs: &SignTable,
        font: Option<&FontAtlas>,
    ) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let lids = umicraft_game::server::chest_lids::ChestLids::default();
        let scene = BlockEntityScene {
            data: &umicraft_game::server::block_entities::EntityTable::new(),
            seconds: 0.0,
            skins: &crate::entity_render::NoSkins,
            lids: &lids,
            signs,
            font,
            partial_tick: 0.0,
            camera: Vec3::ZERO,
        };
        let mut batches = HashMap::new();
        add_sign(&mut batches, entry, &scene);
        batches
    }

    #[test]
    fn a_standing_sign_has_a_plate_and_a_post_and_a_wall_sign_only_a_plate() {
        let none = SignTable::new();
        assert_eq!(
            draw(&entry(STANDING_SIGN, 0), &none, None)[&TextureSlot::Sign].len(),
            72
        );
        assert_eq!(
            draw(&entry(WALL_SIGN, 3), &none, None)[&TextureSlot::Sign].len(),
            36
        );
    }

    #[test]
    fn a_line_makes_one_quad_per_visible_glyph_and_an_empty_sign_none() {
        let font = FontAtlas::fallback();
        let hello = lines(["Hi a", "", "", ""]);
        let batches = draw(&entry(STANDING_SIGN, 0), &hello, Some(&font));
        // "Hi a": three glyphs, the space is not drawn.
        assert_eq!(batches[&TextureSlot::SignText].len(), 3 * 6);
        let empty = lines(["", "", "", ""]);
        let batches = draw(&entry(STANDING_SIGN, 0), &empty, Some(&font));
        assert!(!batches.contains_key(&TextureSlot::SignText));
    }

    #[test]
    fn no_text_is_drawn_without_a_font_or_without_lines() {
        let font = FontAtlas::fallback();
        let hello = lines(["Hi", "", "", ""]);
        assert!(!draw(&entry(WALL_SIGN, 2), &hello, None).contains_key(&TextureSlot::SignText));
        let unknown = SignTable::new();
        assert!(
            !draw(&entry(WALL_SIGN, 2), &unknown, Some(&font)).contains_key(&TextureSlot::SignText)
        );
    }

    #[test]
    fn a_long_line_is_trimmed_to_ninety_pixels() {
        let font = FontAtlas::fallback();
        let trimmed = trim_to_width(&font, &"a".repeat(100), MAX_LINE_WIDTH);
        assert!(font.measure(&trimmed, 1.0) <= MAX_LINE_WIDTH);
        assert!(font.measure(&format!("{trimmed}a"), 1.0) > MAX_LINE_WIDTH);
        assert_eq!(trim_to_width(&font, "short", MAX_LINE_WIDTH), "short");
    }

    #[test]
    fn the_text_lies_in_front_of_the_board_within_the_block() {
        let font = FontAtlas::fallback();
        let long = "W".repeat(40);
        let wide = lines([long.as_str(); 4]);
        let batches = draw(&entry(STANDING_SIGN, 0), &wide, Some(&font));
        for vertex in &batches[&TextureSlot::SignText] {
            let [x, _, z] = vertex.position;
            assert!((4.0..=5.0).contains(&x), "{x}");
            // In front of the board (z 9.5 + 0.047), not behind it.
            assert!(z > 9.5, "{z}");
        }
    }

    #[test]
    fn the_rotation_follows_the_kind() {
        assert_eq!(rotation_degrees(true, 4), 90.0);
        assert_eq!(rotation_degrees(false, 4), 90.0);
        assert_eq!(rotation_degrees(true, 2), 45.0);
        assert_eq!(rotation_degrees(false, 2), 180.0);
    }
}
