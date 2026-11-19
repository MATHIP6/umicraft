//! Where a mod's native library lives for each platform.

/// A supported operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// Windows (`.dll`).
    Windows,
    /// Linux (`.so`).
    Linux,
    /// macOS (`.dylib`).
    Macos,
}

/// A supported CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// 64-bit x86.
    X86_64,
    /// 64-bit ARM.
    Aarch64,
}

impl Os {
    /// The OS this build runs on, or `None` if it is not supported.
    pub fn current() -> Option<Self> {
        if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else {
            None
        }
    }
}

impl Arch {
    /// The architecture this build runs on, or `None` if it is not supported.
    pub fn current() -> Option<Self> {
        if cfg!(target_arch = "x86_64") {
            Some(Self::X86_64)
        } else if cfg!(target_arch = "aarch64") {
            Some(Self::Aarch64)
        } else {
            None
        }
    }
}

/// The file name of the library `library` on `os`: `foo.dll`, `libfoo.so`, `libfoo.dylib`.
pub fn library_file_name(library: &str, os: Os) -> String {
    match os {
        Os::Windows => format!("{library}.dll"),
        Os::Linux => format!("lib{library}.so"),
        Os::Macos => format!("lib{library}.dylib"),
    }
}

/// The `bin/` sub-directory holding the libraries for a platform, e.g. `linux-x86_64`.
pub fn platform_dir(os: Os, arch: Arch) -> &'static str {
    match (os, arch) {
        (Os::Windows, Arch::X86_64) => "windows-x86_64",
        (Os::Windows, Arch::Aarch64) => "windows-aarch64",
        (Os::Linux, Arch::X86_64) => "linux-x86_64",
        (Os::Linux, Arch::Aarch64) => "linux-aarch64",
        (Os::Macos, Arch::X86_64) => "macos-x86_64",
        (Os::Macos, Arch::Aarch64) => "macos-aarch64",
    }
}
