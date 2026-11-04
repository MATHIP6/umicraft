// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of loading, saving and clamping the options.

use winit::keyboard::KeyCode;

use super::*;

fn temp_folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("umicraft-options-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

#[test]
fn the_defaults_match_the_reference_game() {
    let options = Options::default();
    assert_eq!(options.video.fov, 70.0);
    assert_eq!(options.controls.mouse_sensitivity, 0.5);
    assert!((options.controls.look_factor() - 1.0).abs() < 1e-5);
    assert_eq!(options.keys.get(Action::Forward), KeyCode::KeyW);
    assert_eq!(options.keys.get(Action::Command), KeyCode::Slash);
    assert_eq!(options.keys.get(Action::Hotbar9), KeyCode::Digit9);
    assert_eq!(options.chat.visibility, ChatVisibility::Full);
    assert!(options.chat.colors);
}

#[test]
fn options_survive_a_restart() {
    let folder = temp_folder("restart");
    let path = folder.join(FILE_NAME);
    let mut options = Options::default();
    options.keys.set(Action::Forward, KeyCode::ArrowUp);
    options.keys.set(Action::Chat, KeyCode::KeyC);
    options.video.fov = 95.0;
    options.video.render_distance = 12;
    options.video.max_fps = UNLIMITED_FPS;
    options.controls.invert_mouse = true;
    options.controls.mouse_sensitivity = 0.9;
    options.chat.visibility = ChatVisibility::Commands;
    options.chat.colors = false;
    options.save(&path).unwrap();

    let loaded = Options::load(&path);
    assert_eq!(loaded, options);
    assert_eq!(loaded.keys.get(Action::Forward), KeyCode::ArrowUp);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_missing_file_gives_the_defaults() {
    let folder = temp_folder("missing");
    assert_eq!(Options::load(&folder.join(FILE_NAME)), Options::default());
}

#[test]
fn a_corrupt_file_gives_the_defaults_without_failing() {
    let folder = temp_folder("corrupt");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(&path, "this is [not toml").unwrap();
    assert_eq!(Options::load(&path), Options::default());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn one_broken_section_does_not_reset_the_others() {
    let folder = temp_folder("partial");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(
        &path,
        "[video]\nfov = \"wide\"\n[controls]\ninvert_mouse = true\n[keys]\nforward = \"ArrowUp\"\n",
    )
    .unwrap();
    let loaded = Options::load(&path);
    assert_eq!(loaded.video, VideoOptions::default(), "bad section resets");
    assert!(loaded.controls.invert_mouse, "good sections are kept");
    assert_eq!(loaded.keys.get(Action::Forward), KeyCode::ArrowUp);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn unknown_or_reserved_keys_in_the_file_keep_the_default() {
    let folder = temp_folder("keys");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(
        &path,
        "[keys]\nforward = \"Nonsense\"\njump = \"Escape\"\nsneak = \"KeyZ\"\nghost = \"KeyQ\"\n",
    )
    .unwrap();
    let loaded = Options::load(&path);
    assert_eq!(loaded.keys.get(Action::Forward), KeyCode::KeyW);
    assert_eq!(loaded.keys.get(Action::Jump), KeyCode::Space);
    assert_eq!(loaded.keys.get(Action::Sneak), KeyCode::KeyZ);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn out_of_range_numbers_are_clamped() {
    let folder = temp_folder("clamp");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(
        &path,
        "[video]\nrender_distance = 200\nfov = 500.0\nmax_fps = 1\n[controls]\nmouse_sensitivity = 7.0\n",
    )
    .unwrap();
    let loaded = Options::load(&path);
    assert_eq!(loaded.video.render_distance, 16);
    assert_eq!(loaded.video.fov, 110.0);
    assert_eq!(loaded.video.max_fps, 10);
    assert_eq!(loaded.controls.mouse_sensitivity, 1.0);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_old_file_without_the_cloud_keys_keeps_the_defaults() {
    let folder = temp_folder("clouds");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(&path, "[video]\nclouds = false\nrender_distance = 4\n").unwrap();
    let loaded = Options::load(&path);
    assert!(!loaded.video.clouds);
    assert!(loaded.video.fancy_graphics);
    assert_eq!(loaded.video.render_distance, 4);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_non_finite_number_returns_to_its_default() {
    let mut options = Options::default();
    options.video.fov = f32::NAN;
    options.controls.mouse_sensitivity = f32::INFINITY;
    options.clamp();
    assert_eq!(options.video.fov, 70.0);
    assert_eq!(options.controls.mouse_sensitivity, 0.5);
}

#[test]
fn saving_keeps_the_sections_this_version_does_not_know() {
    let folder = temp_folder("unknown-section");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(&path, "[future]\nanswer = 42\n").unwrap();
    Options::default().save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[future]") && text.contains("answer = 42"));
    assert!(text.contains("[keys]"));
    let _ = std::fs::remove_dir_all(&folder);
}
