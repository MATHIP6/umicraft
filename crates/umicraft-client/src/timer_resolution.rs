// SPDX-License-Identifier: MIT OR Apache-2.0

//! A 1 ms timer resolution for the process while the client runs (Windows).
//!
//! The game thread sleeps until the next 50 ms tick (`recv_timeout` in
//! `game::run`). Windows rounds such waits up to its timer period, 15.625 ms
//! by default, and since Windows 10 2004 that period is per process: another
//! program asking for 1 ms (a browser) no longer helps this one. Measured on
//! a Pentium G5400 with a copy of the game loop, ten seconds each:
//!
//! | period | tick late, mean | p95 | ticks over 5 ms late |
//! |---|---|---|---|
//! | default | 8.3-8.6 ms | 15.4-15.6 ms | 72 % |
//! | 1 ms (`timeBeginPeriod(1)`) | 1.0-2.3 ms | 1.7-4.5 ms | 0-4 % |
//!
//! A late tick arrives at the render thread after the interpolation has run
//! out, so the camera stops, then jumps: movement and knockback felt heavy
//! at any frame rate. Games ask for 1 ms for this reason (the reference's
//! LWJGL `Display` does as well). Elsewhere this is a no-op.

/// Holds the 1 ms period for as long as it lives.
#[derive(Debug)]
#[must_use = "the resolution falls back when the guard is dropped"]
pub struct TimerResolution {
    raised: bool,
}

/// The period asked for, in milliseconds.
#[cfg(windows)]
const PERIOD_MS: u32 = 1;

impl TimerResolution {
    /// Asks for a 1 ms timer period; the guard gives it back when dropped.
    pub fn raise() -> Self {
        Self {
            raised: begin_period(),
        }
    }

    /// Whether the period was granted.
    #[must_use]
    pub fn is_raised(&self) -> bool {
        self.raised
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        if self.raised {
            end_period();
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn begin_period() -> bool {
    // SAFETY: `timeBeginPeriod` takes a plain integer and touches no memory of
    // ours; it returns TIMERR_NOERROR (0) when the period is granted.
    unsafe { windows_sys::Win32::Media::timeBeginPeriod(PERIOD_MS) == 0 }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn end_period() {
    // SAFETY: as above; called once for each granted `timeBeginPeriod`.
    unsafe {
        windows_sys::Win32::Media::timeEndPeriod(PERIOD_MS);
    }
}

#[cfg(not(windows))]
fn begin_period() -> bool {
    false
}

#[cfg(not(windows))]
fn end_period() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_period_is_granted_on_windows_and_given_back() {
        let guard = TimerResolution::raise();
        assert_eq!(guard.is_raised(), cfg!(windows));
        drop(guard);
    }
}
