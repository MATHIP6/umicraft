// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of the item tooltip under the pointer
//! (MCP `GuiScreen.drawHoveringText`).
//!
//! The lines come from [`umicraft_game::inventory::tooltip`]. There is no
//! language table in the client yet, so [`resolve`] turns the language keys
//! into English text itself: a small table for the keys the tooltip emits and
//! a humanised last key segment for everything else (`item.diamond_sword.name`
//! becomes "Diamond Sword").

use umicraft_game::inventory::tooltip::{TooltipLine, TooltipText};
use umicraft_ui::{Color, FontAtlas, Rect, TextStyle, Ui};

/// Gap between the pointer and the tooltip, in GUI pixels.
const POINTER_OFFSET: f32 = 12.0;
/// Space between the text and the border, in GUI pixels.
const PADDING: f32 = 3.0;
/// Height of one text line, in GUI pixels.
const LINE_HEIGHT: f32 = 10.0;
/// Height of the glyphs, in GUI pixels.
const GLYPH_HEIGHT: f32 = 8.0;
/// Extra gap after the title line, in GUI pixels.
const TITLE_GAP: f32 = 2.0;
const BACKGROUND: Color = [0.0625, 0.0, 0.0625, 0.94];
const BORDER_TOP: Color = [0.3125, 0.0, 1.0, 0.31];
const BORDER_BOTTOM: Color = [0.157, 0.0, 0.498, 0.31];

const ENCHANTMENTS: &[(&str, &str)] = &[
    ("protect.all", "Protection"),
    ("protect.fire", "Fire Protection"),
    ("protect.fall", "Feather Falling"),
    ("protect.explosion", "Blast Protection"),
    ("protect.projectile", "Projectile Protection"),
    ("oxygen", "Respiration"),
    ("waterWorker", "Aqua Affinity"),
    ("waterWalker", "Depth Strider"),
    ("damage.all", "Sharpness"),
    ("damage.undead", "Smite"),
    ("damage.arthropods", "Bane of Arthropods"),
    ("fire", "Fire Aspect"),
    ("digging", "Efficiency"),
    ("untouching", "Silk Touch"),
    ("durability", "Unbreaking"),
    ("lootBonus", "Looting"),
    ("lootBonusDigger", "Fortune"),
    ("lootBonusFishing", "Luck of the Sea"),
    ("fishingSpeed", "Lure"),
    ("arrowDamage", "Power"),
    ("arrowKnockback", "Punch"),
    ("arrowFire", "Flame"),
    ("arrowInfinite", "Infinity"),
];

const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];

/// The English text of a tooltip text.
pub fn resolve(text: &TooltipText) -> String {
    match text {
        TooltipText::Literal(text) => text.clone(),
        TooltipText::Concat(parts) => parts.iter().map(resolve).collect(),
        TooltipText::Translation { key, args } => {
            let args: Vec<String> = args.iter().map(resolve).collect();
            translate(key, &args)
        }
    }
}

fn translate(key: &str, args: &[String]) -> String {
    match key {
        "item.dyed" => return "Dyed".to_owned(),
        "item.unbreakable" => return "Unbreakable".to_owned(),
        "attribute.modifier.plus.0" => return format!("+{}", args.join(" ")),
        _ => {}
    }
    if let Some(level) = key.strip_prefix("enchantment.level.") {
        let roman = level
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| ROMAN.get(n));
        return roman.map_or_else(|| level.to_owned(), |r| (*r).to_owned());
    }
    if let Some(name) = key.strip_prefix("enchantment.") {
        if let Some((_, english)) = ENCHANTMENTS.iter().find(|(k, _)| *k == name) {
            return (*english).to_owned();
        }
    }
    if let Some(record) = key
        .strip_prefix("item.record.")
        .and_then(|k| k.strip_suffix(".desc"))
    {
        return format!("C418 - {record}");
    }
    humanise(key)
}

/// The key's most telling segment in title case: `item.diamond_sword.name`
/// gives "Diamond Sword", `attribute.name.generic.attackDamage` "Attack Damage".
fn humanise(key: &str) -> String {
    let segment = key
        .rsplit('.')
        .find(|s| !matches!(*s, "name" | "desc"))
        .unwrap_or(key);
    let mut words = String::with_capacity(segment.len() + 4);
    let mut start_of_word = true;
    for c in segment.chars() {
        if c == '_' {
            words.push(' ');
            start_of_word = true;
        } else if c.is_uppercase() && !start_of_word {
            words.push(' ');
            words.extend(c.to_uppercase());
            start_of_word = false;
        } else if start_of_word {
            words.extend(c.to_uppercase());
            start_of_word = false;
        } else {
            words.push(c);
        }
    }
    words
}

/// The vanilla position of a tooltip box: right of and above the pointer,
/// flipped to the left of it when it would leave the screen and pushed up
/// when it would leave the bottom. All values in physical pixels.
fn tooltip_origin(pointer: [f32; 2], size: [f32; 2], viewport: Rect, scale: f32) -> [f32; 2] {
    let mut x = pointer[0] + POINTER_OFFSET * scale;
    if x + size[0] > viewport.x + viewport.width {
        x -= (28.0 * scale) + size[0];
    }
    let mut y = pointer[1] - POINTER_OFFSET * scale;
    let bottom = viewport.y + viewport.height;
    if y + size[1] + 6.0 * scale > bottom {
        y = bottom - size[1] - 6.0 * scale;
    }
    [x, y]
}

/// The sRGB to linear transfer function, the inverse of what the sRGB render
/// target applies on write.
///
/// The UI pass hands vertex colours to an sRGB target as they are, so a
/// vanilla colour (written for a plain 8-bit framebuffer) has to be linearised
/// first or it comes out too light: the tooltip background 0x100010 showed as
/// a bright purple.
fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// A vanilla colour (straight sRGB channels and alpha) as the UI pass wants it.
fn vanilla(color: Color) -> Color {
    [
        linear(color[0]),
        linear(color[1]),
        linear(color[2]),
        color[3],
    ]
}

/// The text colour as straight RGBA.
fn rgba(rgb: u32, factor: f32) -> Color {
    let channel = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0 * factor;
    vanilla([channel(16), channel(8), channel(0), 1.0])
}

/// Draws `lines` in a vanilla tooltip box next to `pointer`.
///
/// `scale` is the GUI scale factor; `viewport` bounds the box.
pub fn draw_tooltip(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    scale: f32,
    pointer: [f32; 2],
    lines: &[TooltipLine],
) {
    if lines.is_empty() {
        return;
    }
    let texts: Vec<String> = lines.iter().map(|l| resolve(&l.text)).collect();
    let width = texts
        .iter()
        .map(|t| atlas.measure(t, scale))
        .fold(0.0, f32::max);
    let extra = if lines.len() > 1 {
        TITLE_GAP + (lines.len() - 1) as f32 * LINE_HEIGHT
    } else {
        0.0
    };
    let height = (GLYPH_HEIGHT + extra) * scale;
    let [x, y] = tooltip_origin(pointer, [width, height], viewport, scale);
    draw_box(ui, [x, y], [width, height], scale);

    let mut line_y = y;
    for (index, (line, text)) in lines.iter().zip(&texts).enumerate() {
        let shadow = TextStyle::new(scale, rgba(line.color.rgb(), 0.25));
        let style = TextStyle::new(scale, rgba(line.color.rgb(), 1.0));
        ui.draw()
            .text([x + scale, line_y + scale], text.clone(), shadow);
        ui.draw().text([x, line_y], text.clone(), style);
        line_y += if index == 0 {
            LINE_HEIGHT + TITLE_GAP
        } else {
            LINE_HEIGHT
        } * scale;
    }
}

/// Draws a vertical gradient as one strip per GUI pixel row.
///
/// The colours are interpolated as vanilla does, on the sRGB channels, and
/// each strip is then linearised for the render target.
fn gradient_column(
    draw: &mut umicraft_ui::DrawList,
    origin: [f32; 2],
    size: [f32; 2],
    top: Color,
    bottom: Color,
) {
    let rows = (size[1] / size[0]).round().max(1.0) as u32;
    let strip = size[1] / rows as f32;
    for row in 0..rows {
        let t = (row as f32 + 0.5) / rows as f32;
        let color = std::array::from_fn(|i| top[i] + (bottom[i] - top[i]) * t);
        draw.rect(
            Rect::new(origin[0], origin[1] + strip * row as f32, size[0], strip),
            vanilla(color),
        );
    }
}

/// The dark background with the purple border around a text block.
fn draw_box(ui: &mut Ui, origin: [f32; 2], size: [f32; 2], scale: f32) {
    let pad = PADDING * scale;
    let (x, y, w, h) = (origin[0], origin[1], size[0], size[1]);
    let (background, border_top, border_bottom) = (
        vanilla(BACKGROUND),
        vanilla(BORDER_TOP),
        vanilla(BORDER_BOTTOM),
    );
    let draw = ui.draw();
    // Background: the body plus a one-pixel rim on every side.
    draw.rect(
        Rect::new(
            x - pad - scale,
            y - pad,
            w + 2.0 * pad + 2.0 * scale,
            h + 2.0 * pad,
        ),
        background,
    );
    draw.rect(
        Rect::new(
            x - pad,
            y - pad - scale,
            w + 2.0 * pad,
            h + 2.0 * pad + 2.0 * scale,
        ),
        background,
    );
    // Border, inside the body: the sides fade from the top colour to the
    // bottom one (`GuiScreen.drawHoveringText` draws them with
    // `drawGradientRect`).
    let inner_h = h + 2.0 * pad - 2.0 * scale;
    let top = y - pad + scale;
    gradient_column(
        draw,
        [x - pad, top],
        [scale, inner_h],
        BORDER_TOP,
        BORDER_BOTTOM,
    );
    gradient_column(
        draw,
        [x + w + pad - scale, top],
        [scale, inner_h],
        BORDER_TOP,
        BORDER_BOTTOM,
    );
    draw.rect(
        Rect::new(x - pad, y - pad, w + 2.0 * pad, scale),
        border_top,
    );
    draw.rect(
        Rect::new(x - pad, y + h + pad - scale, w + 2.0 * pad, scale),
        border_bottom,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::inventory::tooltip::tooltip;
    use umicraft_protocol::packets::play::ItemStack;

    fn key(key: &str) -> TooltipText {
        TooltipText::Translation {
            key: key.into(),
            args: Vec::new(),
        }
    }

    #[test]
    fn vanilla_colours_are_linearised_for_the_srgb_target() {
        let background = vanilla(BACKGROUND);
        // 0x10 / 255 in sRGB is about 0.005 linear, not 0.0625.
        assert!((background[0] - 0.0052).abs() < 0.001);
        assert_eq!(background[3], 0.94);
        assert_eq!(rgba(0xFF_FF_FF, 1.0), [1.0, 1.0, 1.0, 1.0]);
        assert!(rgba(0x55_55_FF, 1.0)[0] < 0.1);
    }

    #[test]
    fn a_gradient_column_has_one_strip_per_gui_pixel_and_ends_on_the_borders() {
        let mut draw = umicraft_ui::DrawList::new();
        // Scale 2: a 2 px wide column, 20 px tall = 10 GUI pixel rows.
        gradient_column(
            &mut draw,
            [0.0, 0.0],
            [2.0, 20.0],
            BORDER_TOP,
            BORDER_BOTTOM,
        );
        assert_eq!(draw.len(), 10);
        let colors: Vec<Color> = draw
            .commands()
            .iter()
            .filter_map(|c| match c {
                umicraft_ui::DrawCommand::Rect { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        assert!(colors[0][2] > colors[9][2], "blue fades towards the bottom");
        assert!((colors[0][2] - vanilla(BORDER_TOP)[2]).abs() < 0.1);
    }

    #[test]
    fn item_keys_become_title_case_names() {
        assert_eq!(resolve(&key("item.diamond_sword.name")), "Diamond Sword");
        assert_eq!(resolve(&key("tile.stone.name")), "Stone");
        assert_eq!(
            resolve(&key("attribute.name.generic.attackDamage")),
            "Attack Damage"
        );
    }

    #[test]
    fn enchantments_and_levels_use_their_english_names() {
        let text = TooltipText::Concat(vec![
            key("enchantment.damage.all"),
            TooltipText::Literal(" ".into()),
            key("enchantment.level.3"),
        ]);
        assert_eq!(resolve(&text), "Sharpness III");
    }

    #[test]
    fn the_attack_damage_line_reads_like_vanilla() {
        let text = TooltipText::Translation {
            key: "attribute.modifier.plus.0".into(),
            args: vec![
                TooltipText::Literal("7".into()),
                key("attribute.name.generic.attackDamage"),
            ],
        };
        assert_eq!(resolve(&text), "+7 Attack Damage");
    }

    #[test]
    fn a_sword_tooltip_resolves_to_readable_lines() {
        let stack = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let lines: Vec<String> = tooltip(&stack).iter().map(|l| resolve(&l.text)).collect();
        assert_eq!(lines[0], "Diamond Sword");
    }

    #[test]
    fn the_box_flips_left_and_up_at_the_screen_edges() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        assert_eq!(
            tooltip_origin([100.0, 100.0], [50.0, 20.0], viewport, 2.0),
            [124.0, 76.0]
        );
        // Right edge: moved to the left of the pointer.
        assert_eq!(
            tooltip_origin([790.0, 100.0], [50.0, 20.0], viewport, 2.0)[0],
            790.0 - 56.0 - 50.0 + 24.0
        );
        // Bottom edge: pushed up.
        assert_eq!(
            tooltip_origin([100.0, 598.0], [50.0, 20.0], viewport, 2.0)[1],
            600.0 - 20.0 - 12.0
        );
    }
}
