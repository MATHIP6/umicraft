// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loading and layering of 1.8 resource packs (spec AST-5).
//!
//! A 1.8 resource pack is either an uncompressed directory or a `.zip` archive
//! placed in the user's `resourcepacks` directory. At its root it carries:
//!
//! - `pack.mcmeta` with metadata (format version and description);
//! - optionally `pack.png` (icon image);
//! - assets under the `assets/minecraft/` namespace:
//!   - `textures/blocks/*.png` (+ optional `.mcmeta` animation descriptions);
//!   - `textures/items/*.png`;
//!   - `textures/colormap/grass.png` and `textures/colormap/foliage.png`;
//!   - `textures/font/ascii.png`;
//!   - `models/block/*.json` and `models/item/*.json`;
//!   - `blockstates/*.json`.
//!
//! [`ResourcePack`] parses and exposes the files of a single pack.
//! [`ResourceStack`] layers multiple packs with an ordered priority over the
//! base client resources, overriding textures, models, colormaps, and fonts
//! without altering the original files on disk.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::client_resources::ClientResources;
use crate::error::{AssetsError, AssetsResult};
use crate::zip::ZipArchive;

/// Standard resource prefix for Minecraft assets inside a pack.
const ASSETS_PREFIX: &str = "assets/minecraft/";

/// Standard file name for resource pack metadata.
const PACK_MCMETA: &str = "pack.mcmeta";

/// Standard file name for resource pack preview icon.
const PACK_PNG: &str = "pack.png";

/// Metadata parsed from a resource pack's `pack.mcmeta` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMetadata {
    /// Format version (`1` for Minecraft 1.8).
    pub pack_format: u32,
    /// Human-readable description of the pack.
    pub description: String,
}

impl PackMetadata {
    /// Parses `pack.mcmeta` JSON bytes.
    ///
    /// Accepts both simple string descriptions and formatted chat-component objects.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Json`] or [`AssetsError::Pack`] if the document is malformed
    /// or lacks a `pack` section.
    pub fn parse(bytes: &[u8]) -> AssetsResult<Self> {
        let value: Value = serde_json::from_slice(bytes).map_err(|source| AssetsError::Json {
            what: PACK_MCMETA.to_string(),
            source,
        })?;

        let pack = value
            .get("pack")
            .ok_or_else(|| AssetsError::Pack {
                name: PACK_MCMETA.to_string(),
                reason: "missing `pack` section".to_string(),
            })?;

        let pack_format = pack
            .get("pack_format")
            .and_then(Value::as_u64)
            .and_then(|num| u32::try_from(num).ok())
            .unwrap_or(1);

        let description = match pack.get("description") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Object(map)) => {
                if let Some(Value::String(text)) = map.get("text") {
                    text.clone()
                } else {
                    Value::Object(map.clone()).to_string()
                }
            }
            Some(other) => other.to_string(),
            None => String::new(),
        };

        Ok(Self {
            pack_format,
            description,
        })
    }
}

/// A loaded 1.8 resource pack held in memory.
#[derive(Debug, Clone)]
pub struct ResourcePack {
    id: String,
    metadata: Option<PackMetadata>,
    icon: Option<Vec<u8>>,
    files: BTreeMap<String, Vec<u8>>,
}

impl ResourcePack {
    /// Creates a pack from an in-memory map of files and optional metadata.
    #[must_use]
    pub fn from_parts(
        id: impl Into<String>,
        metadata: Option<PackMetadata>,
        icon: Option<Vec<u8>>,
        files: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        Self {
            id: id.into(),
            metadata,
            icon,
            files,
        }
    }

    /// Loads a resource pack from a filesystem path (directory or `.zip` file).
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] if files cannot be read, or [`AssetsError::Zip`] if a zip
    /// archive is malformed.
    pub fn from_path(path: impl AsRef<Path>) -> AssetsResult<Self> {
        let path = path.as_ref();
        if path.is_dir() {
            Self::from_directory(path)
        } else if path.is_file() {
            Self::from_zip_file(path)
        } else {
            Err(AssetsError::Missing {
                what: "resource pack".to_string(),
                path: path.display().to_string(),
            })
        }
    }

    /// Loads an uncompressed resource pack from a directory on disk.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] if reading files fails.
    pub fn from_directory(root: &Path) -> AssetsResult<Self> {
        let id = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut files = BTreeMap::new();
        let mut metadata = None;
        let mut icon = None;

        let mcmeta_path = root.join(PACK_MCMETA);
        if mcmeta_path.is_file() {
            if let Ok(bytes) = std::fs::read(&mcmeta_path) {
                metadata = PackMetadata::parse(&bytes).ok();
            }
        }

        let icon_path = root.join(PACK_PNG);
        if icon_path.is_file() {
            icon = std::fs::read(&icon_path).ok();
        }

        let assets_dir = root.join("assets");
        if assets_dir.is_dir() {
            collect_dir_files(root, &assets_dir, &mut files)?;
        }

        Ok(Self {
            id,
            metadata,
            icon,
            files,
        })
    }

    /// Loads a resource pack from a `.zip` archive on disk.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] if the file cannot be read, or [`AssetsError::Zip`] if the archive
    /// is invalid.
    pub fn from_zip_file(path: &Path) -> AssetsResult<Self> {
        let id = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        let data = std::fs::read(path).map_err(|source| AssetsError::Io {
            path: path.display().to_string(),
            source,
        })?;

        Self::from_zip_bytes(&id, data)
    }

    /// Loads a resource pack from raw `.zip` archive bytes.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Zip`] if the zip container cannot be parsed.
    pub fn from_zip_bytes(id: &str, data: Vec<u8>) -> AssetsResult<Self> {
        let archive = ZipArchive::open(data)?;
        let mut files = BTreeMap::new();
        let mut metadata = None;
        let mut icon = None;

        for entry in archive.entries() {
            let normalized = normalize_virtual_path(&entry.name);
            if is_ignored_path(&normalized) {
                continue;
            }
            if normalized == PACK_MCMETA {
                if let Ok(bytes) = archive.read(entry) {
                    metadata = PackMetadata::parse(&bytes).ok();
                }
            } else if normalized == PACK_PNG {
                icon = archive.read(entry).ok();
            } else if normalized.starts_with(ASSETS_PREFIX) {
                if let Ok(bytes) = archive.read(entry) {
                    files.insert(normalized, bytes);
                }
            }
        }

        Ok(Self {
            id: id.to_string(),
            metadata,
            icon,
            files,
        })
    }

    /// Identifier of the resource pack (folder name or zip stem).
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Pack metadata if present and parsed successfully.
    #[must_use]
    pub fn metadata(&self) -> Option<&PackMetadata> {
        self.metadata.as_ref()
    }

    /// Optional raw PNG bytes of the pack icon (`pack.png`).
    #[must_use]
    pub fn icon(&self) -> Option<&[u8]> {
        self.icon.as_deref()
    }

    /// Number of game asset files contained in the pack.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Retrieves the content of a file within the pack, if present.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// Lists all virtual paths within the pack starting with `prefix`, sorted.
    #[must_use]
    pub fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Read-only access to all game asset files in the pack.
    #[must_use]
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}

/// Helper to recursively collect files from a directory into virtual paths.
fn collect_dir_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> AssetsResult<()> {
    let entries = std::fs::read_dir(current).map_err(|source| AssetsError::Io {
        path: current.display().to_string(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| AssetsError::Io {
            path: current.display().to_string(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_files(root, &path, files)?;
        } else if path.is_file() {
            if let Ok(relative) = path.strip_prefix(root) {
                let virtual_path = relative
                    .components()
                    .map(|comp| comp.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if is_ignored_path(&virtual_path) {
                    continue;
                }
                if virtual_path.starts_with(ASSETS_PREFIX) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        files.insert(virtual_path, bytes);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Returns true if `path` is an OS metadata or hidden file (e.g. `__MACOSX`, `.DS_Store`, `._*`).
fn is_ignored_path(path: &str) -> bool {
    path.starts_with("__MACOSX/") || path.split('/').any(|segment| segment.starts_with('.'))
}

/// Normalizes archive paths by removing leading slashes and handling backslashes.
fn normalize_virtual_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

/// An ordered stack of resource packs layered over base client resources.
///
/// Packs are evaluated in ascending order of priority:
/// - Pack at index 0 has the lowest priority;
/// - Pack at index `len - 1` has the highest priority and overrides all earlier packs;
/// - Every pack overrides matching files from the base client resources.
#[derive(Debug, Clone, Default)]
pub struct ResourceStack {
    packs: Vec<ResourcePack>,
}

impl ResourceStack {
    /// Creates an empty stack without any active resource pack.
    #[must_use]
    pub fn new() -> Self {
        Self { packs: Vec::new() }
    }

    /// Creates a stack from an ordered list of packs (ascending priority).
    #[must_use]
    pub fn with_packs(packs: Vec<ResourcePack>) -> Self {
        Self { packs }
    }

    /// Pushes a pack onto the top of the stack (making it the highest priority).
    pub fn push(&mut self, pack: ResourcePack) {
        self.packs.push(pack);
    }

    /// The active packs in ascending priority order.
    #[must_use]
    pub fn packs(&self) -> &[ResourcePack] {
        &self.packs
    }

    /// True if no resource pack is currently active in this stack.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    /// Number of active packs in the stack.
    #[must_use]
    pub fn len(&self) -> usize {
        self.packs.len()
    }

    /// Resolves a file path across the stack, searching from highest to lowest priority.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        for pack in self.packs.iter().rev() {
            if let Some(bytes) = pack.get(path) {
                return Some(bytes);
            }
        }
        None
    }

    /// Applies the layered resource packs onto `base`, generating a new [`ClientResources`]
    /// with overridden textures, models, blockstates, colormaps and atlas.
    ///
    /// # Errors
    ///
    /// [`AssetsError`] if any overridden texture or colormap is invalid.
    pub fn apply_to_resources(&self, base: &ClientResources) -> AssetsResult<ClientResources> {
        if self.packs.is_empty() {
            return Ok(base.clone());
        }
        base.apply_packs(&self.packs)
    }
}
