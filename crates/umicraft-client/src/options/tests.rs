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
    options.language.code = "de_DE".to_string();
    options.multiplayer.last_server = "play.example.net:25570".to_string();
    options.menu.title_style = TitleStyleSetting::Classic;
    options.save(&path).unwrap();

    let loaded = Options::load(&path);
    assert_eq!(loaded, options);
    assert_eq!(loaded.multiplayer.last_server, "play.example.net:25570");
    assert_eq!(loaded.keys.get(Action::Forward), KeyCode::ArrowUp);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn mouse_bindings_survive_a_restart_and_an_old_file_keeps_the_defaults() {
    let folder = temp_folder("mouse");
    let path = folder.join(FILE_NAME);
    let mut options = Options::default();
    options.keys.set(Action::Attack, KeyCode::KeyJ);
    options.keys.set(Action::Sprint, Binding::Mouse(4));
    options.save(&path).unwrap();
    let loaded = Options::load(&path);
    assert_eq!(loaded.keys.get(Action::Attack), KeyCode::KeyJ);
    assert_eq!(loaded.keys.get(Action::Sprint), Binding::Mouse(4));

    // Written before mouse bindings existed: no attack, use or pick entries.
    std::fs::write(&path, "[keys]\nforward = \"KeyI\"\njump = \"Space\"\n").unwrap();
    let old = Options::load(&path);
    assert_eq!(old.keys.get(Action::Forward), KeyCode::KeyI);
    assert_eq!(old.keys.get(Action::Attack), Binding::Mouse(0));
    assert_eq!(old.keys.get(Action::Use), Binding::Mouse(1));
    assert_eq!(old.keys.get(Action::PickBlock), Binding::Mouse(2));
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
    assert_eq!(loaded.video.clouds, 0);
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

#[test]
fn the_language_defaults_to_english_and_a_bad_code_is_replaced() {
    assert_eq!(Options::default().language.code, "en_US");
    let folder = temp_folder("language");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(&path, "[language]\ncode = \"../../etc\"\n").unwrap();
    assert_eq!(Options::load(&path).language.code, "en_US");
    std::fs::write(&path, "[language]\ncode = \"fr_FR\"\n").unwrap();
    assert_eq!(Options::load(&path).language.code, "fr_FR");
    let _ = std::fs::remove_dir_all(&folder);
}

/// Regression: the sound section keeps its field names now that the category
/// enum lives in `umicraft-audio` (a file written before must still load).
#[test]
fn the_sound_section_keeps_its_file_format() {
    let folder = temp_folder("sound-format");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE_NAME);
    std::fs::write(
        &path,
        "[sound]\nmaster = 0.5\nmusic = 0.25\nrecords = 1.0\nweather = 0.75\nblocks = 0.0\nmobs = 1.0\nanimals = 1.0\nplayers = 1.0\nambient = 0.125\n",
    )
    .unwrap();
    let loaded = Options::load(&path);
    assert_eq!(loaded.sound.volume(SoundCategory::Master), 0.5);
    assert_eq!(loaded.sound.volume(SoundCategory::Blocks), 0.0);
    assert_eq!(loaded.sound.volume(SoundCategory::Ambient), 0.125);
    loaded.save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for field in [
        "master", "music", "records", "weather", "blocks", "mobs", "animals", "players", "ambient",
    ] {
        assert!(text.contains(&format!("{field} = ")), "{field} in {text}");
    }
    assert_eq!(Options::load(&path).sound, loaded.sound);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_title_style_lives_in_the_menu_section() {
    let folder = temp_folder("menu");
    let path = folder.join(FILE_NAME);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&path, "[menu]\ntitle_style = \"classic\"\n").unwrap();
    assert_eq!(
        Options::load(&path).menu.title_style,
        TitleStyleSetting::Classic
    );
    std::fs::write(&path, "[video]\nfov = 80.0\n").unwrap();
    assert_eq!(Options::load(&path).menu, MenuOptions::default());
    let _ = std::fs::remove_dir_all(&folder);
}
