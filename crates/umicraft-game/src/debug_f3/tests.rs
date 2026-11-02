// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the F3 debug lines against the formats of `GuiOverlayDebug`.

use super::*;

fn sample() -> DebugF3Input {
    DebugF3Input {
        fps: 143,
        chunk_updates: 1,
        fps_limit: Some(120),
        vsync: true,
        fancy_graphics: false,
        clouds: 1,
        chunks_rendered: 210,
        chunks_total: 441,
        render_distance: 10,
        entities_rendered: 4,
        entities_total: 9,
        entities_hidden: 3,
        particles: 17,
        loaded_entities: 12,
        chunk_cache: (441, 441),
        feet: [-12.345_678_9, 64.0, 100.5],
        yaw: 190.0,
        pitch: -12.34,
        location: Some(DebugLocation {
            biome: "Plains".into(),
            light: 15,
            sky_light: 15,
            block_light: 0,
            local_difficulty: 1.5,
            day: 3,
        }),
        looking_at: Some(LookedAtBlock {
            pos: [-13, 63, 100],
            name: "minecraft:lever".into(),
            properties: vec![
                ("facing".into(), PropertyValue::Text("north".into())),
                ("powered".into(), PropertyValue::Bool(true)),
                ("open".into(), PropertyValue::Bool(false)),
            ],
        }),
        reduced: false,
        memory_used: 300 * 1024 * 1024,
        memory_max: 1024 * 1024 * 1024,
        display: (854, 480),
        gpu_name: "Test GPU".into(),
        gpu_vendor: "Acme".into(),
        gpu_backend: "Vulkan".into(),
    }
}

#[test]
fn left_column_has_the_vanilla_order_and_formats() {
    let lines = DebugF3Lines::build(&sample()).left;
    let expected = [
        "Minecraft 1.8.8 (Umicraft)",
        "143 fps (1 chunk update) T: 120 vsync fast fast-clouds",
        "C: 210/441 D: 10",
        "E: 4/9, B: 3, I: 2",
        "P: 17. T: All: 12",
        "MultiplayerChunkCache: 441, 441",
        "",
        "XYZ: -12.346 / 64.00000 / 100.500",
        "Block: -13 64 100",
        "Chunk: 3 0 4 in -1 4 6",
        "Facing: north (Towards negative Z) (-170.0 / -12.3)",
        "Biome: Plains",
        "Light: 15 (15 sky, 0 block)",
        "Local Difficulty: 1.50 (Day 3)",
        "Looking at: -13 63 100",
    ];
    assert_eq!(lines, expected);
}

#[test]
fn fps_line_pluralises_and_prints_inf() {
    let mut input = sample();
    input.chunk_updates = 0;
    input.fps_limit = None;
    input.vsync = false;
    input.fancy_graphics = true;
    input.clouds = 2;
    assert_eq!(
        fps_line(&input),
        "143 fps (0 chunk updates) T: inf fancy-clouds"
    );
    input.clouds = 0;
    assert_eq!(fps_line(&input), "143 fps (0 chunk updates) T: inf");
}

#[test]
fn reduced_debug_keeps_only_the_chunk_relative_position() {
    let mut input = sample();
    input.reduced = true;
    let lines = DebugF3Lines::build(&input);
    assert_eq!(lines.left.len(), 8);
    assert_eq!(lines.left[7], "Chunk-relative: 3 0 4");
    assert_eq!(lines.right.len(), 5, "no looking-at block in reduced mode");
}

#[test]
fn right_column_has_memory_display_gpu_and_block_properties() {
    let lines = DebugF3Lines::build(&sample()).right;
    let expected = [
        "Mem:  29% 300/1024MB",
        "",
        "Display: 854x480 (Acme)",
        "Test GPU",
        "Vulkan",
        "",
        "minecraft:lever",
        "facing: north",
        "powered: \u{a7}atrue",
        "open: \u{a7}cfalse",
    ];
    assert_eq!(lines, expected);
}

#[test]
fn memory_line_pads_like_percent_space_2d() {
    assert_eq!(
        memory_line(5 * 1024 * 1024, 1024 * 1024 * 1024),
        "Mem:  0% 005/1024MB"
    );
    assert_eq!(memory_line(1, 0), "Mem:  0% 000/000MB");
    assert_eq!(
        memory_line(1024 * 1024 * 1024, 1024 * 1024 * 1024),
        "Mem:  100% 1024/1024MB"
    );
}

#[test]
fn missing_chunk_and_target_drop_their_lines() {
    let mut input = sample();
    input.location = None;
    input.looking_at = None;
    let lines = DebugF3Lines::build(&input);
    assert_eq!(lines.left.len(), 11);
    assert_eq!(lines.right.len(), 5);
}

#[test]
fn facing_follows_the_yaw_quadrants() {
    let cases = [
        (0.0, Facing::South),
        (89.0, Facing::West),
        (-90.0, Facing::East),
        (180.0, Facing::North),
        (-180.0, Facing::North),
        (44.9, Facing::South),
        (45.0, Facing::West),
    ];
    for (yaw, facing) in cases {
        assert_eq!(Facing::from_yaw(yaw), facing, "yaw {yaw}");
    }
    assert_eq!(Facing::East.towards(), "Towards positive X");
    assert_eq!(Facing::South.towards(), "Towards positive Z");
    assert_eq!(Facing::West.towards(), "Towards negative X");
}

#[test]
fn angles_wrap_into_half_open_range() {
    assert_eq!(wrap_angle_180(190.0), -170.0);
    assert_eq!(wrap_angle_180(180.0), -180.0);
    assert_eq!(wrap_angle_180(-540.0), -180.0);
    assert_eq!(wrap_angle_180(10.0), 10.0);
}
