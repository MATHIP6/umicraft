// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the newest frame shows: the crack mesh and the selection outline.

use super::*;

#[test]
fn a_changed_crack_set_rebuilds_the_crack_mesh_once() {
    let mut harness = Harness::new();
    harness.world_view.destroy_layers = Some([5; STAGES]);
    let crack = Crack {
        bounds: OutlineBox {
            min: [0.0, 64.0, 0.0],
            max: [1.0, 65.0, 1.0],
        },
        stage: 4,
    };
    let send = |harness: &mut Harness, cracks: &[Crack]| {
        let mut next = frame(glam::Vec3::ZERO, Instant::now(), 0);
        next.cracks = Arc::from(cracks);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(next))
            .expect("frame queue");
        harness.sync();
    };

    send(&mut harness, &[crack]);
    let (vertices, _) = harness.world_view.take_crack_mesh().expect("a mesh");
    assert_eq!(vertices.len(), 24);
    assert!(harness.world_view.take_crack_mesh().is_none());

    send(&mut harness, &[crack]);
    assert!(
        harness.world_view.take_crack_mesh().is_none(),
        "the same cracks need no new mesh"
    );

    send(&mut harness, &[]);
    let (vertices, indices) = harness.world_view.take_crack_mesh().expect("a clear");
    assert!(vertices.is_empty() && indices.is_empty());
}

#[test]
fn the_procedural_terrain_draws_no_cracks() {
    let mut harness = Harness::new();
    let mut next = frame(glam::Vec3::ZERO, Instant::now(), 0);
    next.cracks = Arc::from([Crack {
        bounds: OutlineBox {
            min: [0.0; 3],
            max: [1.0; 3],
        },
        stage: 0,
    }]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(next))
        .expect("frame queue");
    harness.sync();
    assert!(harness.world_view.take_crack_mesh().is_none());
}

#[test]
fn the_newest_frame_decides_the_selection_outline() {
    let mut harness = Harness::new();
    let outline = OutlineBox {
        min: [0.0; 3],
        max: [1.0; 3],
    };
    let mut selected = frame(glam::Vec3::ZERO, Instant::now(), 0);
    selected.selection = Arc::from([outline]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(selected))
        .expect("frame queue");
    harness.sync();
    assert_eq!(harness.world_view.selection(), &[outline]);

    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(
            glam::Vec3::ZERO,
            Instant::now(),
            0,
        )))
        .expect("frame queue");
    harness.sync();
    assert!(harness.world_view.selection().is_empty());
}
