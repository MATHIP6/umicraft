// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the animation file parser and the frame clock.

use super::*;

const PATH: &str = "assets/minecraft/optifine/anim/block.properties";

fn parse(text: &str) -> Result<AnimSpec, AnimError> {
    let props = Properties::parse(text.as_bytes()).expect("properties");
    AnimSpec::parse(&props, PATH)
}

const BASE: &str = "from=./frames.png\nto=textures/blocks/stone.png\nx=2\ny=4\nw=16\nh=8\n";

#[test]
fn paths_resolve_against_the_file() {
    let spec = parse(BASE).expect("valid");
    assert_eq!(spec.from, "assets/minecraft/optifine/anim/frames.png");
    assert_eq!(spec.to, "assets/minecraft/textures/blocks/stone.png");
    assert_eq!((spec.x, spec.y, spec.width, spec.height), (2, 4, 16, 8));
}

#[test]
fn missing_textures_and_bad_rectangles_are_rejected() {
    assert_eq!(parse("x=0\ny=0\nw=1\nh=1"), Err(AnimError::MissingTexture));
    for broken in ["x=-1", "y=abc", "w=0", "h="] {
        let text = format!("from=a.png\nto=b.png\nx=0\ny=0\nw=1\nh=1\n{broken}");
        assert_eq!(parse(&text), Err(AnimError::InvalidRectangle), "{broken}");
    }
    assert_eq!(
        parse("from=a.png\nto=b.png\nx=0\ny=0\nw=1"),
        Err(AnimError::InvalidRectangle)
    );
}

#[test]
fn without_tiles_every_frame_plays_once_in_order() {
    let spec = parse(&format!("{BASE}duration=3\n")).expect("valid");
    let steps = spec.schedule(32).expect("four frames of eight rows");
    let frames: Vec<_> = steps.iter().map(|s| (s.frame, s.duration)).collect();
    assert_eq!(frames, [(0, 3), (1, 3), (2, 3), (3, 3)]);
}

#[test]
fn the_default_duration_is_one_tick() {
    let spec = parse(BASE).expect("valid");
    assert!(spec
        .schedule(16)
        .expect("two frames")
        .iter()
        .all(|s| s.duration == 1));
}

#[test]
fn tiles_set_the_order_and_durations_override() {
    let text = format!("{BASE}duration=2\ntile.0=2\ntile.1=0\ntile.2=x\nduration.1=9\ntile.4=1\n");
    let steps = parse(&text)
        .expect("valid")
        .schedule(24)
        .expect("three frames");
    // `tile.4` is ignored: the run stops at the first missing key, and `x`
    // falls back to its step number.
    let frames: Vec<_> = steps.iter().map(|s| (s.frame, s.duration)).collect();
    assert_eq!(frames, [(2, 2), (0, 9), (2, 2)]);
}

#[test]
fn a_source_that_is_not_whole_frames_is_rejected() {
    let spec = parse(BASE).expect("valid");
    assert_eq!(
        spec.schedule(20),
        Err(AnimError::PartialFrame {
            height: 20,
            frame_height: 8
        })
    );
    assert_eq!(spec.schedule(0), Err(AnimError::NoFrames));
}

#[test]
fn the_clock_holds_each_step_for_its_duration_and_wraps() {
    let steps = vec![
        Step {
            frame: 0,
            duration: 2,
        },
        Step {
            frame: 1,
            duration: 1,
        },
    ];
    let mut clock = FrameClock::new(steps);
    assert_eq!(clock.current_frame(), 0);
    let shown: Vec<_> = (0..6).map(|_| clock.tick()).collect();
    assert_eq!(shown, [None, Some(1), Some(0), None, Some(1), Some(0)]);
}

#[test]
fn a_zero_duration_changes_every_tick_and_an_empty_clock_never() {
    let mut clock = FrameClock::new(vec![
        Step {
            frame: 5,
            duration: 0,
        },
        Step {
            frame: 6,
            duration: 0,
        },
    ]);
    assert_eq!(clock.tick(), Some(6));
    assert_eq!(clock.tick(), Some(5));
    let mut empty = FrameClock::new(Vec::new());
    assert_eq!(empty.tick(), None);
    assert_eq!(empty.current_frame(), 0);
}

proptest::proptest! {
    #[test]
    fn arbitrary_files_never_panic(
        bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..256),
        height in 0u32..100_000,
    ) {
        if let Ok(props) = Properties::parse(&bytes) {
            if let Ok(spec) = AnimSpec::parse(&props, PATH) {
                if let Ok(steps) = spec.schedule(height) {
                    let mut clock = FrameClock::new(steps);
                    for _ in 0..8 {
                        let _ = clock.tick();
                    }
                }
            }
        }
    }
}
