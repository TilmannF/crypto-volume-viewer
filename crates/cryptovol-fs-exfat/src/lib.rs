//! Read-only exFAT directory and file access for decrypted block devices.
//!
//! This crate implements boot sector parsing, cluster mapping, directory entry
//! set parsing, and single-file extraction for exFAT-formatted volumes. It
//! does not support write operations, directory extraction, or mounting.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use direct synthetic fixture assertions"
    )
)]

use cryptovol_core::{BlockReader, CryptovolError, EXTRACTION_CHUNK_SIZE};
use std::fmt;
use thiserror::Error;

/// Errors returned by the exFAT reader.
#[derive(Debug, Error)]
pub enum ExfatError {
    /// The boot sector is invalid or does not identify as exFAT.
    #[error("invalid exFAT boot sector: {reason}")]
    InvalidBootSector {
        /// Human-readable reason for the rejection.
        reason: &'static str,
    },
    /// A cluster number is out of the valid range (must be >= 2).
    #[error("invalid cluster number {cluster}")]
    InvalidClusterNumber {
        /// The rejected cluster number.
        cluster: u32,
    },
    /// A FAT chain entry points to an invalid or out-of-range cluster.
    #[error("invalid cluster chain")]
    InvalidClusterChain,
    /// A FAT chain contains a cycle.
    #[error("FAT chain cycle detected")]
    FatChainCycle,
    /// A directory entry set is malformed or truncated.
    #[error("malformed directory entry set")]
    MalformedEntrySet,
    /// A File Directory Entry is missing its required Stream Extension secondary.
    #[error("missing stream extension in entry set")]
    MissingStreamExtension,
    /// A File Name Entry is invalid.
    #[error("invalid filename entry")]
    InvalidFilenameEntry,
    /// A filename contains invalid UTF-16 code units.
    #[error("invalid UTF-16 in filename")]
    InvalidUtf16,
    /// The requested path was not found in the volume.
    #[error("path not found: {path}")]
    PathNotFound {
        /// The container path that was requested.
        path: String,
    },
    /// The caller attempted to extract a directory as a file.
    #[error("cannot extract a directory: {path}")]
    AttemptedDirectoryExtraction {
        /// The directory path that was passed to a file-read operation.
        path: String,
    },
    /// An offset or length calculation exceeded the volume bounds.
    #[error("out-of-bounds read")]
    OutOfBoundsRead,
    /// The volume does not identify as exFAT.
    #[error("unsupported filesystem")]
    UnsupportedFilesystem,
    /// An underlying block read failed.
    #[error("read error: {0}")]
    ReadError(#[from] CryptovolError),
    /// A write to the destination failed during streaming extraction.
    #[error("write error: {0}")]
    WriteError(std::io::Error),
}

/// File-system attributes for an exFAT directory entry.
#[derive(Debug, Clone, Default)]
pub struct ExfatAttributes {
    /// Entry is read-only.
    pub read_only: bool,
    /// Entry is hidden.
    pub hidden: bool,
    /// Entry is a system file.
    pub system: bool,
    /// Entry is a directory.
    pub directory: bool,
    /// Entry has the archive bit set.
    pub archive: bool,
}

/// A decoded exFAT timestamp (DOS-style; timezone offset not applied).
#[derive(Debug, Clone)]
pub struct ExfatTimestamp {
    /// Four-digit year (e.g. 2026).
    pub year: u16,
    /// Month 1–12.
    pub month: u8,
    /// Day of month 1–31.
    pub day: u8,
    /// Hour 0–23.
    pub hour: u8,
    /// Minute 0–59.
    pub minute: u8,
    /// Second 0–59 (double-second field × 2).
    pub second: u8,
}

/// A decoded exFAT directory entry (file or directory).
#[derive(Debug, Clone)]
pub struct ExfatEntry {
    /// On-disk filename, decoded from UTF-16LE without normalization.
    pub name: String,
    /// `true` when this entry represents a directory.
    pub is_dir: bool,
    /// Valid data length in bytes (declared by the Stream Extension entry).
    pub size: u64,
    /// File-system attributes parsed from the File Directory Entry.
    pub attributes: ExfatAttributes,
    /// Creation timestamp, if present and non-zero.
    pub created: Option<ExfatTimestamp>,
    /// Last-modified timestamp, if present and non-zero.
    pub modified: Option<ExfatTimestamp>,
    /// Last-accessed timestamp, if present and non-zero.
    pub accessed: Option<ExfatTimestamp>,
    /// First cluster of the file or directory data (from the Stream Extension).
    pub(crate) first_cluster: u32,
    /// True when the NoFatChain flag is set in the Stream Extension.
    pub(crate) no_fat_chain: bool,
}

impl ExfatEntry {
    /// Constructs an entry from its public metadata fields; internal cluster fields are zeroed.
    ///
    /// Useful for tests and display-only contexts where cluster traversal is not needed.
    pub fn from_metadata(
        name: String,
        is_dir: bool,
        size: u64,
        attributes: ExfatAttributes,
        created: Option<ExfatTimestamp>,
        modified: Option<ExfatTimestamp>,
        accessed: Option<ExfatTimestamp>,
    ) -> Self {
        Self {
            name,
            is_dir,
            size,
            attributes,
            created,
            modified,
            accessed,
            first_cluster: 0,
            no_fat_chain: false,
        }
    }
}

/// Parsed and validated exFAT boot sector fields.
///
/// All derived byte offsets are pre-computed to avoid repeated shifts.
#[allow(dead_code)]
struct BootSector {
    /// Volume length in sectors.
    volume_length: u64,
    /// FAT start in sectors from the volume start.
    fat_offset: u32,
    /// FAT length in sectors.
    fat_length: u32,
    /// Cluster heap start in sectors from the volume start.
    cluster_heap_offset: u32,
    /// Number of data clusters (first cluster is index 2).
    cluster_count: u32,
    /// Cluster index of the root directory (always >= 2).
    first_root_cluster: u32,
    /// Raw shift value: bytes per sector = 1 << this.
    bytes_per_sector_shift: u8,
    /// Raw shift value: sectors per cluster = 1 << this.
    sectors_per_cluster_shift: u8,
    /// Number of FAT tables (1 or 2).
    number_of_fats: u8,
    /// Bytes per sector (derived).
    bytes_per_sector: u64,
    /// Bytes per cluster (derived).
    cluster_size: u64,
    /// Byte offset of the cluster heap from the start of the volume (derived).
    cluster_heap_byte_offset: u64,
    /// Byte offset of the first FAT from the start of the volume (derived).
    fat_byte_offset: u64,
}

/// Reads a `u64` in little-endian byte order from `buf[offset..offset+8]`.
///
/// # Panics
///
/// Panics if `offset + 8 > buf.len()`.  Callers must ensure the buffer is
/// large enough before calling (enforced by the `buf.len() >= 512` check in
/// [`parse_boot_sector`]).
fn read_u64_le(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}

/// Reads a `u32` in little-endian byte order from `buf[offset..offset+4]`.
///
/// # Panics
///
/// Panics if `offset + 4 > buf.len()`.  Callers must ensure the buffer is
/// large enough before calling (enforced by the `buf.len() >= 512` check in
/// [`parse_boot_sector`]).
fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Returns the u16 at `offset` in little-endian order.
///
/// Panics if `offset + 2 > buf.len()`.  Callers must bounds-check first.
fn read_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

/// Parses and validates the 512-byte exFAT boot sector at the start of `buf`.
fn parse_boot_sector(buf: &[u8]) -> Result<BootSector, ExfatError> {
    if buf.len() < 512 {
        return Err(ExfatError::InvalidBootSector {
            reason: "boot sector buffer shorter than 512 bytes",
        });
    }

    // OEM name at bytes 3..11 must be exactly b"EXFAT   ".
    if &buf[3..11] != b"EXFAT   " {
        return Err(ExfatError::InvalidBootSector {
            reason: "not an exFAT volume",
        });
    }

    // BytesPerSectorShift (byte 108): valid range 9–12 (512–4096 bytes/sector).
    let bytes_per_sector_shift = buf[108];
    if !(9..=12).contains(&bytes_per_sector_shift) {
        return Err(ExfatError::InvalidBootSector {
            reason: "invalid BytesPerSectorShift",
        });
    }

    // SectorsPerClusterShift (byte 109): valid range 0–25,
    // and the sum of both shifts must not exceed 25.
    let sectors_per_cluster_shift = buf[109];
    if sectors_per_cluster_shift > 25
        || u16::from(bytes_per_sector_shift) + u16::from(sectors_per_cluster_shift) > 25
    {
        return Err(ExfatError::InvalidBootSector {
            reason: "invalid SectorsPerClusterShift",
        });
    }

    // NumberOfFats (byte 110): must be 1 or 2.
    let number_of_fats = buf[110];
    if number_of_fats == 0 || number_of_fats > 2 {
        return Err(ExfatError::InvalidBootSector {
            reason: "invalid NumberOfFats",
        });
    }

    // Parse geometry fields (buf.len() >= 512 is guaranteed above).
    let volume_length = read_u64_le(buf, 72);
    let fat_offset = read_u32_le(buf, 80);
    let fat_length = read_u32_le(buf, 84);
    let cluster_heap_offset = read_u32_le(buf, 88);
    let cluster_count = read_u32_le(buf, 92);
    let first_root_cluster = read_u32_le(buf, 96);

    // Root cluster must be >= 2.
    if first_root_cluster < 2 {
        return Err(ExfatError::InvalidBootSector {
            reason: "invalid root cluster",
        });
    }

    // Compute derived fields.
    let bytes_per_sector = 1u64 << bytes_per_sector_shift;
    let cluster_size = bytes_per_sector << sectors_per_cluster_shift;
    let fat_byte_offset = u64::from(fat_offset) * bytes_per_sector;
    let cluster_heap_byte_offset = u64::from(cluster_heap_offset) * bytes_per_sector;

    // FAT region must lie within the volume (checked arithmetic).
    let fat_end = (fat_offset as u64).checked_add(fat_length as u64).ok_or(
        ExfatError::InvalidBootSector {
            reason: "FAT region overflows u64",
        },
    )?;
    if fat_end > volume_length {
        return Err(ExfatError::InvalidBootSector {
            reason: "FAT region extends beyond volume",
        });
    }

    // Cluster heap must lie within the volume (checked arithmetic).
    let cluster_sectors = (cluster_count as u64)
        .checked_mul(1u64 << sectors_per_cluster_shift)
        .ok_or(ExfatError::InvalidBootSector {
            reason: "cluster heap size overflows u64",
        })?;
    let heap_end = (cluster_heap_offset as u64)
        .checked_add(cluster_sectors)
        .ok_or(ExfatError::InvalidBootSector {
            reason: "cluster heap end overflows u64",
        })?;
    if heap_end > volume_length {
        return Err(ExfatError::InvalidBootSector {
            reason: "cluster heap extends beyond volume",
        });
    }

    Ok(BootSector {
        volume_length,
        fat_offset,
        fat_length,
        cluster_heap_offset,
        cluster_count,
        first_root_cluster,
        bytes_per_sector_shift,
        sectors_per_cluster_shift,
        number_of_fats,
        bytes_per_sector,
        cluster_size,
        cluster_heap_byte_offset,
        fat_byte_offset,
    })
}

/// Maps a cluster index to its byte offset within the cluster heap.
///
/// Returns [`ExfatError::InvalidClusterNumber`] for cluster < 2 and
/// [`ExfatError::OutOfBoundsRead`] on arithmetic overflow.
pub(crate) fn cluster_to_byte_offset(
    heap_byte_offset: u64,
    cluster_size: u64,
    cluster: u32,
) -> Result<u64, ExfatError> {
    if cluster < 2 {
        return Err(ExfatError::InvalidClusterNumber { cluster });
    }
    let index = u64::from(cluster) - 2;
    let span = index
        .checked_mul(cluster_size)
        .ok_or(ExfatError::OutOfBoundsRead)?;
    heap_byte_offset
        .checked_add(span)
        .ok_or(ExfatError::OutOfBoundsRead)
}

/// Walks a FAT chain and returns the ordered list of cluster indices.
///
/// `fat_bytes` is the raw FAT region (each entry is 4 bytes, little-endian).
/// Valid cluster indices are 2..=`cluster_count`+1; out-of-range entries return
/// [`ExfatError::InvalidClusterChain`].  Cycles return [`ExfatError::FatChainCycle`].
pub(crate) fn collect_fat_chain(
    fat_bytes: &[u8],
    start: u32,
    cluster_count: u32,
) -> Result<Vec<u32>, ExfatError> {
    use std::collections::HashSet;

    const FAT_EOF: u32 = 0xFFFF_FFFF;
    let max_valid = cluster_count.saturating_add(1);

    let mut chain: Vec<u32> = Vec::new();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut current = start;

    loop {
        if !visited.insert(current) {
            return Err(ExfatError::FatChainCycle);
        }
        chain.push(current);

        let entry_offset = (current as usize)
            .checked_mul(4)
            .ok_or(ExfatError::InvalidClusterChain)?;
        let entry_end = entry_offset
            .checked_add(4)
            .ok_or(ExfatError::InvalidClusterChain)?;
        let entry_bytes = fat_bytes
            .get(entry_offset..entry_end)
            .ok_or(ExfatError::InvalidClusterChain)?;
        let next = read_u32_le(entry_bytes, 0);

        if next == FAT_EOF {
            break;
        }
        if next < 2 || next > max_valid {
            return Err(ExfatError::InvalidClusterChain);
        }
        current = next;
    }

    Ok(chain)
}

/// Reads exactly `data_length` bytes from a contiguous cluster run.
///
/// Cluster `first_cluster` must be ≥ 2. Returns [`ExfatError::OutOfBoundsRead`]
/// if the byte range exceeds the volume length.
pub(crate) fn read_contiguous_clusters(
    reader: &dyn BlockReader,
    heap_offset: u64,
    cluster_size: u64,
    first_cluster: u32,
    data_length: u64,
) -> Result<Vec<u8>, ExfatError> {
    let byte_offset = cluster_to_byte_offset(heap_offset, cluster_size, first_cluster)?;
    let end = byte_offset
        .checked_add(data_length)
        .ok_or(ExfatError::OutOfBoundsRead)?;
    if end > reader.len() {
        return Err(ExfatError::OutOfBoundsRead);
    }
    let len = usize::try_from(data_length).map_err(|_| ExfatError::OutOfBoundsRead)?;
    let mut buf = vec![0u8; len];
    reader.read_at(byte_offset, &mut buf)?;
    Ok(buf)
}

/// Streams `data_length` bytes from a contiguous (NoFatChain) cluster run to `writer`.
///
/// Returns immediately with `Ok(0)` when `data_length == 0`, avoiding any cluster
/// offset calculation for zero-byte files (whose `first_cluster` may be a placeholder).
/// Data is read in chunks bounded by [`EXTRACTION_CHUNK_SIZE`] to cap peak RAM.
fn stream_contiguous_clusters<W: std::io::Write>(
    reader: &dyn BlockReader,
    heap_offset: u64,
    cluster_size: u64,
    first_cluster: u32,
    data_length: u64,
    writer: &mut W,
) -> Result<u64, ExfatError> {
    if data_length == 0 {
        return Ok(0);
    }
    let byte_offset = cluster_to_byte_offset(heap_offset, cluster_size, first_cluster)?;
    let end = byte_offset
        .checked_add(data_length)
        .ok_or(ExfatError::OutOfBoundsRead)?;
    if end > reader.len() {
        return Err(ExfatError::OutOfBoundsRead);
    }
    let mut buf = vec![0u8; EXTRACTION_CHUNK_SIZE];
    let mut remaining = data_length;
    let mut pos = byte_offset;
    let mut written: u64 = 0;
    while remaining > 0 {
        let chunk = remaining.min(EXTRACTION_CHUNK_SIZE as u64) as usize;
        reader.read_at(pos, &mut buf[..chunk])?;
        writer
            .write_all(&buf[..chunk])
            .map_err(ExfatError::WriteError)?;
        pos += chunk as u64;
        remaining -= chunk as u64;
        written += chunk as u64;
    }
    Ok(written)
}

/// Streams `data_length` bytes from a FAT-chained cluster run to `writer`.
///
/// **FAT-table memory**: reads the entire FAT region (`cluster_count + 2` × 4 bytes)
/// once to resolve the cluster chain. This is volume-metadata sized (proportional to
/// volume cluster count, not to the file being extracted) and is bounded by the
/// volume's own FAT region. File data is then streamed cluster-by-cluster in chunks
/// bounded by [`EXTRACTION_CHUNK_SIZE`].
#[allow(clippy::too_many_arguments)]
fn stream_fat_chained_clusters<W: std::io::Write>(
    reader: &dyn BlockReader,
    fat_offset: u64,
    heap_offset: u64,
    cluster_size: u64,
    start_cluster: u32,
    cluster_count: u32,
    data_length: u64,
    writer: &mut W,
) -> Result<u64, ExfatError> {
    let fat_entries = u64::from(cluster_count.saturating_add(2));
    let fat_size = fat_entries
        .checked_mul(4)
        .ok_or(ExfatError::OutOfBoundsRead)?;
    let fat_end = fat_offset
        .checked_add(fat_size)
        .ok_or(ExfatError::OutOfBoundsRead)?;
    if fat_end > reader.len() {
        return Err(ExfatError::OutOfBoundsRead);
    }
    let fat_size_usize = usize::try_from(fat_size).map_err(|_| ExfatError::OutOfBoundsRead)?;
    let mut fat_bytes = vec![0u8; fat_size_usize];
    reader.read_at(fat_offset, &mut fat_bytes)?;

    let chain = collect_fat_chain(&fat_bytes, start_cluster, cluster_count)?;

    let cluster_size_usize =
        usize::try_from(cluster_size).map_err(|_| ExfatError::OutOfBoundsRead)?;
    let buf_size = cluster_size_usize.min(EXTRACTION_CHUNK_SIZE);
    let mut buf = vec![0u8; buf_size];
    let mut remaining = usize::try_from(data_length).map_err(|_| ExfatError::OutOfBoundsRead)?;
    let mut written: u64 = 0;

    for &cluster in &chain {
        if remaining == 0 {
            break;
        }
        let byte_offset = cluster_to_byte_offset(heap_offset, cluster_size, cluster)?;
        let to_emit = remaining.min(cluster_size_usize);
        let mut cluster_pos: usize = 0;
        while cluster_pos < to_emit {
            let chunk = (to_emit - cluster_pos).min(buf_size);
            reader.read_at(byte_offset + cluster_pos as u64, &mut buf[..chunk])?;
            writer
                .write_all(&buf[..chunk])
                .map_err(ExfatError::WriteError)?;
            written += chunk as u64;
            cluster_pos += chunk;
        }
        remaining -= to_emit;
    }
    Ok(written)
}

/// Decodes a UTF-16LE code-unit sequence into a [`String`].
///
/// Surrogate pairs are resolved by [`char::decode_utf16`].  A lone surrogate
/// returns [`ExfatError::InvalidUtf16`].  The output is never NFC-normalised —
/// the on-disk sequence is preserved exactly.
pub(crate) fn decode_utf16le(units: &[u16]) -> Result<String, ExfatError> {
    char::decode_utf16(units.iter().cloned())
        .map(|r| r.map_err(|_| ExfatError::InvalidUtf16))
        .collect()
}

/// Decodes an exFAT DOS-style timestamp field.
///
/// Returns `None` for a zero timestamp or one with a zero month/day.
/// Format: bits 31–25 = year−1980, 24–21 = month, 20–16 = day,
/// 15–11 = hour, 10–5 = minute, 4–0 = double-seconds (seconds ÷ 2).
fn decode_timestamp(ts: u32) -> Option<ExfatTimestamp> {
    if ts == 0 {
        return None;
    }
    let year = ((ts >> 25) & 0x7F) as u16 + 1980;
    let month = ((ts >> 21) & 0x0F) as u8;
    let day = ((ts >> 16) & 0x1F) as u8;
    let hour = ((ts >> 11) & 0x1F) as u8;
    let minute = ((ts >> 5) & 0x3F) as u8;
    let second = ((ts & 0x1F) * 2) as u8;
    if month == 0 || day == 0 {
        return None;
    }
    Some(ExfatTimestamp {
        year,
        month,
        day,
        hour,
        minute,
        second,
    })
}

/// Computes the exFAT entry-set checksum over `entry_count` consecutive 32-byte entries.
///
/// Bytes 2–3 of the first entry (the `SetChecksum` field) are skipped per spec.
pub(crate) fn entry_set_checksum(entries: &[u8], entry_count: usize) -> u16 {
    let mut checksum: u16 = 0;
    let checksum_len = entry_count * 32;
    for (i, byte) in entries[..checksum_len].iter().enumerate() {
        if i == 2 || i == 3 {
            continue; // SetChecksum field is excluded from its own computation
        }
        checksum = checksum.rotate_right(1).wrapping_add(u16::from(*byte));
    }
    checksum
}

/// Parses a raw directory data blob into a list of [`ExfatEntry`] values.
///
/// Iterates through 32-byte slots.  Entry type 0x00 terminates the scan;
/// entries with bit 7 clear are inactive/deleted and are skipped.
/// Returns [`ExfatError::MalformedEntrySet`] on truncated entry sets and
/// [`ExfatError::MissingStreamExtension`] when a file entry is not immediately
/// followed by a Stream Extension (type 0xC0).
pub(crate) fn parse_directory_entries(bytes: &[u8]) -> Result<Vec<ExfatEntry>, ExfatError> {
    let mut result = Vec::new();
    let mut i = 0usize;

    while i + 32 <= bytes.len() {
        let entry_type = bytes[i];

        if entry_type == 0x00 {
            break;
        }
        if (entry_type & 0x80) == 0 {
            i += 32;
            continue;
        }
        if entry_type != 0x85 {
            // Unknown in-use primary type — skip.
            i += 32;
            continue;
        }

        // ── File Directory Entry (type 0x85) ────────────────────────────────
        let secondary_count = bytes[i + 1] as usize;

        // Stream Extension is mandatory as the first secondary entry.
        if secondary_count < 1 {
            return Err(ExfatError::MissingStreamExtension);
        }
        let stream_off = i + 32;
        if stream_off + 32 > bytes.len() {
            return Err(ExfatError::MalformedEntrySet);
        }
        if bytes[stream_off] != 0xC0 {
            return Err(ExfatError::MissingStreamExtension);
        }

        // Bounds-check the full entry set before accessing any secondary slot.
        let total_size = (1 + secondary_count)
            .checked_mul(32)
            .ok_or(ExfatError::MalformedEntrySet)?;
        let set_end = i
            .checked_add(total_size)
            .ok_or(ExfatError::MalformedEntrySet)?;
        if set_end > bytes.len() {
            return Err(ExfatError::MalformedEntrySet);
        }

        // Validate SetChecksum (u16 LE at primary_entry[2..4]).
        let stored_checksum = read_u16_le(bytes, i + 2);
        let computed_checksum = entry_set_checksum(&bytes[i..set_end], 1 + secondary_count);
        if stored_checksum != computed_checksum {
            return Err(ExfatError::MalformedEntrySet);
        }

        let file_attributes = read_u16_le(bytes, i + 4);
        let create_ts = read_u32_le(bytes, i + 8);
        let modified_ts = read_u32_le(bytes, i + 12);
        let accessed_ts = read_u32_le(bytes, i + 16);

        // ── Stream Extension (type 0xC0) ─────────────────────────────────────
        let general_secondary_flags = bytes[stream_off + 1];
        let no_fat_chain = (general_secondary_flags & 0x02) != 0;
        let name_length = bytes[stream_off + 3] as usize;
        let valid_data_length = read_u64_le(bytes, stream_off + 8);
        let data_length = read_u64_le(bytes, stream_off + 24);
        let first_cluster = read_u32_le(bytes, stream_off + 20);

        // ── File Name entries (type 0xC1) ────────────────────────────────────
        let mut utf16_units: Vec<u16> = Vec::new();
        for j in 2..=secondary_count {
            let ne_off = i + j * 32;
            if bytes[ne_off] != 0xC1 {
                return Err(ExfatError::InvalidFilenameEntry);
            }
            for k in 0..15usize {
                utf16_units.push(read_u16_le(bytes, ne_off + 2 + k * 2));
            }
        }
        utf16_units.truncate(name_length);
        let name = decode_utf16le(&utf16_units)?;

        // ── Attributes and entry ─────────────────────────────────────────────
        let attributes = ExfatAttributes {
            read_only: (file_attributes & 0x01) != 0,
            hidden: (file_attributes & 0x02) != 0,
            system: (file_attributes & 0x04) != 0,
            directory: (file_attributes & 0x10) != 0,
            archive: (file_attributes & 0x20) != 0,
        };
        let is_dir = attributes.directory;
        // For files, ValidDataLength is the logical file size.
        // For directories, ValidDataLength may be 0; use DataLength as fallback.
        let size = if is_dir {
            valid_data_length.max(data_length)
        } else {
            valid_data_length
        };

        result.push(ExfatEntry {
            name,
            is_dir,
            size,
            attributes,
            created: decode_timestamp(create_ts),
            modified: decode_timestamp(modified_ts),
            accessed: decode_timestamp(accessed_ts),
            first_cluster,
            no_fat_chain,
        });

        i += total_size;
    }

    Ok(result)
}

/// Read-only exFAT filesystem mounted over a [`BlockReader`].
pub struct ExfatFileSystem<R> {
    reader: R,
    boot: BootSector,
}

impl<R> fmt::Debug for ExfatFileSystem<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExfatFileSystem").finish_non_exhaustive()
    }
}

impl<R: BlockReader> ExfatFileSystem<R> {
    /// Opens an exFAT filesystem by reading and validating the boot sector.
    ///
    /// # Errors
    ///
    /// Returns [`ExfatError::InvalidBootSector`] if the first 512 bytes do not
    /// describe a valid exFAT volume. Returns [`ExfatError::ReadError`] on I/O
    /// failure.
    pub fn open(reader: R) -> Result<Self, ExfatError> {
        let mut buf = [0u8; 512];
        reader.read_at(0, &mut buf)?;
        let boot = parse_boot_sector(&buf)?;
        Ok(Self { reader, boot })
    }

    fn cluster_size(&self) -> u64 {
        self.boot.cluster_size
    }

    fn cluster_heap_offset(&self) -> u64 {
        self.boot.cluster_heap_byte_offset
    }

    fn fat_offset(&self) -> u64 {
        self.boot.fat_byte_offset
    }

    fn cluster_count(&self) -> u32 {
        self.boot.cluster_count
    }

    /// Reads all clusters for a directory via FAT chain, capped at 256 clusters.
    fn read_dir_cluster_chain(&self, first_cluster: u32) -> Result<Vec<u8>, ExfatError> {
        if first_cluster < 2 {
            return Ok(Vec::new());
        }
        let fat_entries = u64::from(self.cluster_count().saturating_add(2));
        let fat_size = fat_entries
            .checked_mul(4)
            .ok_or(ExfatError::OutOfBoundsRead)?;
        let fat_sz = usize::try_from(fat_size).map_err(|_| ExfatError::OutOfBoundsRead)?;
        let mut fat_bytes = vec![0u8; fat_sz];
        self.reader.read_at(self.fat_offset(), &mut fat_bytes)?;

        let chain = collect_fat_chain(&fat_bytes, first_cluster, self.cluster_count())?;
        let chain_len = chain.len().min(256);

        let cs_usize =
            usize::try_from(self.cluster_size()).map_err(|_| ExfatError::OutOfBoundsRead)?;
        let mut result = Vec::with_capacity(chain_len * cs_usize);
        for &cluster in &chain[..chain_len] {
            let byte_off =
                cluster_to_byte_offset(self.cluster_heap_offset(), self.cluster_size(), cluster)?;
            let end = byte_off
                .checked_add(self.cluster_size())
                .ok_or(ExfatError::OutOfBoundsRead)?;
            if end > self.reader.len() {
                return Err(ExfatError::OutOfBoundsRead);
            }
            let mut buf = vec![0u8; cs_usize];
            self.reader.read_at(byte_off, &mut buf)?;
            result.extend_from_slice(&buf);
        }
        Ok(result)
    }

    /// Reads raw directory bytes, using contiguous read when `no_fat_chain && size > 0`,
    /// otherwise following the FAT chain.
    fn read_dir_bytes(
        &self,
        first_cluster: u32,
        size: u64,
        no_fat_chain: bool,
    ) -> Result<Vec<u8>, ExfatError> {
        if no_fat_chain && size > 0 {
            read_contiguous_clusters(
                &self.reader,
                self.cluster_heap_offset(),
                self.cluster_size(),
                first_cluster,
                size,
            )
        } else {
            self.read_dir_cluster_chain(first_cluster)
        }
    }

    /// Parses directory entries at `first_cluster` using the supplied allocation flags.
    fn list_dir_at(
        &self,
        first_cluster: u32,
        size: u64,
        no_fat_chain: bool,
    ) -> Result<Vec<ExfatEntry>, ExfatError> {
        let bytes = self.read_dir_bytes(first_cluster, size, no_fat_chain)?;
        parse_directory_entries(&bytes)
    }

    /// Resolves `path` to an [`ExfatEntry`].
    ///
    /// An empty path or `"/"` returns a synthetic root entry.  Each `/`-separated
    /// segment is looked up with an exact case-preserving match, falling back to
    /// ASCII case-insensitive.
    fn resolve_path(&self, path: &str) -> Result<ExfatEntry, ExfatError> {
        let segments: Vec<&str> = path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if segments.is_empty() {
            return Ok(ExfatEntry {
                name: String::new(),
                is_dir: true,
                size: 0,
                attributes: ExfatAttributes {
                    read_only: false,
                    hidden: false,
                    system: false,
                    directory: true,
                    archive: false,
                },
                created: None,
                modified: None,
                accessed: None,
                first_cluster: self.boot.first_root_cluster,
                no_fat_chain: false,
            });
        }

        let mut current_cluster = self.boot.first_root_cluster;
        let mut current_size = 0u64;
        let mut current_no_fat_chain = false;
        let mut last_entry: Option<ExfatEntry> = None;

        for &segment in &segments {
            let entries = self.list_dir_at(current_cluster, current_size, current_no_fat_chain)?;
            let found = entries
                .into_iter()
                .find(|e| e.name == segment || e.name.eq_ignore_ascii_case(segment));
            match found {
                Some(entry) => {
                    current_cluster = entry.first_cluster;
                    current_size = entry.size;
                    current_no_fat_chain = entry.no_fat_chain;
                    last_entry = Some(entry);
                }
                None => {
                    return Err(ExfatError::PathNotFound {
                        path: path.to_string(),
                    })
                }
            }
        }

        last_entry.ok_or_else(|| ExfatError::PathNotFound {
            path: path.to_string(),
        })
    }

    /// Lists the entries in `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ExfatError::PathNotFound`] if `path` does not exist, or
    /// [`ExfatError::MalformedEntrySet`] if the directory data is corrupt.
    pub fn list_dir(&self, path: &str) -> Result<Vec<ExfatEntry>, ExfatError> {
        let entry = self.resolve_path(path)?;
        if !entry.is_dir {
            return Err(ExfatError::PathNotFound {
                path: path.to_string(),
            });
        }
        self.list_dir_at(entry.first_cluster, entry.size, entry.no_fat_chain)
    }

    /// Returns the entry at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ExfatError::PathNotFound`] if `path` does not exist.
    pub fn stat(&self, path: &str) -> Result<ExfatEntry, ExfatError> {
        self.resolve_path(path)
    }

    /// Streams the contents of the file at `path` to `writer`.
    ///
    /// Returns the total number of bytes written on success. Data is read in
    /// chunks bounded by [`EXTRACTION_CHUNK_SIZE`] so peak RAM stays constant
    /// regardless of file size. For NoFatChain files, a single contiguous read
    /// window is streamed. For FAT-chained files, the FAT table is loaded once
    /// (volume-metadata sized, not file-data sized) then clusters are streamed
    /// one-by-one.
    ///
    /// # Errors
    ///
    /// Returns [`ExfatError::PathNotFound`] if `path` does not exist,
    /// [`ExfatError::AttemptedDirectoryExtraction`] if `path` is a directory,
    /// [`ExfatError::WriteError`] if `writer` returns an error, or
    /// [`ExfatError::ReadError`] on I/O failure.
    pub fn read_file_to_writer<W: std::io::Write>(
        &self,
        path: &str,
        writer: &mut W,
    ) -> Result<u64, ExfatError> {
        let entry = self.resolve_path(path)?;
        if entry.is_dir {
            return Err(ExfatError::AttemptedDirectoryExtraction {
                path: path.to_string(),
            });
        }
        if entry.no_fat_chain || entry.size == 0 {
            stream_contiguous_clusters(
                &self.reader,
                self.cluster_heap_offset(),
                self.cluster_size(),
                entry.first_cluster,
                entry.size,
                writer,
            )
        } else {
            stream_fat_chained_clusters(
                &self.reader,
                self.fat_offset(),
                self.cluster_heap_offset(),
                self.cluster_size(),
                entry.first_cluster,
                self.cluster_count(),
                entry.size,
                writer,
            )
        }
    }

    /// Reads the complete contents of the file at `path` into a `Vec<u8>`.
    ///
    /// Delegates to [`Self::read_file_to_writer`]; allocates only after all bytes
    /// have been streamed. Prefer `read_file_to_writer` for large files.
    ///
    /// # Errors
    ///
    /// Returns [`ExfatError::PathNotFound`] if `path` does not exist,
    /// [`ExfatError::AttemptedDirectoryExtraction`] if `path` is a directory,
    /// or [`ExfatError::ReadError`] on I/O failure.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, ExfatError> {
        let mut out = Vec::new();
        self.read_file_to_writer(path, &mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cryptovol_core::CryptovolError;

    /// Minimal in-memory [`BlockReader`] for tests.
    struct MemReader(Vec<u8>);

    impl BlockReader for MemReader {
        fn len(&self) -> u64 {
            self.0.len() as u64
        }

        fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
            let start = offset as usize;
            let end = start.saturating_add(buf.len());
            if end > self.0.len() {
                return Err(CryptovolError::OutOfBounds {
                    offset,
                    length: buf.len(),
                    file_len: self.0.len() as u64,
                });
            }
            buf.copy_from_slice(&self.0[start..end]);
            Ok(())
        }
    }

    /// Builds a 512-byte boot sector with all required exFAT fields set to
    /// valid values. Individual tests mutate specific bytes to introduce faults.
    fn make_valid_boot_sector() -> Vec<u8> {
        let mut buf = vec![0u8; 512];
        // Jump boot code
        buf[0] = 0xEB;
        buf[1] = 0x76;
        buf[2] = 0x90;
        // OEM name: "EXFAT   " (8 bytes)
        buf[3..11].copy_from_slice(b"EXFAT   ");
        // VolumeLength = 1024 sectors (u64 LE)
        buf[72..80].copy_from_slice(&1024u64.to_le_bytes());
        // FatOffset = 1 sector (u32 LE)
        buf[80..84].copy_from_slice(&1u32.to_le_bytes());
        // FatLength = 1 sector (u32 LE)
        buf[84..88].copy_from_slice(&1u32.to_le_bytes());
        // ClusterHeapOffset = 2 sectors (u32 LE)
        buf[88..92].copy_from_slice(&2u32.to_le_bytes());
        // ClusterCount = 10 (u32 LE); 10 clusters × 8 sectors = 80 sectors, fits in 1024
        buf[92..96].copy_from_slice(&10u32.to_le_bytes());
        // FirstClusterOfRootDirectory = 2 (u32 LE)
        buf[96..100].copy_from_slice(&2u32.to_le_bytes());
        // VolumeSerialNumber (u32 LE)
        buf[100..104].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        // FileSystemRevision = 1.00 (u16 LE)
        buf[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        // BytesPerSectorShift = 9 (512 bytes/sector)
        buf[108] = 9;
        // SectorsPerClusterShift = 3 (8 sectors/cluster = 4096 bytes)
        buf[109] = 3;
        // NumberOfFats = 1
        buf[110] = 1;
        // DriveSelect = 0x80
        buf[111] = 0x80;
        // Boot signature
        buf[510] = 0x55;
        buf[511] = 0xAA;
        buf
    }

    #[test]
    fn boot_sector_valid_parses_oem_name() {
        let bytes = make_valid_boot_sector();
        let result = ExfatFileSystem::open(MemReader(bytes));
        assert!(
            result.is_ok(),
            "valid boot sector should open successfully, got: {result:?}"
        );
    }

    #[test]
    fn boot_sector_wrong_oem_name_is_rejected() {
        let mut bytes = make_valid_boot_sector();
        bytes[3..11].copy_from_slice(b"NTFS    ");
        let result = ExfatFileSystem::open(MemReader(bytes));
        assert!(
            matches!(result, Err(ExfatError::InvalidBootSector { .. })),
            "wrong OEM name should return InvalidBootSector, got: {result:?}"
        );
    }

    #[test]
    fn boot_sector_zero_bytes_is_rejected() {
        let bytes = vec![0u8; 512];
        let result = ExfatFileSystem::open(MemReader(bytes));
        assert!(
            matches!(result, Err(ExfatError::InvalidBootSector { .. })),
            "zero bytes should return InvalidBootSector, got: {result:?}"
        );
    }

    #[test]
    fn boot_sector_bad_bytes_per_sector_shift_rejected() {
        let mut bytes = make_valid_boot_sector();
        bytes[108] = 0; // invalid: must be 9–12
        let result = ExfatFileSystem::open(MemReader(bytes));
        assert!(
            matches!(result, Err(ExfatError::InvalidBootSector { .. })),
            "BytesPerSectorShift=0 should return InvalidBootSector, got: {result:?}"
        );
    }

    #[test]
    fn boot_sector_bad_sectors_per_cluster_shift_rejected() {
        let mut bytes = make_valid_boot_sector();
        bytes[109] = 26; // invalid: 9+26=35 > 25
        let result = ExfatFileSystem::open(MemReader(bytes));
        assert!(
            matches!(result, Err(ExfatError::InvalidBootSector { .. })),
            "SectorsPerClusterShift=26 should return InvalidBootSector, got: {result:?}"
        );
    }

    // ── cluster math ────────────────────────────────────────────────────────

    #[test]
    fn cluster_to_byte_offset_correct() {
        let heap: u64 = 0x10000;
        let cs: u64 = 4096;
        let off2 = cluster_to_byte_offset(heap, cs, 2).unwrap();
        assert_eq!(off2, 0x10000, "cluster 2 offset should equal heap start");
        let off5 = cluster_to_byte_offset(heap, cs, 5).unwrap();
        assert_eq!(off5, 0x13000, "cluster 5 offset should be heap + 3*4096");
    }

    #[test]
    fn cluster_to_byte_offset_cluster_0_is_invalid() {
        let result = cluster_to_byte_offset(0, 512, 0);
        assert!(
            matches!(result, Err(ExfatError::InvalidClusterNumber { cluster: 0 })),
            "cluster 0 should be rejected, got: {result:?}"
        );
    }

    #[test]
    fn cluster_to_byte_offset_cluster_1_is_invalid() {
        let result = cluster_to_byte_offset(0, 512, 1);
        assert!(
            matches!(result, Err(ExfatError::InvalidClusterNumber { cluster: 1 })),
            "cluster 1 should be rejected, got: {result:?}"
        );
    }

    #[test]
    fn cluster_to_byte_offset_overflow_is_rejected() {
        // heap_offset + (3-2)*4096 would overflow u64::MAX
        let result = cluster_to_byte_offset(u64::MAX - 3, 4096, 3);
        assert!(
            matches!(result, Err(ExfatError::OutOfBoundsRead)),
            "overflow should return OutOfBoundsRead, got: {result:?}"
        );
    }

    // ── FAT chain traversal ─────────────────────────────────────────────────

    #[allow(clippy::items_after_statements)]
    #[test]
    fn fat_chain_traversal_normal() {
        // FAT: [reserved, reserved, →3, →4, EOF]
        let fat: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, // entry 0: reserved
            0x00, 0x00, 0x00, 0x00, // entry 1: reserved
            0x03, 0x00, 0x00, 0x00, // entry 2: next=3
            0x04, 0x00, 0x00, 0x00, // entry 3: next=4
            0xFF, 0xFF, 0xFF, 0xFF, // entry 4: EOF
        ];
        let chain = collect_fat_chain(fat, 2, 16).unwrap();
        assert_eq!(chain, vec![2u32, 3, 4]);
    }

    #[test]
    fn fat_chain_eof_single_cluster() {
        // FAT: [reserved, reserved, EOF]
        let fat: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
            0xFF, // entry 2: EOF immediately
        ];
        let chain = collect_fat_chain(fat, 2, 16).unwrap();
        assert_eq!(chain, vec![2u32]);
    }

    #[test]
    fn fat_chain_cycle_detected() {
        // FAT: entry 2→3, entry 3→2 (cycle)
        let fat: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
            0x00, // entry 2: next=3
            0x02, 0x00, 0x00, 0x00, // entry 3: next=2 (cycle)
        ];
        let result = collect_fat_chain(fat, 2, 16);
        assert!(
            matches!(result, Err(ExfatError::FatChainCycle)),
            "cycle should return FatChainCycle, got: {result:?}"
        );
    }

    #[test]
    fn fat_chain_out_of_range_entry() {
        // FAT: entry 2 points to cluster 999, but max_clusters=4
        let fat: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE7, 0x03, 0x00,
            0x00, // entry 2: next=999 (0x3E7 LE)
        ];
        let result = collect_fat_chain(fat, 2, 4);
        assert!(
            matches!(result, Err(ExfatError::InvalidClusterChain)),
            "out-of-range cluster should return InvalidClusterChain, got: {result:?}"
        );
    }

    // ── contiguous read ─────────────────────────────────────────────────────

    #[test]
    fn contiguous_read_correct_length() {
        // Fill 512 bytes with a known pattern; cluster 2 at heap offset 0 is bytes [0..512].
        let content: Vec<u8> = (0u8..=255).cycle().take(512).collect();
        let result = read_contiguous_clusters(&MemReader(content.clone()), 0, 512, 2, 100);
        let bytes = result.unwrap();
        assert_eq!(
            bytes.len(),
            100,
            "read should return exactly data_length bytes"
        );
        assert_eq!(
            bytes,
            &content[0..100],
            "bytes should match cluster content"
        );
    }

    // ── directory entry helpers ─────────────────────────────────────────────

    /// Encode a Rust str slice as UTF-16LE code units.
    fn str_to_utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// Build a minimal valid exFAT directory entry set in raw bytes.
    ///
    /// Layout: File Directory Entry (0x85) + Stream Extension (0xC0) +
    /// one or more File Name entries (0xC1).
    fn make_entry_set(name_utf16: &[u16], is_dir: bool, size: u64, first_cluster: u32) -> Vec<u8> {
        let name_entry_count = name_utf16.len().div_ceil(15);
        let secondary_count = (1 + name_entry_count) as u8; // 1 stream + N name entries

        let mut out = Vec::new();

        // ── File Directory Entry (32 bytes) ──
        let mut fde = [0u8; 32];
        fde[0] = 0x85;
        fde[1] = secondary_count;
        // attributes: bit4=directory, bit5=archive
        let attrs: u16 = if is_dir { 0x0010 } else { 0x0020 };
        fde[4..6].copy_from_slice(&attrs.to_le_bytes());
        out.extend_from_slice(&fde);

        // ── Stream Extension Entry (32 bytes) ──
        let mut se = [0u8; 32];
        se[0] = 0xC0;
        se[1] = 0x01; // AllocationPossible
        se[3] = name_utf16.len() as u8;
        se[8..16].copy_from_slice(&size.to_le_bytes());
        se[20..24].copy_from_slice(&first_cluster.to_le_bytes());
        se[24..32].copy_from_slice(&size.to_le_bytes()); // data_length = size
        out.extend_from_slice(&se);

        // ── File Name Entries (32 bytes each) ──
        for chunk in name_utf16.chunks(15) {
            let mut ne = [0u8; 32];
            ne[0] = 0xC1;
            ne[1] = 0x01;
            for (i, &unit) in chunk.iter().enumerate() {
                ne[2 + i * 2..4 + i * 2].copy_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&ne);
        }

        // Compute and set SetChecksum at primary_entry[2..4].
        let entry_count = 1 + secondary_count as usize;
        let checksum = entry_set_checksum(&out, entry_count);
        out[2..4].copy_from_slice(&checksum.to_le_bytes());

        out
    }

    // ── checksum tests ──────────────────────────────────────────────────────

    #[test]
    fn entry_set_with_valid_checksum_parses() {
        let name_u16 = str_to_utf16("valid.txt");
        let mut data = make_entry_set(&name_u16, false, 42, 2);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);
        // make_entry_set now embeds correct SetChecksum; parsing must succeed.
        let entries = parse_directory_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "valid.txt");
    }

    #[test]
    fn entry_set_with_wrong_checksum_is_rejected() {
        let name_u16 = str_to_utf16("bad.txt");
        let mut data = make_entry_set(&name_u16, false, 10, 2);
        // Corrupt the SetChecksum field (bytes 2–3 of primary entry).
        data[2] = data[2].wrapping_add(1);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);
        let err = parse_directory_entries(&data).unwrap_err();
        assert!(
            matches!(err, ExfatError::MalformedEntrySet),
            "expected MalformedEntrySet, got {err:?}"
        );
    }

    // ── directory parsing tests ─────────────────────────────────────────────

    #[test]
    fn parse_entry_set_file_with_ascii_name() {
        let name_u16 = str_to_utf16("hello.txt");
        let mut data = make_entry_set(&name_u16, false, 100, 2);
        data.push(0x00); // end-of-directory
        data.resize(data.len().next_multiple_of(32), 0);

        let entries = parse_directory_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello.txt");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].size, 100);
    }

    #[test]
    fn parse_entry_set_directory() {
        let name_u16 = str_to_utf16("Documents");
        let mut data = make_entry_set(&name_u16, true, 0, 3);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);

        let entries = parse_directory_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir, "entry should be a directory");
    }

    #[test]
    fn parse_entry_set_emoji_filename() {
        // 🚀 = U+1F680 → surrogate pair [0xD83D, 0xDE80]
        let name_u16: Vec<u16> = vec![0xD83D, 0xDE80, 0x2E, 0x74, 0x78, 0x74]; // "🚀.txt"
        let mut data = make_entry_set(&name_u16, false, 0, 2);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);

        let entries = parse_directory_entries(&data).unwrap();
        assert_eq!(entries[0].name, "🚀.txt");
    }

    #[test]
    fn parse_entry_set_decomposed_umlaut() {
        // decomposed: 'a' U+0061 + combining diaeresis U+0308 — must NOT be normalised to 'ä'
        let name_u16: Vec<u16> = vec![0x0061, 0x0308];
        let mut data = make_entry_set(&name_u16, false, 0, 2);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);

        let entries = parse_directory_entries(&data).unwrap();
        let chars: Vec<char> = entries[0].name.chars().collect();
        assert_eq!(
            chars.len(),
            2,
            "decomposed form must be preserved as 2 code points"
        );
        assert_eq!(chars[0], 'a');
        assert_eq!(chars[1], '\u{0308}');
    }

    #[test]
    fn parse_empty_directory_returns_empty() {
        let data = [0u8; 32]; // one end-of-directory entry
        let entries = parse_directory_entries(&data).unwrap();
        assert!(entries.is_empty(), "empty directory should yield empty vec");
    }

    #[test]
    fn parse_entry_missing_stream_extension() {
        // File entry (0x85) followed by another File entry (0x85) instead of 0xC0
        let mut data = [0u8; 64];
        data[0] = 0x85; // primary file entry
        data[1] = 2; // secondary_count=2
        data[32] = 0x85; // WRONG: should be 0xC0
        data[33] = 0;

        let result = parse_directory_entries(&data);
        assert!(
            matches!(result, Err(ExfatError::MissingStreamExtension)),
            "wrong secondary type should return MissingStreamExtension, got: {result:?}"
        );
    }

    #[test]
    fn parse_entry_deleted_entry_is_skipped() {
        // Deleted set: type 0x05 (bit7=0) + deleted stream (0x40) + deleted name (0x41)
        let mut data = Vec::new();
        let mut deleted_fde = [0u8; 32];
        deleted_fde[0] = 0x05; // inactive file directory entry
        deleted_fde[1] = 2; // secondary_count=2
        data.extend_from_slice(&deleted_fde);
        data.extend_from_slice(&[0x40u8; 32]); // deleted stream extension
        data.extend_from_slice(&[0x41u8; 32]); // deleted name entry

        // Valid entry that should be visible
        let name_u16 = str_to_utf16("visible.txt");
        data.extend_from_slice(&make_entry_set(&name_u16, false, 0, 2));
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);

        let entries = parse_directory_entries(&data).unwrap();
        assert_eq!(entries.len(), 1, "deleted entry must be skipped");
        assert_eq!(entries[0].name, "visible.txt");
    }

    #[test]
    fn parse_entry_multiple_name_entries() {
        // 20 UTF-16LE units require 2 File Name entries (15 + 5)
        let name: String = "abcdefghijklmnopqrst".to_owned(); // exactly 20 ASCII chars
        let name_u16 = str_to_utf16(&name);
        assert_eq!(name_u16.len(), 20);
        let mut data = make_entry_set(&name_u16, false, 0, 2);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);

        let entries = parse_directory_entries(&data).unwrap();
        assert_eq!(
            entries[0].name, name,
            "20-char name must round-trip correctly"
        );
    }

    #[test]
    fn decode_utf16_invalid_lone_surrogate_returns_error() {
        // Lone high surrogate 0xD83D not followed by a low surrogate
        let name_u16: Vec<u16> = vec![0xD83D, 0x0061]; // lone high surrogate + 'a'
        let mut data = make_entry_set(&name_u16, false, 0, 2);
        data.push(0x00);
        data.resize(data.len().next_multiple_of(32), 0);

        let result = parse_directory_entries(&data);
        assert!(
            matches!(result, Err(ExfatError::InvalidUtf16)),
            "lone surrogate should return InvalidUtf16, got: {result:?}"
        );
    }

    // ── ExfatFileSystem volume helpers ───────────────────────────────────────

    /// Same as `make_entry_set` but with the NoFatChain flag set in the stream extension.
    fn make_entry_set_contiguous(
        name_utf16: &[u16],
        is_dir: bool,
        size: u64,
        first_cluster: u32,
    ) -> Vec<u8> {
        let mut bytes = make_entry_set(name_utf16, is_dir, size, first_cluster);
        // Stream Extension starts at byte 32; general_secondary_flags is at byte 33.
        // AllocationPossible (bit 0) | NoFatChain (bit 1).
        bytes[33] = 0x03;
        // Recompute SetChecksum after modifying the stream extension flags.
        let entry_count = 1 + bytes[1] as usize;
        let checksum = entry_set_checksum(&bytes, entry_count);
        bytes[2..4].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    /// Builds a minimal valid in-memory exFAT volume for integration tests.
    ///
    /// Layout (sector size = 512 bytes):
    /// ```text
    /// Sector 0  (byte    0): boot sector
    /// Sector 1-2(byte  512): FAT (entries 0-6 set; rest zero)
    /// Sector 3  (byte 1536): cluster 2 = root directory
    /// Sector 4  (byte 2048): cluster 3 = hello.txt  b"hello"
    /// Sector 5  (byte 2560): cluster 4 = unused
    /// Sector 6  (byte 3072): cluster 5 = subdir directory
    /// Sector 7  (byte 3584): cluster 6 = nested.txt  b"nested"
    /// ```
    ///
    /// Root dir entries: hello.txt (size=5, cluster=3) + subdir/ (size=0, cluster=5).
    /// Subdir entries  : nested.txt (size=6, cluster=6).
    /// All files use contiguous allocation (NoFatChain=true).
    fn build_synthetic_volume() -> Vec<u8> {
        let sector: usize = 512;
        let mut vol = vec![0u8; 64 * sector]; // 32 768 bytes

        // ── Boot sector ───────────────────────────────────────────────────
        vol[3..11].copy_from_slice(b"EXFAT   ");
        vol[72..80].copy_from_slice(&64u64.to_le_bytes()); // VolumeLength (sectors)
        vol[80..84].copy_from_slice(&1u32.to_le_bytes()); // FatOffset
        vol[84..88].copy_from_slice(&2u32.to_le_bytes()); // FatLength
        vol[88..92].copy_from_slice(&3u32.to_le_bytes()); // ClusterHeapOffset
        vol[92..96].copy_from_slice(&61u32.to_le_bytes()); // ClusterCount
        vol[96..100].copy_from_slice(&2u32.to_le_bytes()); // FirstClusterOfRootDirectory
        vol[108] = 9; // BytesPerSectorShift (512 B)
        vol[109] = 0; // SectorsPerClusterShift (1 sector/cluster)
        vol[110] = 1; // NumberOfFats

        // ── FAT (byte 512) — entries indexed 0..7 ────────────────────────
        let fb = sector;
        vol[fb..fb + 4].copy_from_slice(&0xFFFFFFF8u32.to_le_bytes()); // 0: media
        vol[fb + 4..fb + 8].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // 1: reserved
        vol[fb + 8..fb + 12].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // 2: root EOF
        vol[fb + 12..fb + 16].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // 3: hello EOF
                                                                             // 4: unused (zero)
        vol[fb + 20..fb + 24].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // 5: subdir EOF
        vol[fb + 24..fb + 28].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // 6: nested EOF

        // ── Cluster heap: cluster N at byte (1536 + (N-2)*512) ───────────
        let hb = 3 * sector; // 1536

        // Cluster 2 = root directory
        let hello_u16: Vec<u16> = "hello.txt".encode_utf16().collect();
        let subdir_u16: Vec<u16> = "subdir".encode_utf16().collect();
        let mut root_dir: Vec<u8> = Vec::new();
        root_dir.extend_from_slice(&make_entry_set_contiguous(&hello_u16, false, 5, 3));
        root_dir.extend_from_slice(&make_entry_set_contiguous(&subdir_u16, true, 0, 5));
        root_dir.push(0x00); // end-of-directory
        root_dir.resize(sector, 0);
        vol[hb..hb + sector].copy_from_slice(&root_dir);

        // Cluster 3 = hello.txt data
        vol[hb + sector..hb + sector + 5].copy_from_slice(b"hello");

        // Cluster 5 = subdir directory (byte 3072)
        let nested_u16: Vec<u16> = "nested.txt".encode_utf16().collect();
        let mut sub_dir: Vec<u8> = Vec::new();
        sub_dir.extend_from_slice(&make_entry_set_contiguous(&nested_u16, false, 6, 6));
        sub_dir.push(0x00); // end-of-directory
        sub_dir.resize(sector, 0);
        vol[hb + 3 * sector..hb + 4 * sector].copy_from_slice(&sub_dir);

        // Cluster 6 = nested.txt data (byte 3584)
        vol[hb + 4 * sector..hb + 4 * sector + 6].copy_from_slice(b"nested");

        vol
    }

    // ── ExfatFileSystem::list_dir / stat / read_file tests ──────────────────

    #[test]
    fn list_dir_root_returns_two_entries() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let entries = fs.list_dir("/").unwrap();
        assert_eq!(entries.len(), 2);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"hello.txt"), "expected hello.txt in root");
        assert!(names.contains(&"subdir"), "expected subdir in root");
    }

    #[test]
    fn stat_file_by_exact_name() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let entry = fs.stat("/hello.txt").unwrap();
        assert_eq!(entry.name, "hello.txt");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 5);
    }

    #[test]
    fn stat_dir_by_exact_name() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let entry = fs.stat("/subdir").unwrap();
        assert!(entry.is_dir, "subdir should be a directory");
    }

    #[test]
    fn list_dir_nested() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let entries = fs.list_dir("/subdir").unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"nested.txt"),
            "expected nested.txt in /subdir"
        );
    }

    #[test]
    fn stat_nested_path() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let entry = fs.stat("/subdir/nested.txt").unwrap();
        assert_eq!(entry.name, "nested.txt");
        assert_eq!(entry.size, 6);
    }

    #[test]
    fn stat_missing_path_returns_not_found() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let result = fs.stat("/does-not-exist.txt");
        assert!(
            matches!(result, Err(ExfatError::PathNotFound { .. })),
            "missing path should return PathNotFound, got: {result:?}"
        );
    }

    #[test]
    fn read_file_returns_correct_bytes() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let bytes = fs.read_file("/hello.txt").unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn read_file_rejects_directory() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let result = fs.read_file("/subdir");
        assert!(
            matches!(result, Err(ExfatError::AttemptedDirectoryExtraction { .. })),
            "reading a directory should return AttemptedDirectoryExtraction, got: {result:?}"
        );
    }

    #[test]
    fn read_file_nested() {
        let fs = ExfatFileSystem::open(MemReader(build_synthetic_volume())).unwrap();
        let bytes = fs.read_file("/subdir/nested.txt").unwrap();
        assert_eq!(bytes, b"nested");
    }

    // --- Helpers and tests for read_file_to_writer streaming (T-004 red) ---

    /// Accumulates bytes and counts the number of `write` calls.
    struct CountingWriter {
        calls: usize,
        data: Vec<u8>,
    }

    impl CountingWriter {
        fn new() -> Self {
            Self {
                calls: 0,
                data: Vec::new(),
            }
        }
    }

    impl std::io::Write for CountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Builds an exFAT volume for streaming tests.
    ///
    /// Layout (sector_size = 512 B, cluster_size = 512 B):
    /// ```text
    /// Sector 0: boot sector
    /// Sector 1: FAT
    /// Sector 2: cluster 2 = root directory
    /// Sector 3: cluster 3 = ONEBYTE.TXT  (1 B, contiguous)
    /// Sector 4: cluster 4 = FULL.TXT     (512 B, contiguous)
    /// Sector 5: cluster 5 = CHAINED.TXT  part 1 (FAT → cluster 6)
    /// Sector 6: cluster 6 = CHAINED.TXT  part 2 (513 B total, EOF)
    /// Sector 7: cluster 7 = subdir/ directory
    /// Sector 8: cluster 8 = nested.txt   (4 B, contiguous)
    /// ```
    ///
    /// Root: ONEBYTE.TXT, EMPTY.TXT (0 B), FULL.TXT, CHAINED.TXT, subdir/.
    fn build_streaming_vol() -> Vec<u8> {
        let s: usize = 512;
        let total = 9 * s;
        let mut vol = vec![0u8; total];

        // Boot sector
        vol[3..11].copy_from_slice(b"EXFAT   ");
        vol[72..80].copy_from_slice(&9u64.to_le_bytes()); // VolumeLength
        vol[80..84].copy_from_slice(&1u32.to_le_bytes()); // FatOffset
        vol[84..88].copy_from_slice(&1u32.to_le_bytes()); // FatLength
        vol[88..92].copy_from_slice(&2u32.to_le_bytes()); // ClusterHeapOffset
        vol[92..96].copy_from_slice(&7u32.to_le_bytes()); // ClusterCount (2..=8)
        vol[96..100].copy_from_slice(&2u32.to_le_bytes()); // FirstClusterOfRootDirectory
        vol[108] = 9; // BytesPerSectorShift
        vol[109] = 0; // SectorsPerClusterShift
        vol[110] = 1; // NumberOfFats

        // FAT at sector 1
        let fb = s;
        let fat: [(usize, u32); 9] = [
            (0, 0xFFFFFFF8), // media
            (1, 0xFFFFFFFF), // reserved
            (2, 0xFFFFFFFF), // root dir EOF
            (3, 0xFFFFFFFF), // ONEBYTE.TXT EOF
            (4, 0xFFFFFFFF), // FULL.TXT EOF
            (5, 6),          // CHAINED.TXT → cluster 6
            (6, 0xFFFFFFFF), // CHAINED.TXT EOF
            (7, 0xFFFFFFFF), // subdir EOF
            (8, 0xFFFFFFFF), // nested.txt EOF
        ];
        for (i, v) in fat {
            vol[fb + i * 4..fb + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }

        // Cluster heap: cluster N at byte 1024 + (N-2)*512
        let hb: usize = 2 * s;

        // Cluster 2 = root directory
        let onebyte_u16: Vec<u16> = "ONEBYTE.TXT".encode_utf16().collect();
        let empty_u16: Vec<u16> = "EMPTY.TXT".encode_utf16().collect();
        let full_u16: Vec<u16> = "FULL.TXT".encode_utf16().collect();
        let chained_u16: Vec<u16> = "CHAINED.TXT".encode_utf16().collect();
        let subdir_u16: Vec<u16> = "subdir".encode_utf16().collect();
        let mut root: Vec<u8> = Vec::new();
        root.extend_from_slice(&make_entry_set_contiguous(&onebyte_u16, false, 1, 3));
        // EMPTY.TXT: size=0; use cluster=2 as placeholder (no data read for 0-byte file)
        root.extend_from_slice(&make_entry_set_contiguous(&empty_u16, false, 0, 2));
        root.extend_from_slice(&make_entry_set_contiguous(&full_u16, false, 512, 4));
        root.extend_from_slice(&make_entry_set(&chained_u16, false, 513, 5)); // FAT-chained
        root.extend_from_slice(&make_entry_set_contiguous(&subdir_u16, true, 0, 7));
        root.push(0x00); // end-of-directory
        root.resize(s, 0);
        vol[hb..hb + s].copy_from_slice(&root);

        // Cluster 3: ONEBYTE.TXT (1 byte = 0xAA; rest is padding)
        vol[hb + s] = 0xAA;

        // Cluster 4: FULL.TXT (512 × 0xBB)
        vol[hb + 2 * s..hb + 3 * s].fill(0xBB);

        // Cluster 5: CHAINED.TXT part 1 (512 × 0xCC)
        vol[hb + 3 * s..hb + 4 * s].fill(0xCC);

        // Cluster 6: CHAINED.TXT part 2 (1 byte = 0xDD; rest padding)
        vol[hb + 4 * s] = 0xDD;

        // Cluster 7: subdir directory
        let nested_u16: Vec<u16> = "nested.txt".encode_utf16().collect();
        let mut sub: Vec<u8> = Vec::new();
        sub.extend_from_slice(&make_entry_set_contiguous(&nested_u16, false, 4, 8));
        sub.push(0x00);
        sub.resize(s, 0);
        vol[hb + 5 * s..hb + 6 * s].copy_from_slice(&sub);

        // Cluster 8: nested.txt (4 × 0xEE)
        vol[hb + 6 * s..hb + 6 * s + 4].fill(0xEE);

        vol
    }

    #[test]
    fn stream_empty_file() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/EMPTY.TXT", &mut out)
            .expect("EMPTY.TXT streaming must succeed");
        assert_eq!(n, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn stream_one_byte_contiguous() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/ONEBYTE.TXT", &mut out)
            .expect("ONEBYTE.TXT streaming must succeed");
        assert_eq!(n, 1);
        assert_eq!(out, vec![0xAAu8]);
    }

    #[test]
    fn stream_exact_cluster_contiguous() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/FULL.TXT", &mut out)
            .expect("FULL.TXT streaming must succeed");
        assert_eq!(n, 512);
        assert_eq!(out, vec![0xBBu8; 512]);
    }

    #[test]
    fn stream_cluster_plus_one_fat_chained() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/CHAINED.TXT", &mut out)
            .expect("CHAINED.TXT streaming must succeed");
        assert_eq!(n, 513);
        let mut expected = vec![0xCCu8; 512];
        expected.push(0xDD);
        assert_eq!(out, expected);
    }

    #[test]
    fn stream_nested_file() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/subdir/nested.txt", &mut out)
            .expect("/subdir/nested.txt streaming must succeed");
        assert_eq!(n, 4);
        assert_eq!(out, vec![0xEEu8; 4]);
    }

    #[test]
    fn stream_directory_returns_error() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/subdir", &mut out)
            .expect_err("/subdir must fail: source is a directory");
        assert!(
            matches!(err, ExfatError::AttemptedDirectoryExtraction { .. }),
            "expected AttemptedDirectoryExtraction, got {err}"
        );
    }

    #[test]
    fn stream_not_found_returns_error() {
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/MISSING.TXT", &mut out)
            .expect_err("/MISSING.TXT must fail");
        assert!(
            matches!(err, ExfatError::PathNotFound { .. }),
            "expected PathNotFound, got {err}"
        );
    }

    #[test]
    fn stream_chained_writer_called_multiple_times() {
        // CHAINED.TXT spans 2 clusters; the streaming implementation must
        // produce at least 2 write calls for a 2-cluster FAT-chained file.
        let fs = ExfatFileSystem::open(MemReader(build_streaming_vol())).unwrap();
        let mut writer = CountingWriter::new();
        let n = fs
            .read_file_to_writer("/CHAINED.TXT", &mut writer)
            .expect("CHAINED.TXT streaming must succeed");
        assert_eq!(n, 513);
        assert_eq!(writer.data.len(), 513);
        assert!(
            writer.calls >= 2,
            "2-cluster FAT-chained file must produce >= 2 write calls, got {}",
            writer.calls
        );
    }
}
