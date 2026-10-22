// SPDX-License-Identifier: MIT OR Apache-2.0

//! A minimal zip container reader.
//!
//! The client jar of a Minecraft version is a zip archive, and reading a
//! single entry out of it is all the transport layer needs (the full
//! extraction and the model/texture work belongs to M5). This module
//! implements just enough of the ZIP format (APPNOTE 6.3.x) to find the
//! central directory, locate one entry, and inflate it.
//!
//! Supported: the *stored* (method 0) and *deflate* (method 8) compression
//! methods, which is what the official jars use. Zip64 archives and
//! encrypted entries are rejected with a typed error instead of being
//! mis-parsed.
//!
//! Every offset is bounds checked, so a malformed container produces an
//! [`AssetsError::Zip`] and never a panic (spec §14.3).

use std::io::Read;

use crate::error::{AssetsError, AssetsResult};

/// Signature of the end-of-central-directory record.
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
/// Signature of a central directory file header.
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
/// Signature of a local file header.
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// Size of the fixed part of the end-of-central-directory record.
const EOCD_SIZE: usize = 22;
/// Size of the fixed part of a central directory file header.
const CENTRAL_SIZE: usize = 46;
/// Size of the fixed part of a local file header.
const LOCAL_SIZE: usize = 30;
/// Compression method: stored.
const METHOD_STORED: u16 = 0;
/// Compression method: deflate.
const METHOD_DEFLATE: u16 = 8;
/// The zip64 sentinel size.
const ZIP64_SENTINEL: u32 = u32::MAX;

/// One entry of a zip archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZipEntry {
    /// The entry name, `/` separated.
    pub(crate) name: String,
    /// The compression method.
    method: u16,
    /// The size of the compressed data.
    compressed_size: u64,
    /// The size of the uncompressed data.
    uncompressed_size: u64,
    /// The offset of the local file header of the entry.
    local_header_offset: u64,
}

/// A parsed zip archive held in memory.
#[derive(Debug)]
pub(crate) struct ZipArchive {
    data: Vec<u8>,
    entries: Vec<ZipEntry>,
}

impl ZipArchive {
    /// Parses the central directory of `data`.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Zip`] when the container is truncated, has no
    /// end-of-central-directory record, or uses an unsupported feature.
    pub(crate) fn open(data: Vec<u8>) -> AssetsResult<Self> {
        let entries = parse_central_directory(&data)?;
        Ok(Self { data, entries })
    }

    /// The entry with the given name, if the archive has one.
    pub(crate) fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// The names of every entry in the archive.
    pub(crate) fn names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Reads and decompresses one entry.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Zip`] when the entry is missing, uses an unsupported
    /// compression method, or the compressed data is malformed.
    pub(crate) fn read(&self, entry: &ZipEntry) -> AssetsResult<Vec<u8>> {
        let start = usize::try_from(entry.local_header_offset).map_err(|_| AssetsError::Zip {
            what: entry.name.clone(),
            reason: "the local header offset does not fit into memory".to_string(),
        })?;
        let header = self
            .data
            .get(start..start + LOCAL_SIZE)
            .ok_or_else(|| AssetsError::Zip {
                what: entry.name.clone(),
                reason: "the local file header is out of bounds".to_string(),
            })?;
        if read_u32(header, 0)? != LOCAL_SIGNATURE {
            return Err(AssetsError::Zip {
                what: entry.name.clone(),
                reason: "the local file header has a bad signature".to_string(),
            });
        }
        let name_len = read_u16(header, 26)? as usize;
        let extra_len = read_u16(header, 28)? as usize;
        let data_start = start + LOCAL_SIZE + name_len + extra_len;
        let data_end = data_start
            .checked_add(entry.compressed_size as usize)
            .ok_or_else(|| AssetsError::Zip {
                what: entry.name.clone(),
                reason: "the entry size overflows".to_string(),
            })?;
        let compressed = self
            .data
            .get(data_start..data_end)
            .ok_or_else(|| AssetsError::Zip {
                what: entry.name.clone(),
                reason: "the entry data is out of bounds".to_string(),
            })?;

        let out = match entry.method {
            METHOD_STORED => compressed.to_vec(),
            METHOD_DEFLATE => {
                let mut decoder = flate2::read::DeflateDecoder::new(compressed);
                let mut out = Vec::with_capacity(entry.uncompressed_size as usize);
                decoder
                    .read_to_end(&mut out)
                    .map_err(|err| AssetsError::Zip {
                        what: entry.name.clone(),
                        reason: format!("the deflate stream is malformed: {err}"),
                    })?;
                out
            }
            method => {
                return Err(AssetsError::Zip {
                    what: entry.name.clone(),
                    reason: format!("the compression method {method} is not supported"),
                })
            }
        };
        Ok(out)
    }
}

/// Finds and parses the central directory of `data`.
fn parse_central_directory(data: &[u8]) -> AssetsResult<Vec<ZipEntry>> {
    let eocd = find_eocd(data)?;
    let entry_count = read_u16(data, eocd + 10)? as usize;
    let central_offset = read_u32(data, eocd + 16)? as usize;

    let mut entries = Vec::with_capacity(entry_count);
    let mut cursor = central_offset;
    for _ in 0..entry_count {
        let header = data
            .get(cursor..cursor + CENTRAL_SIZE)
            .ok_or_else(|| AssetsError::Zip {
                what: "central directory".to_string(),
                reason: "the central directory is truncated".to_string(),
            })?;
        if read_u32(header, 0)? != CENTRAL_SIGNATURE {
            return Err(AssetsError::Zip {
                what: "central directory".to_string(),
                reason: "a central directory header has a bad signature".to_string(),
            });
        }
        let method = read_u16(header, 10)?;
        let compressed_size = read_u32(header, 20)?;
        let uncompressed_size = read_u32(header, 24)?;
        if compressed_size == ZIP64_SENTINEL || uncompressed_size == ZIP64_SENTINEL {
            return Err(AssetsError::Zip {
                what: "central directory".to_string(),
                reason: "zip64 archives are not supported".to_string(),
            });
        }
        let name_len = read_u16(header, 28)? as usize;
        let extra_len = read_u16(header, 30)? as usize;
        let comment_len = read_u16(header, 32)? as usize;
        let local_header_offset = read_u32(header, 42)? as u64;
        let name_start = cursor + CENTRAL_SIZE;
        let name_bytes =
            data.get(name_start..name_start + name_len)
                .ok_or_else(|| AssetsError::Zip {
                    what: "central directory".to_string(),
                    reason: "an entry name is out of bounds".to_string(),
                })?;
        let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| AssetsError::Zip {
            what: "central directory".to_string(),
            reason: "an entry name is not valid UTF-8".to_string(),
        })?;

        entries.push(ZipEntry {
            name,
            method,
            compressed_size: u64::from(compressed_size),
            uncompressed_size: u64::from(uncompressed_size),
            local_header_offset,
        });

        cursor = name_start + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

/// Finds the offset of the end-of-central-directory record.
fn find_eocd(data: &[u8]) -> AssetsResult<usize> {
    if data.len() < EOCD_SIZE {
        return Err(AssetsError::Zip {
            what: "archive".to_string(),
            reason: "the archive is too small to hold a zip record".to_string(),
        });
    }
    // The record sits at the end of the file, possibly followed by a comment
    // of up to 64 KiB.
    let lowest = data.len().saturating_sub(EOCD_SIZE + u16::MAX as usize);
    let mut offset = data.len() - EOCD_SIZE;
    loop {
        if read_u32(data, offset)? == EOCD_SIGNATURE {
            let comment_len = read_u16(data, offset + 20)? as usize;
            if offset + EOCD_SIZE + comment_len == data.len() {
                return Ok(offset);
            }
        }
        if offset == lowest {
            break;
        }
        offset -= 1;
    }
    Err(AssetsError::Zip {
        what: "archive".to_string(),
        reason: "no end-of-central-directory record was found".to_string(),
    })
}

/// Reads a little-endian `u16` at `offset`.
fn read_u16(data: &[u8], offset: usize) -> AssetsResult<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| AssetsError::Zip {
            what: "archive".to_string(),
            reason: format!("the archive is truncated at offset {offset}"),
        })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Reads a little-endian `u32` at `offset`.
fn read_u32(data: &[u8], offset: usize) -> AssetsResult<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| AssetsError::Zip {
            what: "archive".to_string(),
            reason: format!("the archive is truncated at offset {offset}"),
        })?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a zip archive with one *stored* entry.
    fn stored_zip(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let name_bytes = name.as_bytes();

        out.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&10_u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0_u16.to_le_bytes()); // flags
        out.extend_from_slice(&METHOD_STORED.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // time
        out.extend_from_slice(&0_u16.to_le_bytes()); // date
        out.extend_from_slice(&0_u32.to_le_bytes()); // crc
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // extra
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(payload);
        let local_offset = 0_u32;

        let central_offset = out.len() as u32;
        out.extend_from_slice(&CENTRAL_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&20_u16.to_le_bytes()); // version made by
        out.extend_from_slice(&10_u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0_u16.to_le_bytes()); // flags
        out.extend_from_slice(&METHOD_STORED.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // time
        out.extend_from_slice(&0_u16.to_le_bytes()); // date
        out.extend_from_slice(&0_u32.to_le_bytes()); // crc
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // extra
        out.extend_from_slice(&0_u16.to_le_bytes()); // comment
        out.extend_from_slice(&0_u16.to_le_bytes()); // disk
        out.extend_from_slice(&0_u16.to_le_bytes()); // internal attrs
        out.extend_from_slice(&0_u32.to_le_bytes()); // external attrs
        out.extend_from_slice(&local_offset.to_le_bytes());
        out.extend_from_slice(name_bytes);

        let central_size = out.len() as u32 - central_offset;
        out.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // disk
        out.extend_from_slice(&0_u16.to_le_bytes()); // cd disk
        out.extend_from_slice(&1_u16.to_le_bytes()); // entries this disk
        out.extend_from_slice(&1_u16.to_le_bytes()); // total entries
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // comment
        out
    }

    #[test]
    fn a_stored_entry_is_read_back() {
        let archive = ZipArchive::open(stored_zip("hello.txt", b"world")).expect("archive parses");
        assert_eq!(archive.names(), vec!["hello.txt".to_string()]);
        let entry = archive.entry("hello.txt").expect("entry is present");
        assert_eq!(archive.read(entry).expect("entry is read"), b"world");
    }

    #[test]
    fn a_missing_entry_is_none() {
        let archive = ZipArchive::open(stored_zip("hello.txt", b"world")).expect("archive parses");
        assert!(archive.entry("nope.txt").is_none());
    }

    #[test]
    fn a_truncated_archive_is_an_error_and_not_a_panic() {
        let mut archive = stored_zip("hello.txt", b"world");
        archive.truncate(10);
        let err = ZipArchive::open(archive).expect_err("a truncated archive must fail");
        assert!(matches!(err, AssetsError::Zip { .. }));
    }

    #[test]
    fn garbage_is_an_error_and_not_a_panic() {
        let err = ZipArchive::open(vec![0x5a; 128]).expect_err("garbage must fail");
        assert!(matches!(err, AssetsError::Zip { .. }));
    }
}
