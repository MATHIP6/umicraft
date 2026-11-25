// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loads the real `hello` and `panicking` example cdylibs (F3.2 acceptance test) and
//! dispatches events to them through a core `EventBus` (F3.3); `packet_logger`
//! observes and injects packets (F3.5); `physics_tweak` wraps the player physics and
//! queries the world (F3.6); `shader_pass` and `depth_fog` register validated post-process passes
//! (F3.7).
//!
//! The libraries are built once per test binary with `cargo build --offline`. The
//! target directory is `CARGO_TARGET_DIR` when set, otherwise it is derived from the
//! test executable's location (`<target>/<profile>/deps/<test>`): that stays correct
//! for custom profiles and avoids parsing `cargo metadata` output.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]

use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use umicraft_builtin_mods::SettingValue;
use umicraft_core::EventBus;
use umicraft_modapi_sys::{
    UmiCameraInput, UmiEventId, UmiMode, UmiPhysicsInput, UmiPlayerPhysics, UmiRenderCamera,
    UmiStatus, UMI_ENTITY_LIVING,
};
use umicraft_modhost::{
    discover, library_file_name, platform_dir, resolve, with_entity_snapshot, with_world,
    ApiVersion, Arch, Candidate, ChatSend, CommandOutcome, EntitySnapshot, EntityView, EventThread,
    Frame, FrameCamera, FrameFacts, KeyPressed, LoadError, ModHost, ModId, Os, PacketReceived,
    PostTick, PreTick, TrustList, WorldQuery,
};

/// The directory holding the built example libraries (`<target>/<profile>`).
fn built_libraries() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let status = Command::new(env!("CARGO"))
            .args([
                "build",
                "-p",
                "umicraft-example-hello",
                "-p",
                "umicraft-example-panicking",
                "-p",
                "umicraft-example-packet-logger",
                "-p",
                "umicraft-example-physics-tweak",
                "-p",
                "umicraft-example-shader-pass",
                "-p",
                "umicraft-example-depth-fog",
                "-p",
                "umicraft-example-entity-list",
                "-p",
                "umicraft-example-camera-orbit",
                "-p",
                "umicraft-example-suite",
                "-p",
                "umicraft-example-free-cam",
                "--offline",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "building the example mods failed");
        profile_dir()
    })
}

/// `<target>/debug`, where `cargo build` (dev profile) puts the cdylibs.
fn profile_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || {
            let exe = std::env::current_exe().unwrap();
            // <target>/<profile>/deps/<test> -> <target>
            exe.ancestors().nth(3).unwrap().to_path_buf()
        },
        PathBuf::from,
    );
    target.join("debug")
}

/// A scratch mods directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        // One directory per test: the tests of this file run in parallel.
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "umi-real-mods-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Installs example `id` into `mods_dir` the way a player would.
fn install(mods_dir: &Path, id: &str) {
    let (os, arch) = (Os::current().unwrap(), Arch::current().unwrap());
    let file = library_file_name(id, os);
    let dir = mods_dir.join(id);
    let bin = dir.join("bin").join(platform_dir(os, arch));
    fs::create_dir_all(&bin).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../mods/examples/{id}"));
    fs::copy(source.join("mod.toml"), dir.join("mod.toml")).unwrap();
    fs::copy(built_libraries().join(&file), bin.join(&file)).unwrap();
}

fn id(text: &str) -> ModId {
    text.to_owned().try_into().unwrap()
}

#[test]
fn hello_loads_panicking_fails_and_handlers_run() {
    let tmp = TempDir::new();
    install(&tmp.0, "hello");
    install(&tmp.0, "panicking");

    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    assert_eq!(resolution.order.len(), 2);

    let mut trust = TrustList::default();
    let mut asked = Vec::new();
    let (host, failures) = ModHost::load(&resolution, &tmp.0, &mut trust, |m, hash| {
        asked.push((m.id.to_string(), hash.to_owned()));
        true
    });

    // The panic is contained: only `panicking` failed, and the process is alive.
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].id.as_str(), "panicking");
    assert!(matches!(
        failures[0].reason,
        LoadError::InitFailed(UmiStatus::PANICKED)
    ));
    let enabled: Vec<_> = host.enabled().map(ModId::as_str).collect();
    assert_eq!(enabled, ["hello"]);

    // Both libraries were put to the player, and both hashes are now trusted.
    assert_eq!(asked.len(), 2);
    for (mod_id, hash) in &asked {
        assert!(trust.is_trusted(&id(mod_id), hash));
    }

    let pre: Vec<_> = host.subscriptions(UmiEventId::PRE_TICK).collect();
    assert_eq!(pre.len(), 1);
    assert_eq!(pre[0].mode, UmiMode::OBSERVE);
    let post: Vec<_> = host.subscriptions(UmiEventId::POST_TICK).collect();
    assert_eq!(post.len(), 1);
    assert_eq!(post[0].mode, UmiMode::INTERCEPT);
    assert_eq!(post[0].priority, 10);

    // The handlers are installed on the game bus only: no render event is hooked.
    let mut game = EventBus::default();
    host.install(&mut game, EventThread::Game);
    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    assert!(game.has_subscribers::<PreTick>() && game.has_subscribers::<ChatSend>());
    assert!(!render.has_subscribers::<PreTick>());

    assert!(!game.dispatch(&mut PreTick::new(7)));
    assert!(!game.dispatch(&mut PostTick::new(7)));
    let mut chat = ChatSend::new("hi :shrug:");
    assert!(!game.dispatch(&mut chat));
    assert_eq!(
        chat.text(),
        "hi \u{af}\\_(\u{30c4})_/\u{af}",
        "hello rewrote the line"
    );
    let stats = host.stats();
    let hello = &stats
        .iter()
        .find(|(id, _)| id.as_str() == "hello")
        .unwrap()
        .1;
    assert_eq!(hello.calls, 2, "the post-tick and the chat interceptors");

    // Switching off is a flag: the mod stops being dispatched to, and can come back.
    host.set_enabled(&id("hello"), false).unwrap();
    let mut untouched = ChatSend::new(":shrug:");
    game.dispatch(&mut untouched);
    assert_eq!(untouched.text(), ":shrug:");
    assert_eq!(host.subscriptions(UmiEventId::PRE_TICK).count(), 0);
    assert_eq!(host.enabled().count(), 0);
    host.set_enabled(&id("hello"), true).unwrap();
    let mut back = ChatSend::new(":shrug:");
    game.dispatch(&mut back);
    assert_ne!(back.text(), ":shrug:");
}

/// What a player of the real `hello` mod sees: settings, a widget, a key, a command.
#[test]
fn hello_declares_settings_a_widget_a_key_and_a_command() {
    let tmp = TempDir::new();
    install(&tmp.0, "hello");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let hello = id("hello");

    let view = host.mods().next().unwrap();
    assert_eq!(
        (view.name, view.enabled, view.failed),
        ("Hello", true, false)
    );
    assert_eq!(view.sha256.len(), 64);
    assert_eq!(view.failure, None);
    let profile = host.profile();
    assert_eq!(profile[0].id, hello);
    assert_eq!(profile[0].state, umicraft_modhost::ModState::Enabled);
    let keys: Vec<_> = view.settings().map(|s| s.spec.key.clone()).collect();
    assert_eq!(keys, ["show_ticks", "tick_color"]);
    let bindings: Vec<_> = host.key_bindings().collect();
    assert_eq!(bindings.len(), 1);
    assert_eq!(
        (bindings[0].key, bindings[0].default_binding),
        ("greet", "KeyH")
    );

    // The widget draws a line on the render thread.
    host.draw_widgets();
    let mut lines = Vec::new();
    host.visit_widgets(|w| {
        lines.extend(w.content.lines().iter().map(|l| l.text.clone()));
        assert_eq!((w.mod_id.as_str(), w.key), ("hello", "ticks"));
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("ticks: "), "{lines:?}");

    // A setting changes what the mod's closure reads.
    host.set_setting(&hello, "show_ticks", &SettingValue::Bool(false))
        .unwrap();
    host.draw_widgets();
    let mut shown = 0;
    host.visit_widgets(|w| shown += w.content.lines().len());
    assert_eq!(shown, 0);

    // The key binding fires an observer on the render thread's worker, which prints.
    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    render.dispatch(&mut KeyPressed::new(hello.clone(), "greet"));
    let outbox = host.chat_outbox();
    let line = outbox.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(line, "hello from the key binding");

    // `/hello bob` runs on the client and prints; `/hello` alone is refused with
    // its usage, and the mod stays enabled.
    let commands = host.commands();
    assert_eq!(commands.complete("he"), ["hello"]);
    assert_eq!(commands.run("/hello bob"), CommandOutcome::Ran);
    assert_eq!(outbox.try_recv().as_deref(), Ok("hello, bob"));
    assert_eq!(commands.run("/hello"), CommandOutcome::Rejected);
    assert!(outbox
        .try_recv()
        .is_ok_and(|line| line.starts_with("Usage: /hello")));
    assert_eq!(host.set_enabled(&hello, true), Ok(()));
    assert!(host.enabled().any(|id| id == &hello));
}

/// The real `packet_logger`: it observes packets on the network bus, `/swing`
/// injects a validated Animation, and the server-compatible mode holds it off.
#[test]
fn packet_logger_observes_injects_and_yields_to_the_compatible_mode() {
    let tmp = TempDir::new();
    install(&tmp.0, "packet_logger");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let view = host.mods().next().unwrap();
    assert!(view.alters_game && view.enabled && !view.held_by_compat);

    let mut network = EventBus::default();
    host.install(&mut network, EventThread::Network);
    assert!(network.has_subscribers::<PacketReceived>());
    assert!(!network.dispatch(&mut PacketReceived::new(0x03, &[0; 16])));

    let outbox = host.packet_outbox();
    let chat = host.chat_outbox();
    assert_eq!(host.commands().run("/swing"), CommandOutcome::Ran);
    assert_eq!(chat.try_recv().as_deref(), Ok("swing sent"));
    let mut injected = Vec::new();
    outbox.drain(&mut injected);
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0].packet_id(), 0x0A);

    host.set_server_compat(true);
    assert!(host.mods().next().unwrap().held_by_compat);
    assert_eq!(host.commands().run("/swing"), CommandOutcome::NotACommand);
    host.set_server_compat(false);
    assert_eq!(host.commands().run("/swing"), CommandOutcome::Ran);
    host.shutdown_all();
}

/// A flat world of stone (id 1) below y = 64 with one other player.
struct FlatWorld;

impl WorldQuery for FlatWorld {
    fn block(&self, _x: i32, y: i32, _z: i32) -> Option<u16> {
        Some(if y < 64 { 1 << 4 } else { 0 })
    }

    fn visit_entities(&self, visit: &mut dyn FnMut(&EntityView<'_>) -> bool) {
        visit(&EntityView {
            id: 7,
            kind: "player",
            position: [3.0, 64.0, 4.0],
            yaw: 0.0,
            pitch: 0.0,
            name: "Steve",
            health: 20.0,
            max_health: 20.0,
            armor: 0.0,
            team: "",
            width: 0.6,
            height: 1.8,
            velocity: [0.0; 3],
            flags: UMI_ENTITY_LIVING,
            is_player: true,
            render_position: [3.0, 64.0, 4.0],
            ..EntityView::default()
        });
    }
}

fn falling(y: f64) -> UmiPlayerPhysics {
    UmiPlayerPhysics {
        size: std::mem::size_of::<UmiPlayerPhysics>() as u32,
        flags: 0,
        position: [0.0, y, 0.0],
        velocity: [0.0, -1.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
        fall_distance: 0.0,
        fly_speed: 0.05,
        game_mode: 0,
    }
}

/// The real `physics_tweak`: wraps the step without changing it, and reads the world from its game-thread callbacks into its widget.
#[test]
fn physics_tweak_wraps_the_step_and_queries_the_world() {
    let tmp = TempDir::new();
    install(&tmp.0, "physics_tweak");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let chain = host.physics().unwrap();
    assert!(chain.is_active());
    let input = UmiPhysicsInput {
        size: std::mem::size_of::<UmiPhysicsInput>() as u32,
        flags: 0,
        forward: 0.0,
        strafe: 0.0,
        yaw: 0.0,
        pitch: 0.0,
    };
    let mut vanilla_runs = 0;
    let mut vanilla = |state: &mut UmiPlayerPhysics| {
        vanilla_runs += 1;
        state.position[0] += 0.2;
        state.position[1] += state.velocity[1];
    };

    // The wrapper only measures: after each step the state is what vanilla made of it.
    let mut state = falling(70.0);
    let mut game = EventBus::default();
    host.install(&mut game, EventThread::Game);
    with_world(&FlatWorld, || {
        for step in 1..=2 {
            chain.step(&mut state, &input, &mut vanilla);
            assert_eq!(state.velocity, [0.0, -1.0, 0.0]);
            assert_eq!(state.position[1], 70.0 - f64::from(step));
            assert!((state.position[0] - 0.2 * f64::from(step)).abs() < 1e-9);
        }
        // The PostTick intercept queries the world (lent on this thread) for the widget.
        game.dispatch(&mut PostTick::new(1));
    });
    assert_eq!(vanilla_runs, 2);
    host.draw_widgets();
    let mut lines = Vec::new();
    host.visit_widgets(|w| lines.extend(w.content.lines().iter().map(|l| l.text.clone())));
    // Feet at (0.4, 68, 0): air below, Steve at (3, 64, 4) is 6.2 m away.
    assert_eq!(
        lines,
        [
            "speed: 4.0 blocks/s",
            "last jump: none",
            "under you: 0:0",
            "nearest player: Steve (6.2 m)"
        ]
    );

    // The server-compatible mode holds a movement mod off.
    host.set_server_compat(true);
    assert!(!chain.is_active());
    host.shutdown_all();
}

/// The real `shader_pass`: its WGSL passes the host's validation and its
/// `PreTick` observer pushes the settings into the pass parameters.
#[test]
fn shader_pass_registers_a_valid_pass_and_feeds_its_params() {
    let tmp = TempDir::new();
    install(&tmp.0, "shader_pass");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let passes: Vec<_> = host
        .post_passes()
        .map(|p| (p.mod_id.to_string(), p.key.to_owned(), p.enabled))
        .collect();
    assert_eq!(
        passes,
        [("shader_pass".to_owned(), "sepia".to_owned(), true)]
    );
    assert!(host
        .post_passes()
        .next()
        .unwrap()
        .wgsl
        .contains("fn umi_vs"));

    host.set_setting(&id("shader_pass"), "strength", &SettingValue::Number(0.25))
        .unwrap();
    let mut game = EventBus::default();
    host.install(&mut game, EventThread::Game);
    game.dispatch(&mut PreTick::new(0));
    // The observer runs on the mod's worker; wait for the parameter to land.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while host.post_passes().next().unwrap().params()[0][0] != 0.25 {
        assert!(std::time::Instant::now() < deadline, "params never set");
        std::thread::yield_now();
    }
    assert_eq!(host.post_passes().next().unwrap().params()[0][1], 1.0);

    host.refuse_post_pass(&id("shader_pass"), "sepia", "test");
    assert!(!host.post_passes().next().unwrap().enabled);
    host.shutdown_all();
}

/// The real `depth_fog` (API 1.1): its WGSL reads `umi_linear_depth`, passes the host's
/// validation and gets its settings as `umi.params[0]`.
#[test]
fn depth_fog_registers_a_pass_that_reads_the_scene_depth() {
    let tmp = TempDir::new();
    install(&tmp.0, "depth_fog");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let pass = host.post_passes().next().expect("one pass");
    assert_eq!((pass.key, pass.enabled), ("fog", true));
    assert!(pass.wgsl.contains("umi_linear_depth(in.uv)"));
    assert!(pass.wgsl.contains("var umi_depth: texture_depth_2d"));

    host.set_setting(&id("depth_fog"), "start", &SettingValue::Number(96.0))
        .unwrap();
    let mut game = EventBus::default();
    host.install(&mut game, EventThread::Game);
    game.dispatch(&mut PreTick::new(0));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while host.post_passes().next().unwrap().params()[0][0] != 96.0 {
        assert!(std::time::Instant::now() < deadline, "params never set");
        std::thread::yield_now();
    }
    host.shutdown_all();
}

/// An entity of the snapshot of the `entity_list` test, `x` blocks east of the origin.
fn listed(id: i32, kind: &'static str, name: &'static str, health: f32) -> EntityView<'static> {
    let living = kind != "item";
    EntityView {
        id,
        kind,
        position: [f64::from(id), 64.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
        name,
        health: if living { health } else { 0.0 },
        max_health: if living { 20.0 } else { 0.0 },
        armor: 0.0,
        team: "",
        width: 0.6,
        height: 1.8,
        velocity: [0.0; 3],
        flags: if living { UMI_ENTITY_LIVING } else { 0 },
        is_player: kind == "player",
        render_position: [f64::from(id), 64.0, 0.0],
        ..EntityView::default()
    }
}

/// The real `entity_list` (API 1.1): its widget reads the render-thread snapshot
/// inside the scope the client opens around the `Frame` event and the widget draws.
#[test]
fn entity_list_draws_the_snapshot_inside_the_scope() {
    let tmp = TempDir::new();
    install(&tmp.0, "entity_list");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);

    // Twelve entities, nearest first; only ten are listed.
    let mut snapshot = EntitySnapshot::new();
    snapshot.begin();
    snapshot.push(&listed(1, "player", "Ann", 10.0));
    snapshot.push(&listed(2, "item", "", 0.0));
    for id in 3..=12 {
        snapshot.push(&listed(id, "zombie", "", 20.0));
    }
    let facts = FrameFacts {
        now_ms: 0,
        position: [0.0, 64.0, 0.0],
        fps: 60,
        frame_ms: 16.0,
        yaw: 0.0,
        pitch: 0.0,
        left_clicks: 0,
        right_clicks: 0,
        ping_ms: None,
        sprinting: false,
        camera: FrameCamera {
            eye: [0.0, 64.0, 0.0],
            ..FrameCamera::default()
        },
    };
    with_entity_snapshot(&snapshot, || {
        render.dispatch(&mut Frame::new(facts));
        host.draw_widgets();
    });
    let mut labels = Vec::new();
    let mut bars = 0;
    host.visit_widgets(|w| {
        labels.extend(w.content.labels().iter().map(|l| l.text.clone()));
        bars = w.content.ticks().len();
    });
    assert_eq!(labels.len(), 10);
    assert_eq!(labels[0], "Ann 1.0m");
    assert_eq!(labels[1], "item 2.0m");
    assert_eq!(labels[9], "zombie 10.0m");
    // A background and a fill for the nine living ones, none for the item.
    assert_eq!(bars, 9 * 2);

    // Outside the scope the snapshot is unsupported: the widget shows nothing.
    host.draw_widgets();
    host.visit_widgets(|w| assert!(w.content.labels().is_empty()));
    host.shutdown_all();
}

#[test]
fn camera_orbit_wraps_the_render_camera_without_the_compat_mode_holding_it() {
    let tmp = TempDir::new();
    install(&tmp.0, "camera_orbit");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    host.set_server_compat(true);
    let chain = host.camera().expect("one camera layer");
    assert!(
        chain.is_active(),
        "a camera mod is not held by server-compat"
    );

    // The orbit is off until its key: the camera stays as the client put it.
    let built_in = UmiRenderCamera {
        size: size_of::<UmiRenderCamera>() as u32,
        flags: 0,
        eye: [0.5, 65.62, 0.5],
        yaw: 0.0,
        pitch: 0.0,
        fov_y: 70.0,
        aspect: 16.0 / 9.0,
        view_bob: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };
    let input = UmiCameraInput {
        size: size_of::<UmiCameraInput>() as u32,
        flags: 0,
        look_dx: 0.0,
        look_dy: 0.0,
        forward: 0.0,
        strafe: 0.0,
        up: 0.0,
        frame_seconds: 0.5,
    };
    let mut camera = built_in;
    chain.place(&mut camera, &input);
    assert_eq!(camera, built_in);

    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    render.dispatch(&mut KeyPressed::new(id("camera_orbit"), "orbit"));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut camera = built_in;
        chain.place(&mut camera, &input);
        if camera.eye != built_in.eye {
            let [dx, dz] = [camera.eye[0] - 0.5, camera.eye[2] - 0.5];
            assert!((dx.hypot(dz) - 5.0).abs() < 1e-9, "on the default circle");
            assert!(camera.pitch > 0.0, "looking down at the player");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the key never arrived"
        );
        std::thread::yield_now();
    }
    assert_eq!(host.mods().next().map(|m| m.failed), Some(false));
    host.shutdown_all();
}

/// The real `suite` (API 1.2): its own tab, two modules, a widget tied to a module,
/// buttons and the module switch, end to end.
#[test]
fn suite_lists_its_modules_and_answers_buttons_and_switches() {
    let tmp = TempDir::new();
    install(&tmp.0, "suite");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let suite = id("suite");

    let view = host.mods().next().unwrap();
    let own: Vec<_> = view.settings().map(|s| s.spec.key.clone()).collect();
    assert_eq!(own, ["about"], "only the About button is on the mod's card");
    let modules: Vec<_> = view
        .modules()
        .map(|m| (m.key, m.category, m.enabled))
        .collect();
    assert_eq!(
        modules,
        [("counter", Some("Suite"), true), ("greeter", None, false)]
    );
    let counter_card: Vec<_> = view
        .modules()
        .next()
        .unwrap()
        .settings()
        .map(|s| (s.spec.key.clone(), s.spec.is_button()))
        .collect();
    assert_eq!(
        counter_card,
        [("every".to_owned(), false), ("reset".to_owned(), true)]
    );

    // The widget follows its module.
    let drawn = |host: &ModHost| {
        host.draw_widgets();
        let mut lines = Vec::new();
        host.visit_widgets(|w| lines.extend(w.content.lines().iter().map(|l| l.text.clone())));
        lines
    };
    let mut game = EventBus::default();
    host.install(&mut game, EventThread::Game);
    for tick in 0..3 {
        game.dispatch(&mut PostTick::new(tick));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while drawn(&host) != ["ticks: 3"] {
        assert!(std::time::Instant::now() < deadline, "{:?}", drawn(&host));
        std::thread::yield_now();
    }
    assert!(host.set_module(&suite, "counter", false).unwrap().is_some());
    assert!(drawn(&host).is_empty(), "hidden while its module is off");
    assert_eq!(
        host.config(&suite).unwrap().modules.get("counter"),
        Some(&false)
    );
    host.set_module(&suite, "counter", true).unwrap();

    // Switches and buttons reach the mod on the render thread.
    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    let mut on = host.set_module(&suite, "greeter", true).unwrap().unwrap();
    assert!(host.set_module(&suite, "greeter", true).unwrap().is_none());
    render.dispatch(&mut on);
    let outbox = host.chat_outbox();
    let line = outbox.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(line, "Hello! The greeter is on.");
    render.dispatch(&mut host.press_button(&suite, "wave").unwrap());
    assert_eq!(outbox.recv_timeout(Duration::from_secs(5)).unwrap(), "o/");
    render.dispatch(&mut host.press_button(&suite, "reset").unwrap());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while drawn(&host) != ["ticks: 0"] {
        assert!(std::time::Instant::now() < deadline, "{:?}", drawn(&host));
        std::thread::yield_now();
    }
    assert!(
        host.press_button(&suite, "every").is_err(),
        "a slider is no button"
    );
    assert_eq!(host.mods().next().map(|m| m.failed), Some(false));
}

/// The real `free_cam` (API 1.3): once its key is pressed it flies the camera with
/// the movement intent and captures the input on every frame; pressed again, it
/// lets go and the camera is the player's again.
#[test]
fn free_cam_flies_the_camera_and_captures_the_input_while_on() {
    let tmp = TempDir::new();
    install(&tmp.0, "free_cam");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, host_version());
    let (mut host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let chain = host.camera().expect("one camera layer");
    let built_in = UmiRenderCamera {
        size: size_of::<UmiRenderCamera>() as u32,
        flags: 0,
        eye: [0.5, 65.62, 0.5],
        yaw: 0.0,
        pitch: 0.0,
        fov_y: 70.0,
        aspect: 16.0 / 9.0,
        view_bob: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };
    let forward = UmiCameraInput {
        size: size_of::<UmiCameraInput>() as u32,
        flags: 0,
        look_dx: 0.0,
        look_dy: 0.0,
        forward: 1.0,
        strafe: 0.0,
        up: 0.0,
        frame_seconds: 0.1,
    };
    // Off: the player keeps the input and the camera is untouched.
    let mut camera = built_in;
    assert!(!chain.place(&mut camera, &forward));
    assert_eq!(camera, built_in);

    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    render.dispatch(&mut KeyPressed::new(id("free_cam"), "toggle"));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut camera = built_in;
        if chain.place(&mut camera, &forward) {
            // Default speed 10 blocks/s for 0.1 s, looking south (+Z).
            assert!(camera.eye[2] > built_in.eye[2], "{:?}", camera.eye);
            assert_eq!(camera.eye[1], built_in.eye[1]);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the key never arrived"
        );
        std::thread::yield_now();
    }
    render.dispatch(&mut KeyPressed::new(id("free_cam"), "toggle"));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut camera = built_in;
        if !chain.place(&mut camera, &forward) {
            assert_eq!(camera, built_in, "back with the player");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the key never arrived"
        );
        std::thread::yield_now();
    }
    assert_eq!(host.mods().next().map(|m| m.failed), Some(false));
    host.shutdown_all();
}

/// The API this client implements. That a 1.0 manifest still resolves on a 1.1
/// host is pinned by `api_version_acceptance_table` (tests/manifest.rs).
fn host_version() -> ApiVersion {
    ApiVersion {
        major: umicraft_modapi_sys::UMI_API_VERSION.major,
        minor: umicraft_modapi_sys::UMI_API_VERSION.minor,
    }
}
