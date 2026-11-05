// SPDX-License-Identifier: MIT OR Apache-2.0

//! The done-criterion "the audio thread never blocks the game or render
//! thread" (ROADMAP P11), measured.
//!
//! A real audio thread runs with a loader that sleeps on every file and a
//! backend that sleeps on every voice, as a stuck disk or driver would. A
//! simulated 20 Hz game thread floods the `AudioHandle` with sounds, music
//! requests and stops, and a 60 fps render thread sends the listener and
//! polls the music flag. Every call is timed on the calling thread; the
//! slowest calls must stay far below a frame.
//!
//! Run `cargo test --release -p umicraft-audio --test stall_latency --
//! --nocapture` for the numbers recorded in `docs/perf-log.md`.

use std::io;
use std::thread::sleep;
use std::time::{Duration, Instant};

use umicraft_audio::backend::{AudioBackend, Reaped, Voice};
use umicraft_audio::engine::SoundLoader;
use umicraft_audio::thread::spawn_with;
use umicraft_audio::{AudioError, AudioHandle, Listener, SoundCategory, SoundIndex, SoundRequest};

const TONE: &[u8] = include_bytes!("fixtures/tone_440hz_mono.ogg");
/// How long each fake device operation stalls the audio thread.
const STALL: Duration = Duration::from_millis(40);
/// The slowest call tolerated for the 99th percentile.
const P99_LIMIT: Duration = Duration::from_millis(1);
/// The slowest call tolerated at all; generous, as one preemption of the test
/// thread by the OS scheduler is not the audio thread's fault.
const MAX_LIMIT: Duration = Duration::from_millis(5);
const SOUNDS: usize = 64;

struct StalledLoader;

impl SoundLoader for StalledLoader {
    fn load(&mut self, _resource_path: &str) -> io::Result<Vec<u8>> {
        sleep(STALL);
        Ok(TONE.to_vec())
    }
}

struct StalledBackend;

impl AudioBackend for StalledBackend {
    fn play(&mut self, _voice: Voice) -> Result<(), AudioError> {
        sleep(STALL);
        Ok(())
    }

    fn reap(&mut self) -> Reaped {
        sleep(STALL);
        Reaped::default()
    }

    fn stop_streaming(&mut self) {
        sleep(STALL);
    }
}

fn index() -> SoundIndex {
    let mut events: Vec<String> = (0..SOUNDS)
        .map(|n| format!(r#""fx{n}": {{"sounds": ["fx{n}"]}}"#))
        .collect();
    events.push(
        r#""music.menu": {"category": "music", "sounds": [{"name": "song", "stream": true}]}"#
            .to_owned(),
    );
    SoundIndex::parse(&format!("{{{}}}", events.join(","))).expect("valid index")
}

/// Calls `step` every `period` for `duration` and returns how long each call took.
fn run_loop(duration: Duration, period: Duration, mut step: impl FnMut(u32)) -> Vec<Duration> {
    let start = Instant::now();
    let mut durations = Vec::new();
    let mut tick = 0;
    while start.elapsed() < duration {
        let begin = Instant::now();
        step(tick);
        durations.push(begin.elapsed());
        tick += 1;
        sleep(period.saturating_sub(begin.elapsed()));
    }
    durations
}

/// One game tick: a burst of sounds, a stop and a restart of the music now and then.
fn game_tick(handle: &AudioHandle, tick: u32, calls: &mut Vec<Duration>) {
    let mut timed = |call: &dyn Fn()| {
        let begin = Instant::now();
        call();
        calls.push(begin.elapsed());
    };
    for n in 0..12 {
        let event = format!("fx{}", (tick as usize * 12 + n) % SOUNDS);
        timed(&|| {
            handle.play(SoundRequest::new(event.as_str()).at([1.0, 64.0, 2.0]));
        });
    }
    if tick % 20 == 0 {
        timed(&|| {
            handle.stop_music();
        });
        timed(&|| {
            handle.play_music(SoundRequest::new("music.menu"));
        });
    }
    if tick % 10 == 0 {
        timed(&|| {
            handle.set_volume(SoundCategory::Blocks, 0.5);
        });
    }
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    let last = sorted.len() - 1;
    sorted[(last as f64 * fraction).round() as usize]
}

#[test]
fn a_stalled_loader_and_backend_never_delay_the_game_or_render_thread() {
    let thread = spawn_with(|| Ok(StalledBackend), Box::new(StalledLoader), 7).expect("spawns");
    let handle = thread.handle();
    assert!(handle.set_index(index()));
    let secs: u64 = std::env::var("UMICRAFT_STALL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let duration = Duration::from_secs(secs);

    let render_handle = handle.clone();
    let render = std::thread::spawn(move || {
        run_loop(duration, Duration::from_micros(16_667), |frame| {
            render_handle.set_listener(Listener {
                position: [f64::from(frame), 64.0, 0.0],
                yaw: frame as f32,
            });
            let _ = render_handle.music_playing();
        })
    });
    let mut game_calls = Vec::new();
    run_loop(duration, Duration::from_millis(50), |tick| {
        game_tick(&handle, tick, &mut game_calls);
    });
    let mut calls = render.join().expect("the render thread finished");
    calls.extend(game_calls);

    calls.sort_unstable();
    let (p50, p99, max) = (
        percentile(&calls, 0.5),
        percentile(&calls, 0.99),
        calls[calls.len() - 1],
    );
    println!(
        "stall {STALL:?}: {} calls, p50 {p50:?}, p99 {p99:?}, max {max:?}, dropped {}",
        calls.len(),
        handle.dropped()
    );
    assert!(p99 < P99_LIMIT, "p99 {p99:?}");
    assert!(max < MAX_LIMIT, "max {max:?}");
    assert!(
        handle.dropped() > 0,
        "the stall must fill the queue, or the test proves nothing"
    );
    // Not joined: the stalled thread still has a full queue to chew through
    // (about 20 s), and the test process ending takes it down.
    drop(thread);
}
