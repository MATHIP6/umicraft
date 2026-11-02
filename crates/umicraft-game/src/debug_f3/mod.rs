// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vanilla F3 debug screen as text (P9-g, ADR-0036).
//!
//! [`DebugF3Lines::build`] turns a plain [`DebugF3Input`] into the two columns
//! of MCP 1.8.8 `GuiOverlayDebug` (`call` for the left one,
//! `getDebugInfoRight` for the right one), in the vanilla order and with the
//! vanilla `String.format` patterns. The module has no GPU and no clock: the
//! client gathers the numbers, the renderer draws the lines.
//!
//! Differences from the reference are deliberate and listed in
//! `docs/parity.md`: the `Java:` / `CPU:` / `Allocated:` lines and the GL
//! vendor/renderer/version lines are replaced by the memory line and the GPU
//! adapter and backend, and the `vbo` flag, the shader line and the
//! integrated-server difficulty lookup do not exist.
//!
//! Empty strings are real lines (vanilla draws no band for them but they take
//! a row), so the renderer must skip the band, not the row.
//!
//! ```
//! use umicraft_game::debug_f3::{DebugF3Input, DebugF3Lines};
//!
//! let lines = DebugF3Lines::build(&DebugF3Input::default());
//! assert_eq!(lines.left[0], "Minecraft 1.8.8 (Umicraft)");
//! ```

use std::fmt::Write as _;

/// The first left line (`"Minecraft 1.8.8 (" + version + "/" + brand + ")"`).
pub const VERSION_LINE: &str = "Minecraft 1.8.8 (Umicraft)";

/// The section sign colour prefix of `EnumChatFormatting.GREEN`.
pub const GREEN: &str = "\u{a7}a";
/// The section sign colour prefix of `EnumChatFormatting.RED`.
pub const RED: &str = "\u{a7}c";

/// The four horizontal facings in `EnumFacing.HORIZONTALS` order (the
/// `horizonalIndex` of each constant: south 0, west 1, north 2, east 3).
const HORIZONTALS: [Facing; 4] = [Facing::South, Facing::West, Facing::North, Facing::East];

/// A horizontal direction (`EnumFacing` without up and down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    /// Negative Z.
    North,
    /// Positive Z.
    South,
    /// Negative X.
    West,
    /// Positive X.
    East,
}

impl Facing {
    /// The facing of a yaw in degrees (`Entity.getHorizontalFacing`).
    #[must_use]
    pub fn from_yaw(yaw: f32) -> Self {
        let index = (f64::from(yaw) * 4.0 / 360.0 + 0.5).floor() as i64 & 3;
        HORIZONTALS[index as usize]
    }

    /// The lower-case name `EnumFacing.toString` prints.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::West => "west",
            Self::East => "east",
        }
    }

    /// The text after the name in the `Facing:` line.
    #[must_use]
    pub const fn towards(self) -> &'static str {
        match self {
            Self::North => "Towards negative Z",
            Self::South => "Towards positive Z",
            Self::West => "Towards negative X",
            Self::East => "Towards positive X",
        }
    }
}

/// The light and biome of the block the player stands in
/// (`Chunk.getBiome`, `getLightSubtracted`, `getLightFor`).
#[derive(Debug, Clone, PartialEq)]
pub struct DebugLocation {
    /// The biome name (`BiomeGenBase.biomeName`).
    pub biome: String,
    /// The combined light (`getLightSubtracted(pos, 0)`).
    pub light: u8,
    /// The sky light (`getLightFor(SKY, pos)`).
    pub sky_light: u8,
    /// The block light (`getLightFor(BLOCK, pos)`).
    pub block_light: u8,
    /// `DifficultyInstance.getAdditionalDifficulty`.
    pub local_difficulty: f32,
    /// `world time / 24000`.
    pub day: u64,
}

/// A block state property value (`IBlockState.getProperties`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValue {
    /// A boolean, drawn green when true and red when false.
    Bool(bool),
    /// Any other value, already converted to its name.
    Text(String),
}

/// The block under the crosshair (`objectMouseOver` of type BLOCK).
#[derive(Debug, Clone, PartialEq)]
pub struct LookedAtBlock {
    /// The block position.
    pub pos: [i32; 3],
    /// The registry name, for example `minecraft:stone`.
    pub name: String,
    /// The actual state's properties in the reference order.
    pub properties: Vec<(String, PropertyValue)>,
}

/// Everything the debug screen prints, as plain numbers and strings.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugF3Input {
    /// Frames counted in the last second (`Minecraft.debugFPS`).
    pub fps: u32,
    /// Chunk meshes rebuilt in the last second (`RenderChunk.renderChunksUpdated`).
    pub chunk_updates: u32,
    /// The frame-rate limit, `None` for unlimited (printed `inf`).
    pub fps_limit: Option<u32>,
    /// Vertical sync is on.
    pub vsync: bool,
    /// Fancy graphics are on (off prints ` fast`).
    pub fancy_graphics: bool,
    /// Cloud setting: 0 off, 1 fast, 2 fancy.
    pub clouds: u8,
    /// Chunk meshes drawn (`C:` numerator).
    pub chunks_rendered: usize,
    /// Chunk meshes in the view frustum array (`C:` denominator).
    pub chunks_total: usize,
    /// The render distance in chunks (`D:`).
    pub render_distance: u32,
    /// Entities drawn (`E:` numerator).
    pub entities_rendered: usize,
    /// Entities considered (`E:` denominator).
    pub entities_total: usize,
    /// Entities culled by the frustum (`B:`).
    pub entities_hidden: usize,
    /// The live particle count (`EffectRenderer.getStatistics`).
    pub particles: usize,
    /// All loaded entities (`World.getDebugLoadedEntities`).
    pub loaded_entities: usize,
    /// Chunks in the client cache (`MultiplayerChunkCache` hash size, list size).
    pub chunk_cache: (usize, usize),
    /// The view entity's feet position (`posX`, `boundingBox.minY`, `posZ`).
    pub feet: [f64; 3],
    /// The view entity's yaw in degrees.
    pub yaw: f32,
    /// The view entity's pitch in degrees.
    pub pitch: f32,
    /// Biome and light, `None` while the chunk is not loaded.
    pub location: Option<DebugLocation>,
    /// The block under the crosshair, if any.
    pub looking_at: Option<LookedAtBlock>,
    /// The reduced debug info flag (`hasReducedDebug || reducedDebugInfo`).
    pub reduced: bool,
    /// Process memory in use, bytes.
    pub memory_used: u64,
    /// Process memory ceiling, bytes (0 prints 0 %).
    pub memory_max: u64,
    /// The window size in pixels.
    pub display: (u32, u32),
    /// The GPU name (wgpu adapter), in place of `GL_RENDERER`.
    pub gpu_name: String,
    /// The GPU vendor or driver, in place of `GL_VENDOR`.
    pub gpu_vendor: String,
    /// The graphics backend (Vulkan, Metal ...), in place of `GL_VERSION`.
    pub gpu_backend: String,
}

impl Default for DebugF3Input {
    fn default() -> Self {
        Self {
            fps: 0,
            chunk_updates: 0,
            fps_limit: None,
            vsync: false,
            fancy_graphics: true,
            clouds: 2,
            chunks_rendered: 0,
            chunks_total: 0,
            render_distance: 0,
            entities_rendered: 0,
            entities_total: 0,
            entities_hidden: 0,
            particles: 0,
            loaded_entities: 0,
            chunk_cache: (0, 0),
            feet: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            location: None,
            looking_at: None,
            reduced: false,
            memory_used: 0,
            memory_max: 0,
            display: (0, 0),
            gpu_name: String::new(),
            gpu_vendor: String::new(),
            gpu_backend: String::new(),
        }
    }
}

/// The two columns of the debug screen, top to bottom.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugF3Lines {
    /// The left column, drawn from `(2, 2)`.
    pub left: Vec<String>,
    /// The right column, right aligned at `width - 2`. Property lines carry
    /// [`GREEN`] / [`RED`] prefixes.
    pub right: Vec<String>,
}

impl DebugF3Lines {
    /// Builds both columns in the vanilla order.
    #[must_use]
    pub fn build(input: &DebugF3Input) -> Self {
        Self {
            left: left_lines(input),
            right: right_lines(input),
        }
    }
}

/// `MathHelper.wrapAngleTo180_float`.
fn wrap_angle_180(value: f32) -> f32 {
    let mut wrapped = value % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

/// The `Minecraft.debug` line: `%d fps (%d chunk update%s) T: %s%s%s%s`.
fn fps_line(input: &DebugF3Input) -> String {
    let plural = if input.chunk_updates == 1 { "" } else { "s" };
    let limit = input
        .fps_limit
        .map_or_else(|| "inf".to_owned(), |limit| limit.to_string());
    let clouds = match input.clouds {
        0 => "",
        1 => " fast-clouds",
        _ => " fancy-clouds",
    };
    format!(
        "{} fps ({} chunk update{plural}) T: {limit}{}{}{clouds}",
        input.fps,
        input.chunk_updates,
        if input.vsync { " vsync" } else { "" },
        if input.fancy_graphics { "" } else { " fast" },
    )
}

/// The lines both modes share, up to the empty separator.
fn common_head(input: &DebugF3Input) -> Vec<String> {
    vec![
        VERSION_LINE.to_owned(),
        fps_line(input),
        format!(
            "C: {}/{} D: {}",
            input.chunks_rendered, input.chunks_total, input.render_distance
        ),
        format!(
            "E: {}/{}, B: {}, I: {}",
            input.entities_rendered,
            input.entities_total,
            input.entities_hidden,
            input
                .entities_total
                .saturating_sub(input.entities_hidden + input.entities_rendered),
        ),
        format!("P: {}. T: All: {}", input.particles, input.loaded_entities),
        format!(
            "MultiplayerChunkCache: {}, {}",
            input.chunk_cache.0, input.chunk_cache.1
        ),
        String::new(),
    ]
}

/// The block position of the feet (`new BlockPos(posX, minY, posZ)`).
fn block_pos(feet: [f64; 3]) -> [i32; 3] {
    feet.map(|axis| axis.floor() as i32)
}

/// `GuiOverlayDebug.call`.
fn left_lines(input: &DebugF3Input) -> Vec<String> {
    let mut lines = common_head(input);
    let pos = block_pos(input.feet);
    let [x, y, z] = pos;
    if input.reduced {
        lines.push(format!("Chunk-relative: {} {} {}", x & 15, y & 15, z & 15));
        return lines;
    }
    let facing = Facing::from_yaw(input.yaw);
    lines.push(format!(
        "XYZ: {:.3} / {:.5} / {:.3}",
        input.feet[0], input.feet[1], input.feet[2]
    ));
    lines.push(format!("Block: {x} {y} {z}"));
    lines.push(format!(
        "Chunk: {} {} {} in {} {} {}",
        x & 15,
        y & 15,
        z & 15,
        x >> 4,
        y >> 4,
        z >> 4
    ));
    lines.push(format!(
        "Facing: {} ({}) ({:.1} / {:.1})",
        facing.name(),
        facing.towards(),
        wrap_angle_180(input.yaw),
        wrap_angle_180(input.pitch)
    ));
    if let Some(place) = &input.location {
        lines.push(format!("Biome: {}", place.biome));
        lines.push(format!(
            "Light: {} ({} sky, {} block)",
            place.light, place.sky_light, place.block_light
        ));
        lines.push(format!(
            "Local Difficulty: {:.2} (Day {})",
            place.local_difficulty, place.day
        ));
    }
    if let Some(block) = &input.looking_at {
        let [bx, by, bz] = block.pos;
        lines.push(format!("Looking at: {bx} {by} {bz}"));
    }
    lines
}

/// `Mem: % 2d%% %03d/%03dMB`; the space flag prints a leading space.
fn memory_line(used: u64, max: u64) -> String {
    let percent = if max == 0 { 0 } else { used * 100 / max };
    let megabytes = |bytes: u64| bytes / 1024 / 1024;
    format!(
        "Mem:  {percent}% {:03}/{:03}MB",
        megabytes(used),
        megabytes(max)
    )
}

/// The property lines of the looked-at block (`name: value`, booleans coloured).
fn block_lines(block: &LookedAtBlock) -> Vec<String> {
    let mut lines = vec![String::new(), block.name.clone()];
    for (name, value) in &block.properties {
        let mut line = format!("{name}: ");
        match value {
            PropertyValue::Bool(true) => write!(line, "{GREEN}true"),
            PropertyValue::Bool(false) => write!(line, "{RED}false"),
            PropertyValue::Text(text) => write!(line, "{text}"),
        }
        .ok();
        lines.push(line);
    }
    lines
}

/// `GuiOverlayDebug.getDebugInfoRight` with the GPU lines in place of GL.
fn right_lines(input: &DebugF3Input) -> Vec<String> {
    let mut lines = vec![
        memory_line(input.memory_used, input.memory_max),
        String::new(),
        format!(
            "Display: {}x{} ({})",
            input.display.0, input.display.1, input.gpu_vendor
        ),
        input.gpu_name.clone(),
        input.gpu_backend.clone(),
    ];
    if !input.reduced {
        if let Some(block) = &input.looking_at {
            lines.extend(block_lines(block));
        }
    }
    lines
}

#[cfg(test)]
mod tests;
