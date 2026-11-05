// SPDX-License-Identifier: MIT OR Apache-2.0

//! The saved server list (`servers.toml`).
//!
//! Mirrors the semantics of the reference client's `ServerData` / `ServerList`
//! (`mcp: .../multiplayer/ServerData.java`, `ServerList.java`): an ordered list
//! of named `host[:port]` entries that the player adds, edits, removes and
//! reorders. Unlike the reference (an NBT file, silently ignored when broken),
//! the file is human-editable TOML written atomically through
//! [`ConfigStore`]; a missing or corrupt file yields an empty book and an
//! entry with an invalid address is skipped, so a hand edit can never stop the
//! menu from opening.

use std::fmt;
use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use umicraft_core::{ConfigStore, CoreError};

use umicraft_ui::menus::ResourcePackMode;

/// The port used when an address omits it (vanilla default).
pub const DEFAULT_PORT: u16 = 25565;

/// Name given to an entry saved with an empty name (the reference "add
/// server" screen pre-fills the same text).
pub const DEFAULT_NAME: &str = "Minecraft Server";

/// File name of the book inside the data directory.
const FILE_NAME: &str = "servers.toml";

/// The `ConfigStore` section holding the list.
const SECTION: &str = "servers";

/// Longest accepted host name (DNS limit).
const MAX_HOST_LEN: usize = 255;

/// Longest accepted display name; longer input is cut, not rejected.
const MAX_NAME_CHARS: usize = 64;

/// Why an address string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddressError {
    /// Nothing but whitespace.
    #[error("the server address is empty")]
    Empty,
    /// The host part is missing, too long or has forbidden characters.
    #[error("`{0}` is not a valid host name")]
    InvalidHost(String),
    /// The port is not a number in `1..=65535`.
    #[error("`{0}` is not a valid port")]
    InvalidPort(String),
    /// A `[` without its `]`, or text after the `]` that is not `:port`.
    #[error("malformed IPv6 address `{0}`")]
    InvalidIpv6(String),
}

/// A parsed `host[:port]` address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    /// Host name or IP literal (IPv6 without brackets).
    pub host: String,
    /// TCP port.
    pub port: u16,
}

impl fmt::Display for ServerAddress {
    /// Formats the address the way a player types it; the port is omitted
    /// when it is the default, IPv6 literals are bracketed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == DEFAULT_PORT {
            f.write_str(&host)
        } else {
            write!(f, "{host}:{}", self.port)
        }
    }
}

/// Parses `host`, `host:port`, `[v6]`, `[v6]:port` or a bare IPv6 literal.
///
/// Surrounding whitespace is ignored; the port defaults to [`DEFAULT_PORT`].
///
/// # Errors
/// An [`AddressError`] naming the part that is wrong. (The reference client
/// silently falls back to the default port on a bad one; an editor screen can
/// show a message instead.)
///
/// # Example
/// ```
/// use umicraft_client::servers::parse_address;
///
/// let address = parse_address("play.example.com:25570").unwrap();
/// assert_eq!(address.port, 25570);
/// ```
pub fn parse_address(input: &str) -> Result<ServerAddress, AddressError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AddressError::Empty);
    }
    if let Some(rest) = input.strip_prefix('[') {
        return parse_bracketed(input, rest);
    }
    if let Ok(host) = input.parse::<Ipv6Addr>() {
        return Ok(ServerAddress {
            host: host.to_string(),
            port: DEFAULT_PORT,
        });
    }
    match input.split_once(':') {
        None => Ok(ServerAddress {
            host: validate_host(input)?,
            port: DEFAULT_PORT,
        }),
        Some((host, port)) => Ok(ServerAddress {
            host: validate_host(host)?,
            port: parse_port(port)?,
        }),
    }
}

/// Parses the part after `[` of `[v6]` / `[v6]:port`.
fn parse_bracketed(whole: &str, rest: &str) -> Result<ServerAddress, AddressError> {
    let invalid = || AddressError::InvalidIpv6(whole.to_string());
    let (host, tail) = rest.split_once(']').ok_or_else(invalid)?;
    let host: Ipv6Addr = host.parse().map_err(|_| invalid())?;
    let port = match tail {
        "" => DEFAULT_PORT,
        _ => parse_port(tail.strip_prefix(':').ok_or_else(invalid)?)?,
    };
    Ok(ServerAddress {
        host: host.to_string(),
        port,
    })
}

/// Accepts DNS-ish names and IPv4 literals.
fn validate_host(host: &str) -> Result<String, AddressError> {
    let allowed =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') || !c.is_ascii();
    if host.is_empty() || host.len() > MAX_HOST_LEN || !host.chars().all(allowed) {
        return Err(AddressError::InvalidHost(host.to_string()));
    }
    Ok(host.to_string())
}

fn parse_port(text: &str) -> Result<u16, AddressError> {
    match text.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(AddressError::InvalidPort(text.to_string())),
    }
}

/// One saved server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEntry {
    name: String,
    address: String,
    pack_mode: ResourcePackMode,
}

impl ServerEntry {
    /// Creates an entry; the address is validated and stored trimmed.
    ///
    /// An empty name becomes [`DEFAULT_NAME`]; a name over 64 characters is cut.
    ///
    /// # Errors
    /// [`AddressError`] when `address` does not parse.
    pub fn new(name: &str, address: &str) -> Result<Self, AddressError> {
        parse_address(address)?;
        let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
        Ok(Self {
            name: if name.is_empty() {
                DEFAULT_NAME.to_string()
            } else {
                name
            },
            address: address.trim().to_string(),
            pack_mode: ResourcePackMode::default(),
        })
    }

    /// The entry with another resource pack mode.
    #[must_use]
    pub fn with_pack_mode(mut self, pack_mode: ResourcePackMode) -> Self {
        self.pack_mode = pack_mode;
        self
    }

    /// What to do when this server offers a resource pack.
    #[must_use]
    pub fn pack_mode(&self) -> ResourcePackMode {
        self.pack_mode
    }

    /// The display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The address exactly as the player typed it.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The parsed address, ready to connect or ping.
    #[must_use]
    pub fn socket_address(&self) -> ServerAddress {
        // The constructor validated the text, so the fallback is unreachable;
        // it only keeps this accessor infallible.
        parse_address(&self.address).unwrap_or(ServerAddress {
            host: self.address.clone(),
            port: DEFAULT_PORT,
        })
    }
}

/// Failure to persist the book.
#[derive(Debug, Error)]
pub enum BookError {
    /// The platform data directory is unknown, so there is nowhere to save.
    #[error("no data directory is available to store the server list")]
    NoLocation,
    /// Writing the file failed.
    #[error(transparent)]
    Save(#[from] CoreError),
}

/// On-disk shape of one entry; read leniently, validated on load.
#[derive(Serialize, Deserialize)]
struct StoredEntry {
    name: String,
    address: String,
    /// Absent in files written before the setting existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_packs: Option<String>,
}

impl From<&ServerEntry> for StoredEntry {
    fn from(entry: &ServerEntry) -> Self {
        Self {
            name: entry.name.clone(),
            address: entry.address.clone(),
            resource_packs: (entry.pack_mode != ResourcePackMode::default())
                .then(|| entry.pack_mode.name().to_string()),
        }
    }
}

/// On-disk shape of the `[servers]` section.
#[derive(Serialize, Deserialize, Default)]
struct StoredBook {
    /// Entries are kept as loose values so one broken entry cannot hide the rest.
    #[serde(default)]
    entries: Vec<serde_json::Value>,
}

/// The ordered list of saved servers, bound to its file.
#[derive(Debug, Clone, Default)]
pub struct ServerBook {
    path: Option<PathBuf>,
    entries: Vec<ServerEntry>,
}

impl ServerBook {
    /// Loads `servers.toml` from the data directory.
    ///
    /// Without a data directory the book works in memory and
    /// [`ServerBook::save`] reports [`BookError::NoLocation`].
    #[must_use]
    pub fn open_default() -> Self {
        match umicraft_assets::cache::data_dir() {
            Some(directory) => Self::open(&directory.join(FILE_NAME)),
            None => Self::default(),
        }
    }

    /// Loads the book stored at `path`; never fails.
    ///
    /// A missing file is a normal first run; an unreadable or corrupt file is
    /// logged and treated as empty (the next save replaces it); entries with an
    /// invalid address are skipped.
    #[must_use]
    pub fn open(path: &Path) -> Self {
        let entries = if path.exists() {
            Self::read_entries(path).unwrap_or_else(|error| {
                tracing::warn!(%error, path = %path.display(), "ignoring unreadable server list");
                Vec::new()
            })
        } else {
            Vec::new()
        };
        Self {
            path: Some(path.to_path_buf()),
            entries,
        }
    }

    fn read_entries(path: &Path) -> Result<Vec<ServerEntry>, CoreError> {
        let store = ConfigStore::from_path(path)?;
        let stored: StoredBook = store.section(SECTION)?.unwrap_or_default();
        Ok(stored
            .entries
            .into_iter()
            .filter_map(|value| serde_json::from_value::<StoredEntry>(value).ok())
            .filter_map(|entry| {
                let built = ServerEntry::new(&entry.name, &entry.address).ok()?;
                let mode = entry
                    .resource_packs
                    .as_deref()
                    .and_then(ResourcePackMode::from_name);
                Some(built.with_pack_mode(mode.unwrap_or_default()))
            })
            .collect())
    }

    /// Writes the book atomically.
    ///
    /// # Errors
    /// [`BookError::NoLocation`] for an in-memory book, [`BookError::Save`]
    /// when the file cannot be written.
    pub fn save(&self) -> Result<(), BookError> {
        let path = self.path.as_ref().ok_or(BookError::NoLocation)?;
        let stored = StoredBook {
            entries: self
                .entries
                .iter()
                .filter_map(|entry| serde_json::to_value(StoredEntry::from(entry)).ok())
                .collect(),
        };
        let mut store = ConfigStore::default();
        store.set_section(SECTION, &stored)?;
        store.save(path)?;
        Ok(())
    }

    /// Appends an entry and returns its index.
    ///
    /// # Errors
    /// [`AddressError`] for an invalid address (nothing is added).
    pub fn add(&mut self, name: &str, address: &str) -> Result<usize, AddressError> {
        self.entries.push(ServerEntry::new(name, address)?);
        Ok(self.entries.len() - 1)
    }

    /// Replaces the entry at `index`.
    ///
    /// # Errors
    /// [`EditError::NoSuchEntry`] for a bad index, [`EditError::Address`] for
    /// an invalid address; the entry is untouched in both cases.
    pub fn edit(&mut self, index: usize, name: &str, address: &str) -> Result<(), EditError> {
        let replacement = ServerEntry::new(name, address)?;
        let slot = self
            .entries
            .get_mut(index)
            .ok_or(EditError::NoSuchEntry(index))?;
        *slot = replacement.with_pack_mode(slot.pack_mode);
        Ok(())
    }

    /// Sets the resource pack mode of the entry at `index`; false when out of
    /// range.
    pub fn set_pack_mode(&mut self, index: usize, mode: ResourcePackMode) -> bool {
        self.entries
            .get_mut(index)
            .map(|entry| entry.pack_mode = mode)
            .is_some()
    }

    /// Removes and returns the entry at `index`, or `None` when out of range.
    pub fn remove(&mut self, index: usize) -> Option<ServerEntry> {
        (index < self.entries.len()).then(|| self.entries.remove(index))
    }

    /// Swaps the entry with the one before it; returns the new index, or
    /// `None` when it is already first or out of range (vanilla's "up" arrow).
    pub fn move_up(&mut self, index: usize) -> Option<usize> {
        if index == 0 || index >= self.entries.len() {
            return None;
        }
        self.entries.swap(index, index - 1);
        Some(index - 1)
    }

    /// Swaps the entry with the one after it; returns the new index, or
    /// `None` when it is already last or out of range (vanilla's "down" arrow).
    pub fn move_down(&mut self, index: usize) -> Option<usize> {
        if index + 1 >= self.entries.len() {
            return None;
        }
        self.entries.swap(index, index + 1);
        Some(index + 1)
    }

    /// The entry at `index`.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&ServerEntry> {
        self.entries.get(index)
    }

    /// Number of saved servers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates the entries in display order.
    pub fn iter(&self) -> std::slice::Iter<'_, ServerEntry> {
        self.entries.iter()
    }
}

impl<'a> IntoIterator for &'a ServerBook {
    type Item = &'a ServerEntry;
    type IntoIter = std::slice::Iter<'a, ServerEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Why [`ServerBook::edit`] failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    /// The new address is invalid.
    #[error(transparent)]
    Address(#[from] AddressError),
    /// The index is past the end of the list.
    #[error("there is no server at index {0}")]
    NoSuchEntry(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pack_mode_survives_a_save_and_old_files_read_as_prompt() {
        let folder = std::env::temp_dir().join("umicraft-book-pack-mode");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("temp folder");
        let path = folder.join("servers.toml");
        // A file from before the setting existed.
        std::fs::write(
            &path,
            "[[servers.entries]]\nname = \"Old\"\naddress = \"old.example\"\n",
        )
        .expect("write");
        let mut book = ServerBook::open(&path);
        assert_eq!(
            book.get(0).expect("entry").pack_mode(),
            ResourcePackMode::Prompt
        );
        book.add("New", "new.example").expect("add");
        assert!(book.set_pack_mode(1, ResourcePackMode::Disabled));
        assert!(!book.set_pack_mode(9, ResourcePackMode::Enabled));
        book.save().expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            text.matches("resource_packs").count(),
            1,
            "the default is omitted"
        );
        let reloaded = ServerBook::open(&path);
        assert_eq!(
            reloaded.get(0).expect("old").pack_mode(),
            ResourcePackMode::Prompt
        );
        assert_eq!(
            reloaded.get(1).expect("new").pack_mode(),
            ResourcePackMode::Disabled
        );
    }

    #[test]
    fn editing_keeps_the_pack_mode() {
        let mut book = ServerBook::default();
        book.add("A", "a.example").expect("add");
        book.set_pack_mode(0, ResourcePackMode::Enabled);
        book.edit(0, "B", "b.example").expect("edit");
        assert_eq!(
            book.get(0).expect("entry").pack_mode(),
            ResourcePackMode::Enabled
        );
    }

    fn address(host: &str, port: u16) -> ServerAddress {
        ServerAddress {
            host: host.to_string(),
            port,
        }
    }

    #[test]
    fn parses_valid_addresses() {
        let table = [
            ("example.com", address("example.com", 25565)),
            ("  example.com  ", address("example.com", 25565)),
            ("example.com:25570", address("example.com", 25570)),
            ("127.0.0.1:1", address("127.0.0.1", 1)),
            ("[::1]", address("::1", 25565)),
            ("[::1]:25566", address("::1", 25566)),
            ("::1", address("::1", 25565)),
            ("my_server-1.net:65535", address("my_server-1.net", 65535)),
        ];
        for (input, expected) in table {
            assert_eq!(parse_address(input), Ok(expected), "{input}");
        }
    }

    #[test]
    fn rejects_invalid_addresses() {
        let table = [
            ("", AddressError::Empty),
            ("   ", AddressError::Empty),
            (":25565", AddressError::InvalidHost(String::new())),
            ("a b", AddressError::InvalidHost("a b".into())),
            ("host:0", AddressError::InvalidPort("0".into())),
            ("host:65536", AddressError::InvalidPort("65536".into())),
            ("host:abc", AddressError::InvalidPort("abc".into())),
            ("host:", AddressError::InvalidPort(String::new())),
            ("[::1", AddressError::InvalidIpv6("[::1".into())),
            ("[::1]x", AddressError::InvalidIpv6("[::1]x".into())),
            ("[nope]:1", AddressError::InvalidIpv6("[nope]:1".into())),
        ];
        for (input, expected) in table {
            assert_eq!(parse_address(input), Err(expected), "{input}");
        }
    }

    #[test]
    fn display_round_trips() {
        for text in ["example.com", "example.com:1234", "[::1]", "[::1]:99"] {
            assert_eq!(parse_address(text).unwrap().to_string(), text);
        }
    }

    #[test]
    fn entry_defaults_and_trims_the_name() {
        assert_eq!(ServerEntry::new("  ", "a").unwrap().name(), DEFAULT_NAME);
        assert_eq!(ServerEntry::new(" Hub ", "a").unwrap().name(), "Hub");
        let long = "x".repeat(500);
        assert_eq!(ServerEntry::new(&long, "a").unwrap().name().len(), 64);
        assert!(ServerEntry::new("n", "bad address").is_err());
    }

    fn book_with(names: &[&str]) -> ServerBook {
        let mut book = ServerBook::default();
        for name in names {
            book.add(name, "host").unwrap();
        }
        book
    }

    fn names(book: &ServerBook) -> Vec<&str> {
        book.iter().map(ServerEntry::name).collect()
    }

    #[test]
    fn add_edit_remove() {
        let mut book = book_with(&["a", "b"]);
        assert_eq!(book.add("c", "h:1"), Ok(2));
        assert!(book.add("bad", "").is_err());
        assert_eq!(book.len(), 3);

        book.edit(1, "B", "other:2").unwrap();
        assert_eq!(book.get(1).unwrap().address(), "other:2");
        assert_eq!(book.edit(9, "x", "h"), Err(EditError::NoSuchEntry(9)));
        assert_eq!(
            book.edit(0, "x", "h h"),
            Err(EditError::Address(AddressError::InvalidHost("h h".into())))
        );
        assert_eq!(book.get(0).unwrap().name(), "a");

        assert_eq!(book.remove(0).unwrap().name(), "a");
        assert!(book.remove(5).is_none());
        assert_eq!(names(&book), ["B", "c"]);
    }

    #[test]
    fn reordering_stops_at_the_ends() {
        let mut book = book_with(&["a", "b", "c"]);
        assert_eq!(book.move_up(0), None);
        assert_eq!(book.move_down(2), None);
        assert_eq!(book.move_down(9), None);
        assert_eq!(book.move_up(2), Some(1));
        assert_eq!(names(&book), ["a", "c", "b"]);
        assert_eq!(book.move_down(0), Some(1));
        assert_eq!(names(&book), ["c", "a", "b"]);
    }

    fn temp_file(tag: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("umicraft-servers-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("servers.toml")
    }

    #[test]
    fn persists_and_reloads_in_order() {
        let path = temp_file("persist");
        let mut book = ServerBook::open(&path);
        assert!(book.is_empty());
        book.add("One", "one.example").unwrap();
        book.add("Two \"quoted\"", "[::1]:3").unwrap();
        book.save().unwrap();

        let loaded = ServerBook::open(&path);
        assert_eq!(names(&loaded), ["One", "Two \"quoted\""]);
        assert_eq!(loaded.get(1).unwrap().socket_address(), address("::1", 3));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_gives_an_empty_book_that_can_be_saved_over() {
        let path = temp_file("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is [not toml").unwrap();
        let mut book = ServerBook::open(&path);
        assert!(book.is_empty());
        book.add("Fresh", "h").unwrap();
        book.save().unwrap();
        assert_eq!(names(&ServerBook::open(&path)), ["Fresh"]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_entries_are_skipped() {
        let path = temp_file("skip");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[[servers.entries]]\nname = \"ok\"\naddress = \"a\"\n\n\
             [[servers.entries]]\nname = \"bad\"\naddress = \"a b\"\n\n\
             [[servers.entries]]\nname = 5\n\n\
             [[servers.entries]]\nname = \"ok2\"\naddress = \"b:2\"\n",
        )
        .unwrap();
        assert_eq!(names(&ServerBook::open(&path)), ["ok", "ok2"]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn in_memory_book_cannot_save() {
        assert!(matches!(
            ServerBook::default().save(),
            Err(BookError::NoLocation)
        ));
    }
}
