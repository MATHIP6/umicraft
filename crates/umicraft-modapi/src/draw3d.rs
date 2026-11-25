// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing in the world (API 1.4): the [`WorldCx`] a world layer fills each
//! frame, from [`InitCx::world_layer`](crate::InitCx::world_layer).

use core::marker::PhantomData;

use umicraft_modapi_sys::{
    UmiHost, UmiStatus, UmiWorldCx, UMI_WORLD_FILL, UMI_WORLD_THROUGH_WALLS,
};

/// Whether the blocks in front of a shape hide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Depth {
    /// Hidden by what stands in front of it, like the block outline.
    #[default]
    Tested,
    /// Drawn over the blocks in front of it.
    ThroughWalls,
}

impl Depth {
    fn flags(self) -> u32 {
        match self {
            Self::Tested => 0,
            Self::ThroughWalls => UMI_WORLD_THROUGH_WALLS,
        }
    }
}

/// The frame a world layer draws into; positions are world coordinates in
/// blocks, colours `0xRRGGBB` with a separate opacity.
///
/// Drawing is best effort, as on a [`Hud`](crate::Hud): a refused call (a
/// non-finite number, a full frame: 4096 lines, 4096 triangles and 256 marks per
/// mod) skips the rest of the frame, and the next frame starts clean.
#[derive(Debug)]
pub struct WorldCx<'a> {
    raw: *mut UmiWorldCx,
    host: UmiHost,
    failed: bool,
    frame: PhantomData<&'a mut UmiWorldCx>,
}

impl WorldCx<'_> {
    pub(crate) fn new(raw: *mut UmiWorldCx, host: UmiHost) -> Self {
        Self {
            raw,
            host,
            failed: false,
            frame: PhantomData,
        }
    }

    fn emit(&mut self, call: impl FnOnce(&UmiHost, *mut UmiWorldCx) -> UmiStatus) {
        if !self.failed {
            self.failed = !call(&self.host, self.raw).is_ok();
        }
    }

    /// Adds a line from `from` to `to`, `thickness` GUI pixels wide (0.05 to 8;
    /// under one screen pixel it is drawn one pixel wide and fainter).
    pub fn line(
        &mut self,
        from: [f64; 3],
        to: [f64; 3],
        color: u32,
        alpha: f32,
        thickness: f32,
        depth: Depth,
    ) {
        let ([x1, y1, z1], [x2, y2, z2]) = (from, to);
        self.emit(|host, raw| {
            (host.world_line)(
                raw,
                x1,
                y1,
                z1,
                x2,
                y2,
                z2,
                color,
                alpha,
                thickness,
                depth.flags(),
            )
        });
    }

    /// Adds the 12 edges of the box between the corners `a` and `b`.
    pub fn outline_box(&mut self, a: [f64; 3], b: [f64; 3], color: u32, alpha: f32, depth: Depth) {
        self.cuboid(a, b, color, alpha, depth.flags());
    }

    /// Adds the 6 faces of the box between the corners `a` and `b`, filled.
    pub fn filled_box(&mut self, a: [f64; 3], b: [f64; 3], color: u32, alpha: f32, depth: Depth) {
        self.cuboid(a, b, color, alpha, depth.flags() | UMI_WORLD_FILL);
    }

    fn cuboid(&mut self, a: [f64; 3], b: [f64; 3], color: u32, alpha: f32, flags: u32) {
        let ([x1, y1, z1], [x2, y2, z2]) = (a, b);
        self.emit(|host, raw| (host.world_box)(raw, x1, y1, z1, x2, y2, z2, color, alpha, flags));
    }

    /// Adds the filled convex polygon through `points` (3 to 64 of them).
    pub fn polygon(&mut self, points: &[[f64; 3]], color: u32, alpha: f32, depth: Depth) {
        self.emit(|host, raw| {
            (host.world_polygon)(
                raw,
                points.as_ptr().cast(),
                points.len(),
                color,
                alpha,
                depth.flags(),
            )
        });
    }

    /// Marks the entity `id` for the entity mask post passes read (`umi_entity_mask`,
    /// see `docs/modding/post.md`): its model is drawn there with this colour, only
    /// where it is visible with [`Depth::Tested`], whole with
    /// [`Depth::ThroughWalls`]. Without a post pass of the mod's own that reads the
    /// mask, nothing shows.
    pub fn highlight(&mut self, id: i32, color: u32, alpha: f32, depth: Depth) {
        self.emit(|host, raw| (host.world_highlight)(raw, id, color, alpha, depth.flags()));
    }
}
