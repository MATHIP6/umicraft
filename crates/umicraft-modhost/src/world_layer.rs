// SPDX-License-Identifier: MIT OR Apache-2.0

//! World layers (API 1.4, ADR-0097): geometry a mod draws in world coordinates
//! each frame, and the entities it marks for the mask of the post passes.
//!
//! A layer is declared during init like a widget. Once per frame in a world the
//! client calls [`ModHost::draw_world_layers`](crate::ModHost::draw_world_layers):
//! every enabled layer runs under the HUD budget and fills one shared
//! [`WorldDraw`] through the `world_*` host functions. A mod is capped per frame
//! (all its layers together), so one mod cannot crowd the others out of the
//! renderer's buffers.

use umicraft_modapi_sys::{UmiStatus, UmiWorldDrawFn, UMI_WORLD_FILL, UMI_WORLD_THROUGH_WALLS};

use crate::budget::HUD_BUDGET;
use crate::context::ModContext;
use crate::error::DeclareError;
use crate::ffi::{with_world_cx, UserPtr};
use crate::hook::run_hook;
use crate::ui::validate::check_key;

/// World layers one mod may declare.
pub const MAX_WORLD_LAYERS: usize = 4;
/// Lines one mod may add per frame; a box's edges count 12.
pub const MAX_WORLD_LINES: usize = 4096;
/// Triangles one mod may add per frame; a filled box counts 12, a polygon of `n`
/// points `n - 2`.
pub const MAX_WORLD_TRIANGLES: usize = 4096;
/// Entities one mod may mark per frame.
pub const MAX_HIGHLIGHTS: usize = 256;
/// Corners of a world polygon.
pub(crate) const POLYGON_POINTS: (usize, usize) = (3, 64);
/// Range of a line's width, in GUI pixels.
const LINE_THICKNESS: (f32, f32) = (0.05, 8.0);

/// A straight line in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldLine {
    /// One end, in blocks.
    pub from: [f64; 3],
    /// The other end, in blocks.
    pub to: [f64; 3],
    /// Straight-alpha RGBA.
    pub color: [f32; 4],
    /// Width in GUI pixels.
    pub thickness: f32,
    /// Drawn over the blocks in front of it.
    pub through_walls: bool,
}

/// A filled triangle in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldTriangle {
    /// The corners, in blocks.
    pub points: [[f64; 3]; 3],
    /// Straight-alpha RGBA.
    pub color: [f32; 4],
    /// Drawn over the blocks in front of it.
    pub through_walls: bool,
}

/// An entity marked for the entity mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Highlight {
    /// The server's entity id.
    pub entity: i32,
    /// Straight-alpha RGBA written into the mask.
    pub color: [f32; 4],
    /// Keeps the parts hidden behind blocks.
    pub through_walls: bool,
}

/// What the world layers drew this frame, in mod load order.
///
/// Kept and refilled by the client every frame: once its buffers have grown, a
/// frame allocates nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldDraw {
    /// The lines.
    pub lines: Vec<WorldLine>,
    /// The filled triangles.
    pub triangles: Vec<WorldTriangle>,
    /// The marked entities; for an entity marked twice the later mark wins.
    pub highlights: Vec<Highlight>,
}

impl WorldDraw {
    /// Forgets everything, keeping the buffers.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.triangles.clear();
        self.highlights.clear();
    }

    /// Whether nothing was drawn nor marked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.triangles.is_empty() && self.highlights.is_empty()
    }
}

/// What one mod may still add this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Room {
    lines: usize,
    triangles: usize,
    highlights: usize,
}

impl Room {
    fn full() -> Self {
        Self {
            lines: MAX_WORLD_LINES,
            triangles: MAX_WORLD_TRIANGLES,
            highlights: MAX_HIGHLIGHTS,
        }
    }
}

/// The target of the `world_*` host functions during one layer's draw: the shared
/// output and what the mod may still add.
pub(crate) struct WorldCall<'a> {
    pub(crate) out: &'a mut WorldDraw,
    pub(crate) room: &'a mut Room,
}

/// `0xRRGGBB` and an opacity as straight-alpha RGBA; the alpha byte a C mod may
/// leave in the high bits is dropped.
fn rgba(color: u32, alpha: f32) -> [f32; 4] {
    let channel = |shift: u32| ((color >> shift) & 0xFF) as f32 / 255.0;
    [channel(16), channel(8), channel(0), alpha.clamp(0.0, 1.0)]
}

fn finite(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite())
}

/// `OK` or the status of a refusal.
fn status(result: Result<(), UmiStatus>) -> UmiStatus {
    result.err().unwrap_or(UmiStatus::OK)
}

/// Takes `n` from `left`, or refuses (`DENIED`) without taking anything.
fn take(left: &mut usize, n: usize) -> Result<(), UmiStatus> {
    *left = left.checked_sub(n).ok_or(UmiStatus::DENIED)?;
    Ok(())
}

impl WorldCall<'_> {
    /// `world_line`.
    pub(crate) fn line(
        &mut self,
        [from, to]: [[f64; 3]; 2],
        color: u32,
        alpha: f32,
        thickness: f32,
        flags: u32,
    ) -> UmiStatus {
        if !finite(&from) || !finite(&to) || !alpha.is_finite() || !thickness.is_finite() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        status(take(&mut self.room.lines, 1).map(|()| {
            self.out.lines.push(WorldLine {
                from,
                to,
                color: rgba(color, alpha),
                thickness: thickness.clamp(LINE_THICKNESS.0, LINE_THICKNESS.1),
                through_walls: flags & UMI_WORLD_THROUGH_WALLS != 0,
            });
        }))
    }

    /// `world_box`: the 12 edges, or with `UMI_WORLD_FILL` the 6 faces as 12
    /// triangles. The corners may come in any order.
    pub(crate) fn cuboid(
        &mut self,
        [a, b]: [[f64; 3]; 2],
        color: u32,
        alpha: f32,
        flags: u32,
    ) -> UmiStatus {
        if !finite(&a) || !finite(&b) || !alpha.is_finite() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        let min: [f64; 3] = std::array::from_fn(|axis| a[axis].min(b[axis]));
        let max: [f64; 3] = std::array::from_fn(|axis| a[axis].max(b[axis]));
        let corner = |index: usize| -> [f64; 3] {
            std::array::from_fn(|axis| {
                if (index >> axis) & 1 == 1 {
                    max[axis]
                } else {
                    min[axis]
                }
            })
        };
        let color = rgba(color, alpha);
        let through_walls = flags & UMI_WORLD_THROUGH_WALLS != 0;
        if flags & UMI_WORLD_FILL != 0 {
            return status(take(&mut self.room.triangles, 12).map(|()| {
                for quad in BOX_FACES {
                    let [p, q, r, s] = quad.map(corner);
                    for points in [[p, q, r], [p, r, s]] {
                        self.out.triangles.push(WorldTriangle {
                            points,
                            color,
                            through_walls,
                        });
                    }
                }
            }));
        }
        status(take(&mut self.room.lines, 12).map(|()| {
            for from in 0..8_usize {
                for axis in 0..3 {
                    if (from >> axis) & 1 == 0 {
                        self.out.lines.push(WorldLine {
                            from: corner(from),
                            to: corner(from | (1 << axis)),
                            color,
                            thickness: BOX_EDGE_THICKNESS,
                            through_walls,
                        });
                    }
                }
            }
        }))
    }

    /// `world_polygon`: a fan of `points.len() - 2` triangles.
    pub(crate) fn polygon(
        &mut self,
        points: &[[f64; 3]],
        color: u32,
        alpha: f32,
        flags: u32,
    ) -> UmiStatus {
        let (fewest, most) = POLYGON_POINTS;
        if !(fewest..=most).contains(&points.len())
            || !alpha.is_finite()
            || !points.iter().all(|point| finite(point))
        {
            return UmiStatus::INVALID_ARGUMENT;
        }
        let color = rgba(color, alpha);
        let through_walls = flags & UMI_WORLD_THROUGH_WALLS != 0;
        status(take(&mut self.room.triangles, points.len() - 2).map(|()| {
            let first = points[0];
            for pair in points[1..].windows(2) {
                self.out.triangles.push(WorldTriangle {
                    points: [first, pair[0], pair[1]],
                    color,
                    through_walls,
                });
            }
        }))
    }

    /// `world_highlight`.
    pub(crate) fn highlight(
        &mut self,
        entity: i32,
        color: u32,
        alpha: f32,
        flags: u32,
    ) -> UmiStatus {
        if !alpha.is_finite() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        status(take(&mut self.room.highlights, 1).map(|()| {
            self.out.highlights.push(Highlight {
                entity,
                color: rgba(color, alpha),
                through_walls: flags & UMI_WORLD_THROUGH_WALLS != 0,
            });
        }))
    }
}

/// Width of the edges of a `world_box`, in GUI pixels: the selection outline's
/// two pixels at the usual GUI scale of 2.
const BOX_EDGE_THICKNESS: f32 = 1.0;

/// The six faces of a box as corner indices (bit 0: x = max, bit 1: y = max, bit
/// 2: z = max), each wound counter-clockwise seen from outside.
const BOX_FACES: [[usize; 4]; 6] = [
    [0, 4, 6, 2], // x = min
    [1, 3, 7, 5], // x = max
    [0, 1, 5, 4], // y = min
    [2, 6, 7, 3], // y = max
    [0, 2, 3, 1], // z = min
    [4, 5, 7, 6], // z = max
];

/// A declared world layer.
#[derive(Debug)]
pub(crate) struct WorldLayerSlot {
    key: String,
    draw: UmiWorldDrawFn,
    user: UserPtr,
    /// The module it is attached to: drawn only while that module is on.
    module: Option<usize>,
}

impl WorldLayerSlot {
    /// Validates and builds a layer.
    pub(crate) fn new(
        key: &str,
        draw: Option<UmiWorldDrawFn>,
        user: UserPtr,
    ) -> Result<Self, DeclareError> {
        check_key("world layer key", key)?;
        let draw = draw.ok_or(DeclareError::Invalid {
            what: "world layer",
            why: "the draw function must not be null",
        })?;
        Ok(Self {
            key: key.to_owned(),
            draw,
            user,
            module: None,
        })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// Attaches the layer to module `index`; `false` if it already is attached.
    pub(crate) fn attach(&mut self, index: usize) -> bool {
        self.module.replace(index).is_none()
    }

    pub(crate) fn module(&self) -> Option<usize> {
        self.module
    }

    /// Calls the draw function under [`HUD_BUDGET`]; a failure has already failed the
    /// mod when this returns `false`.
    fn draw(&self, owner: &ModContext, call: &mut WorldCall<'_>) -> bool {
        run_hook(
            owner,
            owner.hud_counters(),
            HUD_BUDGET,
            format_args!("its world layer `{}`", self.key),
            || with_world_cx(call, |cx| (self.draw)(cx, self.user.as_ptr())),
        )
    }
}

impl ModContext {
    /// Declares a world layer.
    ///
    /// # Errors
    ///
    /// [`DeclareError::Late`] after init, [`DeclareError::TooMany`] past
    /// [`MAX_WORLD_LAYERS`], [`DeclareError::Duplicate`] for a key already used,
    /// [`DeclareError::Invalid`] for a bad key or a null draw function.
    pub fn register_world_layer(
        &self,
        key: &str,
        draw: Option<UmiWorldDrawFn>,
        user: UserPtr,
    ) -> Result<(), DeclareError> {
        self.ui
            .register(|declared| declared.add_world_layer(WorldLayerSlot::new(key, draw, user)?))
    }

    /// Runs this mod's world layers into `out`, unless the mod is not enabled, but a
    /// layer whose module is off; stops at the first failure, since a failed mod is
    /// skipped from then on.
    ///
    /// A mod that fails during a layer leaves nothing of this frame in `out`: what
    /// its earlier layers added is taken back.
    pub fn draw_world_layers(&self, out: &mut WorldDraw) {
        let declared = self.ui.declared();
        let mut room = Room::full();
        let marks = (out.lines.len(), out.triangles.len(), out.highlights.len());
        for layer in declared.world_layers() {
            if !self.is_enabled() {
                return;
            }
            if !declared.shows(layer.module()) {
                continue;
            }
            let mut call = WorldCall {
                out: &mut *out,
                room: &mut room,
            };
            if !layer.draw(self, &mut call) {
                out.lines.truncate(marks.0);
                out.triangles.truncate(marks.1);
                out.highlights.truncate(marks.2);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
