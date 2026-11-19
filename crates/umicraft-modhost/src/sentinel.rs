// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crash sentinel: a file that exists only while the client is running.
//!
//! Mods run native code that can crash the process, and a crash cannot be caught. The
//! client arms the sentinel before loading mods and disarms it on a clean exit; finding
//! it at the next start means the previous run did not end cleanly, so the launcher
//! can offer safe mode.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Namespace for the sentinel operations.
#[derive(Debug, Clone, Copy)]
pub struct CrashSentinel;

/// Proof that the sentinel is armed; [`SentinelGuard::disarm`] removes the file.
///
/// Dropping the guard deliberately does nothing: a panic or a crash unwinds through
/// it, and the file must survive so the next start sees the unclean exit.
#[derive(Debug)]
#[must_use = "dropping the guard leaves the sentinel armed"]
pub struct SentinelGuard {
    path: PathBuf,
}

impl CrashSentinel {
    /// Whether a previous run left the sentinel at `path`.
    pub fn armed(path: &Path) -> bool {
        path.exists()
    }

    /// Writes the sentinel (holding the process id, to help a human investigate).
    ///
    /// # Errors
    ///
    /// The I/O error if the file or its parent directory cannot be written.
    pub fn arm(path: &Path) -> io::Result<SentinelGuard> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, std::process::id().to_string())?;
        Ok(SentinelGuard {
            path: path.to_path_buf(),
        })
    }
}

impl SentinelGuard {
    /// Removes the sentinel; call only on a clean exit.
    ///
    /// # Errors
    ///
    /// The I/O error if the file cannot be removed.
    pub fn disarm(self) -> io::Result<()> {
        fs::remove_file(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("umi-sentinel-{}-{name}", std::process::id()))
    }

    #[test]
    fn arm_then_disarm_removes_the_file() {
        let path = scratch("clean");
        assert!(!CrashSentinel::armed(&path));
        let guard = CrashSentinel::arm(&path).unwrap();
        assert!(CrashSentinel::armed(&path));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );
        guard.disarm().unwrap();
        assert!(!CrashSentinel::armed(&path));
    }

    #[test]
    fn dropping_the_guard_leaves_the_sentinel() {
        let path = scratch("crash");
        drop(CrashSentinel::arm(&path).unwrap());
        assert!(CrashSentinel::armed(&path));
        fs::remove_file(&path).unwrap();
    }
}
