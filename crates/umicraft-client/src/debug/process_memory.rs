// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory use for the `Mem:` line of the F3 screen.
//!
//! The reference prints the JVM heap (`Runtime.totalMemory`/`maxMemory`).
//! A native client has no heap ceiling, so this reports the resident set of
//! the process against the physical memory of the machine. Linux reads `/proc`;
//! elsewhere only the resident set is known (`memory-stats`) and the maximum is zero.

/// Resident and total memory in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryUse {
    /// The resident set size of this process.
    pub used: u64,
    /// The physical memory of the machine.
    pub max: u64,
}

/// Reads the current memory use; zeros when the platform has no `/proc`.
#[must_use]
pub fn read() -> MemoryUse {
    // Windows and macOS: the working set of the process, from `memory-stats`. The
    // physical memory of the machine is not read there, so the maximum is zero.
    #[cfg(not(target_os = "linux"))]
    if let Some(stats) = memory_stats::memory_stats() {
        return MemoryUse {
            used: stats.physical_mem as u64,
            max: 0,
        };
    }
    let used = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| kilobytes(&text, "VmRSS:"));
    let max = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| kilobytes(&text, "MemTotal:"));
    MemoryUse {
        used: used.unwrap_or(0),
        max: max.unwrap_or(0),
    }
}

/// The bytes of the `key  1234 kB` line of a `/proc` file.
fn kilobytes(text: &str, key: &str) -> Option<u64> {
    let line = text.lines().find_map(|line| line.strip_prefix(key))?;
    let value: u64 = line.split_whitespace().next()?.parse().ok()?;
    value.checked_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proc_line_is_converted_to_bytes() {
        let text = "Name:\tumicraft\nVmRSS:\t   2048 kB\nThreads:\t4\n";
        assert_eq!(kilobytes(text, "VmRSS:"), Some(2048 * 1024));
        assert_eq!(kilobytes(text, "MemTotal:"), None);
    }

    #[test]
    fn garbage_is_not_a_number() {
        assert_eq!(kilobytes("VmRSS: lots kB", "VmRSS:"), None);
    }
}
