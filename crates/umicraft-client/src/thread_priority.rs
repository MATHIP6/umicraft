// SPDX-License-Identifier: MIT OR Apache-2.0

//! Thread priorities: who the CPU serves first when every core is busy.
//!
//! On a two-core, four-thread CPU (a Pentium G5400) the client has more busy
//! threads than the CPU has slots while chunks load: the render thread, the
//! chunk meshing workers, the game thread and the network thread. Windows
//! shares the slots evenly among equals, so a tick or a packet waited behind
//! a mesh job, the ticks arrived in bunches and the camera jumped: a high
//! frame rate that felt like 30. The game and network threads sleep almost
//! always and must wake on time, so they go above normal, which is enough to
//! put them ahead of the meshing. The meshing workers and the render thread
//! stay normal: below normal, the workers waited behind every other busy
//! program of the PC (a video wallpaper decoding on every core) and the
//! world stayed empty for seconds after a join.
//!
//! Measured on a Pentium G5400 (four threads) with a copy of the game loop's
//! wait (`recv_timeout` until the next 50 ms tick, 1 ms timer period) while
//! four threads stand in for busy mesh workers, 200 ticks each:
//!
//! | game thread | tick late, mean | p95 | max |
//! |---|---|---|---|
//! | normal | 30.0 ms | 63.6 ms | 107.3 ms |
//! | above normal | 5.7 ms | 10.0 ms | 14.2 ms |

/// A priority for the calling thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Short, timely work: the ticks, the packets.
    AboveNormal,
}

/// Sets the priority of the calling thread; a refusal is logged and ignored.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn set_current(priority: Priority) {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
    };
    let value = match priority {
        Priority::AboveNormal => THREAD_PRIORITY_ABOVE_NORMAL,
    };
    // SAFETY: `GetCurrentThread` returns a pseudo-handle of the calling
    // thread, valid for this call; no pointer is involved.
    let ok = unsafe { SetThreadPriority(GetCurrentThread(), value) };
    if ok == 0 {
        tracing::debug!(?priority, "the thread priority was not changed");
    }
}

/// Thread priorities are left to the system elsewhere.
#[cfg(not(windows))]
pub fn set_current(_priority: Priority) {}
