// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crash sentinel: a locked file that exists only while the client is running.
//!
//! Mods run native code that can crash the process, and a crash cannot be caught. The
//! client arms the sentinel before loading mods and disarms it on a clean exit.
//!
//! The file is held under an exclusive advisory lock (`File::try_lock`: `flock` on
//! Unix, `LockFileEx` on Windows) for as long as the guard lives, so two things can be
//! told apart. A leftover file that can be locked belongs to a process that is gone: the
//! previous run did not end cleanly, and the launcher can offer safe mode. A file that
//! is locked by someone else belongs to a running client: another instance, for which
//! there is no sentinel and no safe mode, because its file is not ours to judge. The OS
//! releases the lock when a process dies, however it dies.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Namespace for the sentinel operations.
#[derive(Debug, Clone, Copy)]
pub struct CrashSentinel;

/// The outcome of [`CrashSentinel::arm`].
#[derive(Debug)]
#[must_use = "dropping an armed guard leaves the sentinel behind"]
pub enum Arming {
    /// This instance holds the sentinel.
    Armed {
        /// Disarm it on a clean exit.
        guard: SentinelGuard,
        /// A sentinel was left by a process that is gone: the last run crashed.
        previous_crash: bool,
    },
    /// Another running instance holds the sentinel: leave it alone.
    AnotherInstance,
}

/// Proof that the sentinel is armed and locked; [`SentinelGuard::disarm`] removes it.
///
/// Dropping the guard deliberately leaves the file: a panic or a crash unwinds through
/// it, and the file must survive so the next start sees the unclean exit. The lock is
/// released when the process ends.
#[derive(Debug)]
#[must_use = "dropping the guard leaves the sentinel armed"]
pub struct SentinelGuard {
    path: PathBuf,
    /// Kept open: closing the file would release the lock.
    file: File,
}

impl CrashSentinel {
    /// Arms the sentinel at `path`, writing the process id into it to help a human
    /// investigate, and keeps it locked.
    ///
    /// # Errors
    ///
    /// The I/O error if the file or its parent directory cannot be created, locked
    /// (for a reason other than another holder) or written.
    pub fn arm(path: &Path) -> io::Result<Arming> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        // No truncation on open: the previous holder's pid stays readable until we hold the lock.
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(Arming::AnotherInstance),
            Err(TryLockError::Error(error)) => return Err(error),
        }
        file.set_len(0)?;
        file.write_all(std::process::id().to_string().as_bytes())?;
        Ok(Arming::Armed {
            guard: SentinelGuard {
                path: path.to_path_buf(),
                file,
            },
            previous_crash: existed,
        })
    }
}

impl SentinelGuard {
    /// Unlocks and removes the sentinel; call only on a clean exit.
    ///
    /// The file is removed first, then unlocked: another instance that locks it in
    /// between would hold a file nobody can see, which is harmless.
    ///
    /// # Errors
    ///
    /// The I/O error if the file cannot be removed or unlocked.
    pub fn disarm(self) -> io::Result<()> {
        fs::remove_file(&self.path)?;
        self.file.unlock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("umi-sentinel-{}-{name}", std::process::id()))
    }

    fn armed(arming: Arming) -> (SentinelGuard, bool) {
        match arming {
            Arming::Armed {
                guard,
                previous_crash,
            } => (guard, previous_crash),
            Arming::AnotherInstance => panic!("expected to arm"),
        }
    }

    #[test]
    fn a_fresh_arm_is_no_crash_and_disarm_removes_the_file() {
        let path = scratch("clean");
        let (guard, previous_crash) = armed(CrashSentinel::arm(&path).unwrap());
        assert!(!previous_crash);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );
        guard.disarm().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn a_leftover_file_means_the_previous_run_crashed() {
        let path = scratch("crash");
        // Dropping the guard stands for a crash: the file stays, the lock goes with the handle.
        drop(armed(CrashSentinel::arm(&path).unwrap()));
        assert!(path.exists());
        let (guard, previous_crash) = armed(CrashSentinel::arm(&path).unwrap());
        assert!(previous_crash);
        guard.disarm().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn a_second_arm_while_the_first_is_held_is_another_instance() {
        let path = scratch("twice");
        let (first, _) = armed(CrashSentinel::arm(&path).unwrap());
        // std's lock is per open file description (flock) / per handle (LockFileEx), so a
        // second open in this process conflicts just like a second process would.
        assert!(matches!(
            CrashSentinel::arm(&path).unwrap(),
            Arming::AnotherInstance
        ));
        // The refused instance did not touch the first one's file.
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );
        first.disarm().unwrap();
    }
}
