// SPDX-License-Identifier: MIT OR Apache-2.0

//! Boot tracing and instrumentation to diagnose launch stalls, GPU initialization
//! deadlocks, surface acquisition errors, and presentation failures.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

static BOOT_LOGGER: Mutex<Option<BootLogger>> = Mutex::new(None);
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

struct BootLogger {
    file: File,
    start: Instant,
    last_step: Instant,
}

/// Initializes the boot log file and installs a panic hook that logs the full panic
/// information and stack trace to the log file immediately.
pub fn init() {
    let mut guard = BOOT_LOGGER.lock().unwrap();
    if guard.is_none() {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("umicraft_boot.log");

        let now = Instant::now();
        if let Ok(mut f) = file {
            let _ = writeln!(f, "=== UMICRAFT BOOT LOG STARTED ===");
            let _ = f.flush();
            *guard = Some(BootLogger {
                file: f,
                start: now,
                last_step: now,
            });
        }
    }
    drop(guard);

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        let msg = format!(
            "[BOOT] PANIC on thread '{}': {}\nBacktrace:\n{:?}",
            name,
            info,
            std::backtrace::Backtrace::capture()
        );
        log_step("PANIC", &msg);
        default_hook(info);
    }));
}

/// Logs a boot step with elapsed total time, delta since previous step, current thread info,
/// and writes immediately with flush.
pub fn log_step(step: &str, details: &str) {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("main");
    let thread_id = format!("{:?}", thread.id());

    let now = Instant::now();
    let mut guard = BOOT_LOGGER.lock().unwrap();
    let (total_ms, delta_ms, file_opt) = if let Some(ref mut logger) = *guard {
        let total = now.duration_since(logger.start).as_secs_f64() * 1000.0;
        let delta = now.duration_since(logger.last_step).as_secs_f64() * 1000.0;
        logger.last_step = now;
        (total, delta, Some(&mut logger.file))
    } else {
        (0.0, 0.0, None)
    };

    let line = if details.is_empty() {
        format!(
            "[{:>8.2}ms (+{:>6.2}ms)] [Thread: {} ({})] [BOOT] {}",
            total_ms, delta_ms, thread_name, thread_id, step
        )
    } else {
        format!(
            "[{:>8.2}ms (+{:>6.2}ms)] [Thread: {} ({})] [BOOT] {} - {}",
            total_ms, delta_ms, thread_name, thread_id, step, details
        )
    };

    eprintln!("{}", line);
    if let Some(file) = file_opt {
        let _ = writeln!(file, "{}", line);
        let _ = file.flush();
    }
}

/// Increments and logs a presented frame count.
pub fn log_frame_presented() -> u64 {
    let count = FRAME_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    let now = Instant::now();
    let mut guard = BOOT_LOGGER.lock().unwrap();
    if let Some(ref mut logger) = *guard {
        let total = now.duration_since(logger.start).as_secs_f64() * 1000.0;
        let line = format!(
            "[{:>8.2}ms] [BOOT] frame_presented count={}",
            total, count
        );
        let _ = writeln!(logger.file, "{}", line);
        let _ = logger.file.flush();
    }
    count
}

/// Returns the total number of frames presented so far.
pub fn frames_presented() -> u64 {
    FRAME_COUNTER.load(Ordering::SeqCst)
}

/// Logs a boot progress step with immediate file and console flush.
#[macro_export]
macro_rules! boot_log {
    ($step:expr) => {
        $crate::boot_log::log_step($step, "")
    };
    ($step:expr, $($arg:tt)*) => {
        $crate::boot_log::log_step($step, &format!($($arg)*))
    };
}
