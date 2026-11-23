// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: world marker (API 1.1).
//!
//! Shows how to draw on a point of the world: a `Frame` intercept (render
//! thread, the camera of the frame is final) projects the marked point with
//! `Frame::world_to_screen`, and a widget anchored with `Anchor::Screen` draws an
//! outline and the distance at those GUI pixels, right after, in the same frame.
//! The marker is dropped at the player's feet with a key binding; it shows
//! nothing the player could not see (spec §10).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umicraft_modapi::{umicraft_mod, Anchor, Frame, InitCx, KeyPressed, Mod, ModError, Placement};

/// The id of the key binding that drops the marker.
const MARK_KEY: &str = "mark";
/// Its default binding.
const MARK_BINDING: &str = "KeyM";
/// Half the side of the box, in GUI pixels.
const HALF_BOX: f32 = 8.0;
/// Colour of the box and the text.
const COLOR: u32 = 0x00FF_D700;
/// How high above the feet the marker floats, in blocks (about the waist).
const MARK_HEIGHT: f64 = 1.0;

/// What the handlers share: where the marker is and where it was seen.
#[derive(Debug, Default)]
struct Marker {
    /// The marked point; `None` until the key is pressed.
    point: Option<[f64; 3]>,
    /// The player's feet in the last frame, where the next marker goes.
    feet: [f64; 3],
    /// Where the point is on the screen this frame, and how far it is.
    on_screen: Option<([f32; 2], f64)>,
}

/// Tolerant of a panic in another handler: the values are plain data.
#[derive(Debug, Default, Clone)]
struct Shared(Arc<Mutex<Marker>>);

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Marker> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The distance between two points, in blocks.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Everything lives in the closures.
struct WorldMarker;

impl Mod for WorldMarker {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let shared = Shared::default();

        // The key arrives on the mod's worker: it only moves the marker.
        cx.key(MARK_KEY, "Drop the marker here", MARK_BINDING)?;
        let marker = shared.clone();
        cx.observe::<KeyPressed>(0, move |event| {
            if event.key() == MARK_KEY {
                let mut marker = marker.lock();
                let [x, y, z] = marker.feet;
                marker.point = Some([x, y + MARK_HEIGHT, z]);
            }
        })?;

        // An intercept runs on the render thread, before the widgets are drawn,
        // with the camera the frame is drawn with: the projection matches the
        // picture even while the view bobs or the field of view changes.
        let marker = shared.clone();
        cx.intercept::<Frame>(0, move |frame, _control| {
            let mut marker = marker.lock();
            marker.feet = frame.position();
            marker.on_screen = marker.point.and_then(|point| {
                let at = frame.world_to_screen(point)?;
                Some((at, distance(point, frame.eye())))
            });
        })?;

        // A widget on the screen: its coordinates are GUI pixels of the screen.
        cx.hud_widget(
            "marker",
            "World marker",
            Placement::new(Anchor::Screen),
            move |hud| {
                let Some(([x, y], blocks)) = shared.lock().on_screen else {
                    return;
                };
                let side = 2.0 * HALF_BOX;
                hud.outline(x - HALF_BOX, y - HALF_BOX, side, side, COLOR, 1.0);
                hud.segment(x, y + HALF_BOX, x, y + HALF_BOX + 4.0, COLOR, 1.0, 1.0);
                hud.label(
                    x,
                    y + HALF_BOX + 5.0,
                    &format!("{blocks:.1} m"),
                    COLOR,
                    1.0,
                    1.0,
                );
            },
        )?;
        Ok(Self)
    }
}

umicraft_mod!(WorldMarker);
