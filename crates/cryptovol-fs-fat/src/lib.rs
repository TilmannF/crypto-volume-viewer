//! Read-only FAT directory and file access for decrypted block devices.
//!
//! The current reader supports the narrow FAT profiles needed by the committed
//! TC/VC test fixture. It parses both 8.3 and long filename (LFN) directory
//! entries -- including Unicode/UTF-16LE names -- lists directories, and
//! reads single files, without write operations.

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
use std::collections::HashSet;
use std::fmt;
use thiserror::Error;

/// Errors returned by the FAT reader.
#[derive(Debug, Error)]
pub enum FatError {
    /// The boot sector is malformed or outside current support.
    #[error("invalid or unsupported boot sector")]
    InvalidBootSector,
    /// The detected FAT type is not supported.
    #[error("unsupported FAT type")]
    UnsupportedFatType,
    /// A directory entry is malformed.
    #[error("malformed directory entry")]
    MalformedEntry,
    /// A path could not be found.
    #[error("path not found: {path}")]
    PathNotFound {
        /// Container path that was requested.
        path: String,
    },
    /// A non-directory entry was used as a directory.
    #[error("not a directory: {path}")]
    NotADirectory {
        /// Container path that was requested.
        path: String,
    },
    /// Underlying block read failed.
    #[error("read error: {0}")]
    ReadError(#[from] CryptovolError),
    /// A parsed offset or cluster chain points outside the volume.
    #[error("out of bounds or corrupt cluster chain")]
    OutOfBounds,
    /// A checked size conversion or allocation size overflowed.
    #[error("size conversion overflow")]
    SizeOverflow,
    /// A directory path was passed to file extraction.
    #[error("is a directory: {path}")]
    IsADirectory {
        /// Container path that was requested.
        path: String,
    },
    /// A cluster chain repeats a cluster.
    #[error("cluster chain cycle detected")]
    ClusterChainCycle,
    /// A file's cluster chain ends before its advertised size.
    #[error("cluster chain ended before advertised file size")]
    ClusterChainTooShort,
    /// A cluster number is outside the valid data cluster range.
    #[error("invalid cluster number: {cluster}")]
    InvalidCluster {
        /// Invalid cluster number.
        cluster: u32,
    },
    /// Destination writer failed during streaming extraction.
    #[error("write failed during extraction: {0}")]
    WriteError(std::io::Error),
}

/// FAT variant detected from the boot-sector layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    /// FAT12 layout.
    Fat12,
    /// FAT16 layout.
    Fat16,
    /// FAT32 layout.
    Fat32,
}

/// Safe filesystem metadata derived from the boot sector.
#[derive(Debug, Clone)]
pub struct FatMetadata {
    /// Detected FAT type.
    pub fat_type: FatType,
    /// Bytes per logical sector.
    pub bytes_per_sector: u16,
    /// Sectors per allocation cluster.
    pub sectors_per_cluster: u8,
    /// Bytes per allocation cluster.
    pub cluster_size: u64,
}

/// Parsed FAT directory entry attribute flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatAttributes {
    /// Read-only bit (attribute bit 0).
    pub read_only: bool,
    /// Hidden bit (attribute bit 1).
    pub hidden: bool,
    /// System bit (attribute bit 2).
    pub system: bool,
    /// Directory bit (attribute bit 4).
    pub directory: bool,
    /// Archive bit (attribute bit 5).
    pub archive: bool,
}

/// A FAT calendar date (timezone-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatDate {
    /// Gregorian year (1980–2107).
    pub year: u16,
    /// Month of year (1–12).
    pub month: u8,
    /// Day of month (1–31).
    pub day: u8,
}

/// A FAT time-of-day (timezone-free, two-second granularity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatTime {
    /// Hour (0–23).
    pub hour: u8,
    /// Minute (0–59).
    pub minute: u8,
    /// Second (0–58, even values only).
    pub second: u8,
}

/// A combined FAT date+time (timezone-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatTimestamp {
    /// Calendar date.
    pub date: FatDate,
    /// Time of day.
    pub time: FatTime,
}

/// Public directory entry returned by directory listings.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// Long name from LFN entries, or the 8.3 short name when no valid LFN is present.
    pub name: String,
    /// Uppercase 8.3 short name rendered with an extension when present.
    pub short_name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Advertised file size in bytes.
    pub size: u32,
    /// FAT attribute flags.
    pub attributes: FatAttributes,
    /// Creation timestamp, when present and valid.
    pub created: Option<FatTimestamp>,
    /// Last-modified timestamp, when present and valid.
    pub modified: Option<FatTimestamp>,
    /// Last-accessed date, when present and valid.
    pub accessed: Option<FatDate>,
}

/// Internal entry that also carries the start cluster needed for directory traversal.
struct InternalEntry {
    name: String,
    short_name: String,
    is_dir: bool,
    size: u32,
    start_cluster: u32,
    attributes: FatAttributes,
    created: Option<FatTimestamp>,
    modified: Option<FatTimestamp>,
    accessed: Option<FatDate>,
}

impl From<InternalEntry> for DirectoryEntry {
    fn from(e: InternalEntry) -> Self {
        DirectoryEntry {
            name: e.name,
            short_name: e.short_name,
            is_dir: e.is_dir,
            size: e.size,
            attributes: e.attributes,
            created: e.created,
            modified: e.modified,
            accessed: e.accessed,
        }
    }
}

/// Internal parsed layout, kept separate from the public FatMetadata.
struct Layout {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sector_count: u16,
    fat_count: u8,
    fat_size: u32,
    root_entry_count: u16,
    first_data_sector: u32,
    root_cluster: u32,
    fat_type: FatType,
    cluster_size: u64,
    max_valid_cluster: u32,
}

/// Read-only FAT filesystem reader over a decrypted block reader.
pub struct FatFileSystem<'a> {
    reader: &'a dyn BlockReader,
    layout: Layout,
    metadata: FatMetadata,
}

impl<'a> fmt::Debug for FatFileSystem<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FatFileSystem")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl<'a> FatFileSystem<'a> {
    /// Opens a FAT filesystem from the start of `reader`.
    ///
    /// # Errors
    ///
    /// Returns [`FatError`] when the boot sector is malformed, unsupported, or
    /// cannot be read.
    pub fn open(reader: &'a dyn BlockReader) -> Result<Self, FatError> {
        let mut sector = [0u8; 512];
        reader.read_at(0, &mut sector)?;

        if sector[510] != 0x55 || sector[511] != 0xAA {
            return Err(FatError::InvalidBootSector);
        }

        let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
        let sectors_per_cluster = sector[13];
        let reserved_sector_count = u16::from_le_bytes([sector[14], sector[15]]);
        let fat_count = sector[16];
        let root_entry_count = u16::from_le_bytes([sector[17], sector[18]]);
        let total_sectors_16 = u16::from_le_bytes([sector[19], sector[20]]);
        let sectors_per_fat_16 = u16::from_le_bytes([sector[22], sector[23]]);
        let total_sectors_32 = u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]]);
        let sectors_per_fat_32 =
            u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
        let root_cluster = u32::from_le_bytes([sector[44], sector[45], sector[46], sector[47]]);

        if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
            return Err(FatError::InvalidBootSector);
        }
        if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
            return Err(FatError::InvalidBootSector);
        }
        if fat_count == 0 {
            return Err(FatError::InvalidBootSector);
        }

        let bps = u64::from(bytes_per_sector);
        let fat_size = if sectors_per_fat_16 > 0 {
            sectors_per_fat_16 as u32
        } else {
            sectors_per_fat_32
        };
        let total_sectors = if total_sectors_16 > 0 {
            total_sectors_16 as u32
        } else {
            total_sectors_32
        };

        if fat_size == 0 || total_sectors == 0 {
            return Err(FatError::InvalidBootSector);
        }

        let root_dir_bytes = u64::from(root_entry_count)
            .checked_mul(32)
            .ok_or(FatError::InvalidBootSector)?;
        let root_dir_sectors = root_dir_bytes.div_ceil(bps);
        let fat_sectors = u64::from(fat_count)
            .checked_mul(u64::from(fat_size))
            .ok_or(FatError::InvalidBootSector)?;
        let first_data_sector = u64::from(reserved_sector_count)
            .checked_add(fat_sectors)
            .and_then(|value| value.checked_add(root_dir_sectors))
            .ok_or(FatError::InvalidBootSector)?;

        if first_data_sector >= u64::from(total_sectors) {
            return Err(FatError::InvalidBootSector);
        }

        let data_sectors = u64::from(total_sectors) - first_data_sector;
        let count_of_clusters = data_sectors / u64::from(sectors_per_cluster);
        if count_of_clusters == 0 {
            return Err(FatError::InvalidBootSector);
        }
        let max_valid_cluster = u32::try_from(
            count_of_clusters
                .checked_add(1)
                .ok_or(FatError::InvalidBootSector)?,
        )
        .map_err(|_| FatError::InvalidBootSector)?;

        let fat_type = if count_of_clusters < 4085 {
            FatType::Fat12
        } else if count_of_clusters < 65525 {
            FatType::Fat16
        } else {
            FatType::Fat32
        };

        if fat_type == FatType::Fat32 && (root_cluster < 2 || root_cluster > max_valid_cluster) {
            return Err(FatError::InvalidBootSector);
        }

        let cluster_size = u64::from(bytes_per_sector)
            .checked_mul(u64::from(sectors_per_cluster))
            .ok_or(FatError::InvalidBootSector)?;

        let layout = Layout {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sector_count,
            fat_count,
            fat_size,
            root_entry_count,
            first_data_sector: u32::try_from(first_data_sector)
                .map_err(|_| FatError::InvalidBootSector)?,
            root_cluster,
            fat_type,
            cluster_size,
            max_valid_cluster,
        };

        let metadata = FatMetadata {
            fat_type,
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size,
        };

        Ok(Self {
            reader,
            layout,
            metadata,
        })
    }

    /// Returns safe metadata parsed from the FAT boot sector.
    pub fn metadata(&self) -> &FatMetadata {
        &self.metadata
    }

    /// Lists a directory at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`FatError`] when `path` cannot be resolved as a directory or
    /// the underlying directory chain is malformed.
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirectoryEntry>, FatError> {
        let components = parse_path(path);

        if components.is_empty() {
            let entries = self.read_root_dir_entries()?;
            return Ok(entries.into_iter().map(DirectoryEntry::from).collect());
        }

        let mut current = self.read_root_dir_entries()?;

        for (i, component) in components.iter().enumerate() {
            let pos = current.iter().position(|e| entry_matches(e, component));

            match pos {
                None => {
                    return Err(FatError::PathNotFound {
                        path: path.to_string(),
                    })
                }
                Some(idx) if !current[idx].is_dir => {
                    return Err(FatError::NotADirectory {
                        path: path.to_string(),
                    })
                }
                Some(idx) => {
                    let cluster = current[idx].start_cluster;
                    if i == components.len() - 1 {
                        let entries = self.read_cluster_chain_entries(cluster)?;
                        return Ok(entries.into_iter().map(DirectoryEntry::from).collect());
                    }
                    current = self.read_cluster_chain_entries(cluster)?;
                }
            }
        }

        // Only reached if components was empty, but that's handled above.
        Err(FatError::PathNotFound {
            path: path.to_string(),
        })
    }

    fn read_root_dir_entries(&self) -> Result<Vec<InternalEntry>, FatError> {
        match self.layout.fat_type {
            FatType::Fat32 => self.read_cluster_chain_entries(self.layout.root_cluster),
            FatType::Fat12 | FatType::Fat16 => {
                let fat_bytes = u64::from(self.layout.fat_count)
                    .checked_mul(u64::from(self.layout.fat_size))
                    .ok_or(FatError::OutOfBounds)?;
                let root_offset = u64::from(self.layout.reserved_sector_count)
                    .checked_add(fat_bytes)
                    .and_then(|sector| sector.checked_mul(u64::from(self.layout.bytes_per_sector)))
                    .ok_or(FatError::OutOfBounds)?;
                let root_size = usize::from(self.layout.root_entry_count)
                    .checked_mul(32)
                    .ok_or(FatError::SizeOverflow)?;
                let mut data = vec![0u8; root_size];
                self.reader.read_at(root_offset, &mut data)?;
                Ok(parse_dir_entries_with_lfn(&data))
            }
        }
    }

    fn read_cluster_chain_entries(
        &self,
        start_cluster: u32,
    ) -> Result<Vec<InternalEntry>, FatError> {
        const MAX_CHAIN: u32 = 65536;
        let mut cluster = start_cluster;
        let mut visited: HashSet<u32> = HashSet::new();
        let mut chain_len: u32 = 0;
        let mut all_data: Vec<u8> = Vec::new();

        loop {
            if chain_len >= MAX_CHAIN {
                return Err(FatError::OutOfBounds);
            }
            chain_len += 1;
            if !visited.insert(cluster) {
                return Err(FatError::ClusterChainCycle);
            }

            let sector = self.cluster_to_sector(cluster)?;
            let offset = sector
                .checked_mul(self.layout.bytes_per_sector as u64)
                .ok_or(FatError::OutOfBounds)?;
            let size =
                usize::try_from(self.layout.cluster_size).map_err(|_| FatError::SizeOverflow)?;
            let mut data = vec![0u8; size];
            self.reader.read_at(offset, &mut data)?;

            // Check whether this cluster contains the end-of-directory marker.
            // Still accumulate the data so LFN sequences crossing cluster boundaries
            // are correctly handled by parse_dir_entries_with_lfn.
            let has_end = data.chunks_exact(32).any(|c| c[0] == 0x00);
            all_data.extend_from_slice(&data);

            if has_end {
                break;
            }

            let next = self.next_cluster(cluster)?;
            if self.is_eof_cluster(next) {
                break;
            }
            if next < 2 || next > self.layout.max_valid_cluster {
                return Err(FatError::InvalidCluster { cluster: next });
            }
            cluster = next;
        }

        Ok(parse_dir_entries_with_lfn(&all_data))
    }

    fn cluster_to_sector(&self, cluster: u32) -> Result<u64, FatError> {
        if cluster < 2 || cluster > self.layout.max_valid_cluster {
            return Err(FatError::InvalidCluster { cluster });
        }
        let data_offset = (cluster as u64 - 2)
            .checked_mul(self.layout.sectors_per_cluster as u64)
            .ok_or(FatError::OutOfBounds)?;
        (self.layout.first_data_sector as u64)
            .checked_add(data_offset)
            .ok_or(FatError::OutOfBounds)
    }

    fn next_cluster(&self, cluster: u32) -> Result<u32, FatError> {
        let fat_byte_start =
            self.layout.reserved_sector_count as u64 * self.layout.bytes_per_sector as u64;

        match self.layout.fat_type {
            FatType::Fat16 => {
                let entry_offset = fat_byte_start
                    .checked_add(cluster as u64 * 2)
                    .ok_or(FatError::OutOfBounds)?;
                let mut buf = [0u8; 2];
                self.reader.read_at(entry_offset, &mut buf)?;
                Ok(u16::from_le_bytes(buf) as u32)
            }
            FatType::Fat32 => {
                let entry_offset = fat_byte_start
                    .checked_add(cluster as u64 * 4)
                    .ok_or(FatError::OutOfBounds)?;
                let mut buf = [0u8; 4];
                self.reader.read_at(entry_offset, &mut buf)?;
                Ok(u32::from_le_bytes(buf) & 0x0FFF_FFFF)
            }
            FatType::Fat12 => {
                // Each entry is 12 bits; byte offset = (cluster * 3) / 2
                let byte_off = (cluster as u64)
                    .checked_mul(3)
                    .ok_or(FatError::OutOfBounds)?
                    / 2;
                let entry_offset = fat_byte_start
                    .checked_add(byte_off)
                    .ok_or(FatError::OutOfBounds)?;
                let mut buf = [0u8; 2];
                self.reader.read_at(entry_offset, &mut buf)?;
                let word = u16::from_le_bytes(buf);
                let val = if cluster & 1 == 0 {
                    word & 0x0FFF
                } else {
                    word >> 4
                };
                Ok(val as u32)
            }
        }
    }

    fn is_eof_cluster(&self, cluster: u32) -> bool {
        match self.layout.fat_type {
            FatType::Fat12 => cluster >= 0xFF8,
            FatType::Fat16 => cluster >= 0xFFF8,
            FatType::Fat32 => cluster >= 0x0FFF_FFF8,
        }
    }

    /// Streams the file at `path` to `writer` in bounded chunks.
    ///
    /// Returns the total number of bytes written, which equals the file's
    /// declared size on success.  No `Vec<u8>` proportional to the file size
    /// is allocated; each cluster is read into a buffer of at most
    /// [`EXTRACTION_CHUNK_SIZE`] bytes and written immediately to `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`FatError`] when the path does not resolve to a file, the
    /// cluster chain is malformed, an underlying read fails, or `writer`
    /// returns an error.
    pub fn read_file_to_writer<W: std::io::Write>(
        &self,
        path: &str,
        writer: &mut W,
    ) -> Result<u64, FatError> {
        let components = parse_path(path);
        if components.is_empty() {
            return Err(FatError::PathNotFound {
                path: path.to_string(),
            });
        }

        let (parents, tail) = components.split_at(components.len() - 1);
        let filename = &tail[0];

        // Navigate parent components to reach the containing directory.
        let dir_entries = if parents.is_empty() {
            self.read_root_dir_entries()?
        } else {
            let mut current = self.read_root_dir_entries()?;
            for component in parents {
                let pos = current.iter().position(|e| entry_matches(e, component));
                match pos {
                    None => {
                        return Err(FatError::PathNotFound {
                            path: path.to_string(),
                        })
                    }
                    Some(idx) if !current[idx].is_dir => {
                        return Err(FatError::NotADirectory {
                            path: path.to_string(),
                        })
                    }
                    Some(idx) => {
                        let cluster = current[idx].start_cluster;
                        current = self.read_cluster_chain_entries(cluster)?;
                    }
                }
            }
            current
        };

        // Find the target entry in the containing directory.
        let entry = dir_entries
            .iter()
            .find(|e| entry_matches(e, filename))
            .ok_or_else(|| FatError::PathNotFound {
                path: path.to_string(),
            })?;

        if entry.is_dir {
            return Err(FatError::IsADirectory {
                path: path.to_string(),
            });
        }

        if entry.size == 0 {
            return Ok(0);
        }

        if entry.start_cluster < 2 {
            return Err(FatError::InvalidCluster {
                cluster: entry.start_cluster,
            });
        }

        let cluster_size =
            usize::try_from(self.layout.cluster_size).map_err(|_| FatError::SizeOverflow)?;
        // Buffer is bounded by EXTRACTION_CHUNK_SIZE; for typical FAT cluster
        // sizes (≤ 32 KiB) this equals cluster_size.
        let buf_size = cluster_size.min(EXTRACTION_CHUNK_SIZE);
        let mut buf = vec![0u8; buf_size];

        let mut cluster = entry.start_cluster;
        let mut visited: HashSet<u32> = HashSet::new();
        let mut remaining = usize::try_from(entry.size).map_err(|_| FatError::SizeOverflow)?;
        let mut written: u64 = 0;

        loop {
            if !visited.insert(cluster) {
                return Err(FatError::ClusterChainCycle);
            }

            let sector = self.cluster_to_sector(cluster)?;
            let byte_offset = sector
                .checked_mul(self.layout.bytes_per_sector as u64)
                .ok_or(FatError::OutOfBounds)?;

            // Emit the relevant portion of this cluster in buf_size pieces.
            let to_emit = remaining.min(cluster_size);
            let mut cluster_pos: usize = 0;
            while cluster_pos < to_emit {
                let chunk = (to_emit - cluster_pos).min(buf_size);
                self.reader
                    .read_at(byte_offset + cluster_pos as u64, &mut buf[..chunk])?;
                writer
                    .write_all(&buf[..chunk])
                    .map_err(FatError::WriteError)?;
                written += chunk as u64;
                cluster_pos += chunk;
            }
            remaining -= to_emit;

            if remaining == 0 {
                break;
            }

            let next = self.next_cluster(cluster)?;
            if self.is_eof_cluster(next) {
                return Err(FatError::ClusterChainTooShort);
            }
            if next < 2 || next > self.layout.max_valid_cluster {
                return Err(FatError::InvalidCluster { cluster: next });
            }
            cluster = next;
        }

        Ok(written)
    }

    /// Reads the full contents of the file at `path` into a `Vec<u8>`.
    ///
    /// Implemented through [`Self::read_file_to_writer`] so behaviour is
    /// identical to the streaming path.
    ///
    /// # Errors
    ///
    /// Returns [`FatError`] when the path does not resolve to a file or the
    /// cluster chain is malformed.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FatError> {
        let mut out = Vec::new();
        self.read_file_to_writer(path, &mut out)?;
        Ok(out)
    }
}

/// Parse FAT attribute byte into individual flag fields.
pub(crate) fn parse_fat_attributes(attr: u8) -> FatAttributes {
    FatAttributes {
        read_only: attr & 0x01 != 0,
        hidden: attr & 0x02 != 0,
        system: attr & 0x04 != 0,
        directory: attr & 0x10 != 0,
        archive: attr & 0x20 != 0,
    }
}

/// Parse a raw FAT 16-bit date field.
///
/// Returns `None` when `raw` is zero or the decoded month or day is invalid.
/// The year field is stored as an offset from 1980.
pub(crate) fn parse_fat_date(raw: u16) -> Option<FatDate> {
    if raw == 0 {
        return None;
    }
    let year = ((raw >> 9) & 0x7F) + 1980;
    let month = ((raw >> 5) & 0x0F) as u8;
    let day = (raw & 0x1F) as u8;
    if month == 0 || day == 0 {
        return None;
    }
    Some(FatDate { year, month, day })
}

/// Parse a raw FAT 16-bit time field.
///
/// Returns `None` when `raw` is zero or the decoded hour, minute, or second
/// is out of range.  Seconds are stored as a two-second count (0–29).
pub(crate) fn parse_fat_time(raw: u16) -> Option<FatTime> {
    if raw == 0 {
        return None;
    }
    let hour = ((raw >> 11) & 0x1F) as u8;
    let minute = ((raw >> 5) & 0x3F) as u8;
    let second = ((raw & 0x1F) as u8).wrapping_mul(2);
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some(FatTime {
        hour,
        minute,
        second,
    })
}

/// Classify a raw 32-byte FAT directory entry.
/// Returns None for deleted, unused, volume-label, LFN, and dot entries.
pub(crate) fn classify_entry(raw: &[u8; 32]) -> Option<InternalEntry> {
    if raw[0] == 0xE5 || raw[0] == 0x00 {
        return None;
    }
    let attr = raw[11];
    // Volume label (0x08 set) or LFN (all four lower attr bits set = 0x0F)
    if attr & 0x08 != 0 || attr == 0x0F {
        return None;
    }

    let name = trim_fat_field(&raw[0..8]);
    let ext = trim_fat_field(&raw[8..11]);
    let full_name = if ext.is_empty() {
        name
    } else {
        format!("{name}.{ext}")
    };

    // Skip . and .. navigation entries
    if full_name == "." || full_name == ".." {
        return None;
    }

    let attributes = parse_fat_attributes(attr);
    let is_dir = attributes.directory;
    let size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);
    let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]) as u32;
    let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]) as u32;
    let start_cluster = (cluster_hi << 16) | cluster_lo;

    // Created timestamp: time at [14..16], date at [16..18]
    let created = {
        let t = parse_fat_time(u16::from_le_bytes([raw[14], raw[15]]));
        let d = parse_fat_date(u16::from_le_bytes([raw[16], raw[17]]));
        match (d, t) {
            (Some(date), Some(time)) => Some(FatTimestamp { date, time }),
            _ => None,
        }
    };

    // Modified timestamp: time at [22..24], date at [24..26]
    let modified = {
        let t = parse_fat_time(u16::from_le_bytes([raw[22], raw[23]]));
        let d = parse_fat_date(u16::from_le_bytes([raw[24], raw[25]]));
        match (d, t) {
            (Some(date), Some(time)) => Some(FatTimestamp { date, time }),
            _ => None,
        }
    };

    // Accessed date: at [18..20]
    let accessed = parse_fat_date(u16::from_le_bytes([raw[18], raw[19]]));

    Some(InternalEntry {
        short_name: full_name.clone(),
        name: full_name,
        is_dir,
        size,
        start_cluster,
        attributes,
        created,
        modified,
        accessed,
    })
}

/// Compute the standard FAT LFN checksum for an 11-byte 8.3 padded name.
///
/// Starting with 0u8, for each byte: `acc = acc.rotate_right(1).wrapping_add(byte)`.
pub(crate) fn lfn_checksum(name83: &[u8; 11]) -> u8 {
    name83
        .iter()
        .fold(0u8, |acc, &b| acc.rotate_right(1).wrapping_add(b))
}

/// Parse directory entries from a raw region with LFN sequence awareness.
///
/// Iterates 32-byte chunks. Stops at the first entry whose first byte is `0x00`.
/// Deleted entries (`0xE5`) clear any accumulated LFN state and are skipped.
/// LFN entries (attr `0x0F`) are collected into a pending buffer. When an 8.3
/// short entry is encountered, the pending LFN buffer is used to reconstruct
/// the long name; on any checksum mismatch the 8.3 short name is used instead.
pub(crate) fn parse_dir_entries_with_lfn(data: &[u8]) -> Vec<InternalEntry> {
    let mut entries = Vec::new();
    let mut pending_lfn: Vec<[u8; 32]> = Vec::new();

    for chunk in data.chunks_exact(32) {
        let mut raw = [0u8; 32];
        raw.copy_from_slice(chunk);

        let first_byte = raw[0];
        let attr = raw[11];

        // End of used directory entries.
        if first_byte == 0x00 {
            break;
        }

        // Deleted entry: discard any pending LFN state.
        if first_byte == 0xE5 {
            pending_lfn.clear();
            continue;
        }

        // LFN entry (all four lower attribute bits set).
        if attr == 0x0F {
            // LAST_LONG_ENTRY flag (0x40) in seq byte signals the start of a new
            // sequence; discard any previously accumulated orphaned fragments.
            if first_byte & 0x40 != 0 {
                pending_lfn.clear();
            }
            pending_lfn.push(raw);
            continue;
        }

        // Volume label: skip without producing an entry.
        if attr & 0x08 != 0 {
            pending_lfn.clear();
            continue;
        }

        // Standard 8.3 short entry: delegate to classify_entry for all field parsing.
        // classify_entry handles deleted/unused/volume-label/dot filtering.
        if let Some(mut entry) = classify_entry(&raw) {
            entry.name = reconstruct_long_name(&mut pending_lfn, &raw, &entry.short_name);
            entries.push(entry);
        } else {
            // classify_entry filtered it (dot, dotdot, or other skipped entry).
            pending_lfn.clear();
            continue;
        }

        pending_lfn.clear();
    }

    entries
}

/// Reconstruct a long name from the pending LFN buffer and the current short entry.
///
/// Returns the short name unchanged when the buffer is empty or any checksum fails.
/// Sorts collected LFN fragments by ascending sequence number before assembling
/// the UTF-16LE byte stream; seq=1 holds the beginning of the name, seq=N (with
/// the LAST_LONG_ENTRY flag) was seen first in the directory and holds the end.
fn reconstruct_long_name(
    pending_lfn: &mut Vec<[u8; 32]>,
    short_raw: &[u8; 32],
    short_name: &str,
) -> String {
    if pending_lfn.is_empty() {
        return short_name.to_owned();
    }

    let mut name83 = [0u8; 11];
    name83.copy_from_slice(&short_raw[0..11]);
    let expected = lfn_checksum(&name83);

    if pending_lfn.iter().any(|e| e[13] != expected) {
        pending_lfn.clear();
        return short_name.to_owned();
    }

    // Sort ascending by sequence number (mask LAST_LONG_ENTRY flag 0x40).
    pending_lfn.sort_by_key(|e| e[0] & 0x3F);

    // Assemble UTF-16LE bytes from name fields of each fragment in order.
    // Fields: bytes [1..11], [14..26], [28..32] — 5 + 6 + 2 = 13 code units each.
    // Bytes [26..28] are the LFN entry's reserved first-cluster field (always
    // zero), not part of the name.
    let mut utf16_bytes: Vec<u8> = Vec::with_capacity(pending_lfn.len() * 26);
    for entry in pending_lfn.iter() {
        utf16_bytes.extend_from_slice(&entry[1..11]);
        utf16_bytes.extend_from_slice(&entry[14..26]);
        utf16_bytes.extend_from_slice(&entry[28..32]);
    }

    decode_utf16le_name(&utf16_bytes)
}

/// Split a container path into non-empty components, stripping leading/trailing slashes.
fn parse_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Trim trailing spaces from a FAT name or extension field and return as a String.
fn trim_fat_field(bytes: &[u8]) -> String {
    let trimmed = match bytes.iter().rposition(|&b| b != b' ') {
        Some(pos) => &bytes[..=pos],
        None => &[],
    };
    trimmed.iter().map(|&b| b as char).collect()
}

/// Decode a FAT LFN name field stored as little-endian UTF-16 code units.
///
/// Iterates `bytes` in steps of 2.  Stops at the first U+0000 code unit.
/// Skips U+FFFF code units (padding).  Surrogate pairs are handled by
/// [`char::decode_utf16`]; unpaired surrogates yield U+FFFD (REPLACEMENT
/// CHARACTER).  If `bytes.len()` is odd the trailing byte is ignored.
/// The returned [`String`] is not Unicode-normalized.
pub(crate) fn decode_utf16le_name(bytes: &[u8]) -> String {
    let mut units: Vec<u16> = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let unit = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        i += 2;
        if unit == 0x0000 {
            break;
        }
        if unit == 0xFFFF {
            continue;
        }
        units.push(unit);
    }
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Returns true if `entry` matches `component` by long name or short 8.3 name.
///
/// Match order:
/// 1. Exact Unicode match on the long name (case-sensitive for non-ASCII).
/// 2. Case-folded match on the long name (handles typical ASCII long names).
/// 3. Case-insensitive ASCII match on the 8.3 short name.
///
/// This does not perform full Windows Unicode casefolding. Non-ASCII case
/// differences in long names will not match.
fn entry_matches(entry: &InternalEntry, component: &str) -> bool {
    // Exact long-name match (case-sensitive, Unicode-aware)
    if entry.name == component {
        return true;
    }
    // Case-folded long-name match (covers typical ASCII long names)
    if entry.name.to_lowercase() == component.to_lowercase() {
        return true;
    }
    // 8.3 short-name match (ASCII case-insensitive)
    if entry.short_name.eq_ignore_ascii_case(component) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use cryptovol_core::CryptovolError;

    /// Minimal BlockReader backed by a byte slice, for unit tests.
    struct SliceReader<'a>(&'a [u8]);

    impl<'a> BlockReader for SliceReader<'a> {
        fn len(&self) -> u64 {
            self.0.len() as u64
        }

        fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
            let start = offset as usize;
            let end = start
                .checked_add(buf.len())
                .ok_or(CryptovolError::OutOfBounds {
                    offset,
                    length: buf.len(),
                    file_len: self.len(),
                })?;
            if end > self.0.len() {
                return Err(CryptovolError::OutOfBounds {
                    offset,
                    length: buf.len(),
                    file_len: self.len(),
                });
            }
            buf.copy_from_slice(&self.0[start..end]);
            Ok(())
        }
    }

    /// Build a minimal 512-byte FAT16 BPB sector.
    fn fat16_bpb(
        bytes_per_sector: u16,
        sectors_per_cluster: u8,
        reserved: u16,
        fat_count: u8,
        root_entry_count: u16,
        total_sectors_16: u16,
        sectors_per_fat: u16,
    ) -> [u8; 512] {
        let mut buf = [0u8; 512];
        // bytes per sector
        buf[11..13].copy_from_slice(&bytes_per_sector.to_le_bytes());
        // sectors per cluster
        buf[13] = sectors_per_cluster;
        // reserved sector count
        buf[14..16].copy_from_slice(&reserved.to_le_bytes());
        // number of FATs
        buf[16] = fat_count;
        // root entry count
        buf[17..19].copy_from_slice(&root_entry_count.to_le_bytes());
        // total sectors 16
        buf[19..21].copy_from_slice(&total_sectors_16.to_le_bytes());
        // sectors per FAT 16
        buf[22..24].copy_from_slice(&sectors_per_fat.to_le_bytes());
        // boot signature
        buf[510] = 0x55;
        buf[511] = 0xAA;
        buf
    }

    /// Build a minimal 512-byte FAT32 BPB sector.
    fn fat32_bpb() -> [u8; 512] {
        let mut buf = [0u8; 512];
        // bytes per sector = 512
        buf[11..13].copy_from_slice(&512u16.to_le_bytes());
        // sectors per cluster = 8
        buf[13] = 8;
        // reserved sector count = 32
        buf[14..16].copy_from_slice(&32u16.to_le_bytes());
        // fat count = 2
        buf[16] = 2;
        // root entry count = 0 (FAT32)
        buf[17..19].copy_from_slice(&0u16.to_le_bytes());
        // total sectors 16 = 0 (FAT32 uses 32-bit field)
        buf[19..21].copy_from_slice(&0u16.to_le_bytes());
        // sectors per FAT 16 = 0 (FAT32 uses extended field)
        buf[22..24].copy_from_slice(&0u16.to_le_bytes());
        // total sectors 32: large enough for FAT32 (>= 65525 data clusters)
        // with 8 sec/cluster and 512 bytes/sec: need > 65525*8 data sectors
        // data sectors = total - reserved - fat_count*fat_size - root_dir_sectors
        // fat_size_32: set to 512; total_sectors_32 = 32 + 2*512 + 0 + 65530*8 = 32+1024+524240 = 525296
        buf[32..36].copy_from_slice(&525296u32.to_le_bytes());
        // sectors per FAT 32
        buf[36..40].copy_from_slice(&512u32.to_le_bytes());
        // root cluster = 2
        buf[44..48].copy_from_slice(&2u32.to_le_bytes());
        // boot signature
        buf[510] = 0x55;
        buf[511] = 0xAA;
        buf
    }

    fn set_fat32_root_cluster(buf: &mut [u8; 512], root_cluster: u32) {
        buf[44..48].copy_from_slice(&root_cluster.to_le_bytes());
    }

    #[test]
    fn parse_valid_fat16_bpb() {
        let sector = fat16_bpb(512, 4, 4, 2, 512, 20480, 20);
        let reader = SliceReader(&sector);
        let fs = FatFileSystem::open(&reader).expect("should parse valid FAT16 BPB");
        let meta = fs.metadata();
        assert_eq!(meta.fat_type, FatType::Fat16);
        assert_eq!(meta.bytes_per_sector, 512);
        assert_eq!(meta.sectors_per_cluster, 4);
        assert_eq!(meta.cluster_size, 2048);
    }

    #[test]
    fn parse_valid_fat32_bpb() {
        let sector = fat32_bpb();
        let reader = SliceReader(&sector);
        let fs = FatFileSystem::open(&reader).expect("should parse valid FAT32 BPB");
        assert_eq!(fs.metadata().fat_type, FatType::Fat32);
    }

    #[test]
    fn parse_valid_fat12_bpb() {
        // Small volume: sectors_per_cluster=1, reserved=1, fat_count=2, root_entry_count=16,
        // total_sectors_16=96, sectors_per_fat=1 -> data_sectors = 96 - 1 - 2 - 1 = 92
        // count_of_clusters = 92 / 1 = 92 < 4085 → FAT12
        let sector = fat16_bpb(512, 1, 1, 2, 16, 96, 1);
        let reader = SliceReader(&sector);
        let fs = FatFileSystem::open(&reader).expect("should parse valid FAT12 BPB");
        assert_eq!(fs.metadata().fat_type, FatType::Fat12);
    }

    #[test]
    fn reject_invalid_bytes_per_sector() {
        let sector = fat16_bpb(0, 4, 4, 2, 512, 20480, 20);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject bytes_per_sector=0");
        assert!(
            matches!(err, FatError::InvalidBootSector),
            "expected InvalidBootSector, got {err}"
        );
    }

    #[test]
    fn reject_missing_boot_signature() {
        let mut sector = fat16_bpb(512, 4, 4, 2, 512, 20480, 20);
        sector[510] = 0x00;
        sector[511] = 0x00;
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject missing boot signature");
        assert!(
            matches!(err, FatError::InvalidBootSector),
            "expected InvalidBootSector, got {err}"
        );
    }

    #[test]
    fn reject_non_power_of_two_sectors_per_cluster() {
        let sector = fat16_bpb(512, 3, 4, 2, 512, 20480, 20);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject invalid cluster size");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    #[test]
    fn reject_zero_fat_count() {
        let sector = fat16_bpb(512, 4, 4, 0, 512, 20480, 20);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject fat_count=0");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    #[test]
    fn reject_zero_fat_size() {
        let sector = fat16_bpb(512, 4, 4, 2, 512, 20480, 0);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject fat_size=0");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    #[test]
    fn reject_zero_total_sectors() {
        let sector = fat16_bpb(512, 4, 4, 2, 512, 0, 20);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject total_sectors=0");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    #[test]
    fn reject_data_area_outside_declared_volume() {
        let sector = fat16_bpb(512, 4, 4, 2, 512, 32, 20);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject invalid data area");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    #[test]
    fn reject_fat32_root_cluster_below_data_range() {
        let mut sector = fat32_bpb();
        set_fat32_root_cluster(&mut sector, 1);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject root cluster < 2");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    #[test]
    fn reject_fat32_root_cluster_beyond_data_range() {
        let mut sector = fat32_bpb();
        set_fat32_root_cluster(&mut sector, u32::MAX);
        let reader = SliceReader(&sector);
        let err = FatFileSystem::open(&reader).expect_err("should reject out-of-range root");
        assert!(matches!(err, FatError::InvalidBootSector));
    }

    fn make_entry(name8: &[u8; 8], ext3: &[u8; 3], attr: u8, size: u32) -> [u8; 32] {
        let mut raw = [0u8; 32];
        raw[0..8].copy_from_slice(name8);
        raw[8..11].copy_from_slice(ext3);
        raw[11] = attr;
        raw[28..32].copy_from_slice(&size.to_le_bytes());
        raw
    }

    #[test]
    fn parse_83_file_entry() {
        // "HELLO   " + "TXT", archive attr, size=36
        let raw = make_entry(b"HELLO   ", b"TXT", 0x20, 36);
        let entry = classify_entry(&raw).expect("should classify archive entry");
        assert_eq!(entry.name, "HELLO.TXT");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 36);
    }

    #[test]
    fn parse_83_dir_entry() {
        // "DIR     " + "   ", directory attr, size=0
        let raw = make_entry(b"DIR     ", b"   ", 0x10, 0);
        let entry = classify_entry(&raw).expect("should classify directory entry");
        assert_eq!(entry.name, "DIR");
        assert!(entry.is_dir);
        assert_eq!(entry.size, 0);
    }

    #[test]
    fn ignore_deleted_entry() {
        let mut raw = make_entry(b"HELLO   ", b"TXT", 0x20, 36);
        raw[0] = 0xE5;
        assert!(
            classify_entry(&raw).is_none(),
            "deleted entry should be None"
        );
    }

    #[test]
    fn ignore_unused_entry() {
        let mut raw = make_entry(b"HELLO   ", b"TXT", 0x20, 36);
        raw[0] = 0x00;
        assert!(
            classify_entry(&raw).is_none(),
            "unused entry should be None"
        );
    }

    #[test]
    fn ignore_volume_label_entry() {
        let raw = make_entry(b"MYLABEL ", b"   ", 0x08, 0);
        assert!(
            classify_entry(&raw).is_none(),
            "volume label should be None"
        );
    }

    #[test]
    fn ignore_lfn_entry() {
        let raw = make_entry(b"LONGNAME", b"   ", 0x0F, 0);
        assert!(classify_entry(&raw).is_none(), "LFN entry should be None");
    }

    // --- Synthetic FAT16 volume helpers ---

    const TEST_SECTOR_SIZE: usize = 512;
    const TEST_SECTORS_PER_CLUSTER: u8 = 1;
    const TEST_RESERVED: u16 = 4;
    const TEST_FAT_COUNT: u8 = 2;
    const TEST_ROOT_ENTRY_COUNT: u16 = 16;
    // total_sectors_16 set large enough to force FAT16 determination:
    // first_data_sector = 4 + 2*16 + 1 = 37
    // data_sectors = 16384 - 37 = 16347
    // count_of_clusters = 16347 / 1 = 16347 → FAT16
    const TEST_TOTAL_SECTORS: u16 = 16384;
    const TEST_SECTORS_PER_FAT: u16 = 16;

    // BPB-computed constants
    const TEST_FAT1_SECTOR: usize = TEST_RESERVED as usize; // sector 4
    const TEST_FAT2_SECTOR: usize = TEST_FAT1_SECTOR + TEST_SECTORS_PER_FAT as usize; // sector 20
    const TEST_ROOT_DIR_SECTOR: usize = TEST_FAT2_SECTOR + TEST_SECTORS_PER_FAT as usize; // sector 36
                                                                                          // root_dir_sectors = ceil(16*32/512) = 1
    const TEST_FIRST_DATA_SECTOR: usize = TEST_ROOT_DIR_SECTOR + 1; // sector 37
                                                                    // Cluster 2 → sector 37; cluster 3 → sector 38
    const TEST_CLUSTER2_SECTOR: usize = TEST_FIRST_DATA_SECTOR; // sector 37

    /// Build a complete but compact 40-sector FAT16 volume in memory.
    /// Root dir has: HELLO.TXT (file, size=36, cluster=3), DIR (directory, cluster=2),
    /// a volume label entry (ignored), then an unused (0x00) terminator.
    /// DIR's cluster (cluster 2, sector 37) contains: NESTED.TXT (file, size=17).
    fn make_fat16_volume() -> Vec<u8> {
        // Allocate enough sectors for our test data (0..=38)
        let num_sectors = TEST_CLUSTER2_SECTOR + 2;
        let mut vol = vec![0u8; num_sectors * TEST_SECTOR_SIZE];

        // Sector 0: BPB
        let bps = TEST_SECTOR_SIZE as u16;
        vol[11..13].copy_from_slice(&bps.to_le_bytes());
        vol[13] = TEST_SECTORS_PER_CLUSTER;
        vol[14..16].copy_from_slice(&TEST_RESERVED.to_le_bytes());
        vol[16] = TEST_FAT_COUNT;
        vol[17..19].copy_from_slice(&TEST_ROOT_ENTRY_COUNT.to_le_bytes());
        vol[19..21].copy_from_slice(&TEST_TOTAL_SECTORS.to_le_bytes());
        vol[22..24].copy_from_slice(&TEST_SECTORS_PER_FAT.to_le_bytes());
        vol[510] = 0x55;
        vol[511] = 0xAA;

        // FAT1 at sector 4
        let fat1 = TEST_FAT1_SECTOR * TEST_SECTOR_SIZE;
        // FAT16 entries: each is 2 bytes, little-endian
        vol[fat1..fat1 + 2].copy_from_slice(&0xFFF8u16.to_le_bytes()); // entry 0: media
        vol[fat1 + 2..fat1 + 4].copy_from_slice(&0xFFFFu16.to_le_bytes()); // entry 1: reserved
        vol[fat1 + 4..fat1 + 6].copy_from_slice(&0xFFFFu16.to_le_bytes()); // entry 2: DIR → EOF
        vol[fat1 + 6..fat1 + 8].copy_from_slice(&0xFFFFu16.to_le_bytes()); // entry 3: HELLO.TXT → EOF

        // FAT2 at sector 20 (copy of FAT1 header)
        let fat2 = TEST_FAT2_SECTOR * TEST_SECTOR_SIZE;
        vol.copy_within(fat1..fat1 + 8, fat2);

        // Root directory at sector 36
        let root = TEST_ROOT_DIR_SECTOR * TEST_SECTOR_SIZE;
        write_dir_entry(&mut vol[root..], b"HELLO   ", b"TXT", 0x20, 36, 3);
        write_dir_entry(&mut vol[root + 32..], b"DIR     ", b"   ", 0x10, 0, 2);
        // Volume label — must be ignored by list_dir
        write_dir_entry(&mut vol[root + 64..], b"MYVOL   ", b"   ", 0x08, 0, 0);
        // Entry at root+96: 0x00 byte → end of used entries (already zeroed)

        // Cluster 2 (DIR contents) at sector 37
        let dir_data = TEST_CLUSTER2_SECTOR * TEST_SECTOR_SIZE;
        write_dir_entry(&mut vol[dir_data..], b"NESTED  ", b"TXT", 0x20, 17, 4);
        // Entry at dir_data+32: 0x00 → end of used entries (already zeroed)

        vol
    }

    fn write_dir_entry(
        buf: &mut [u8],
        name: &[u8; 8],
        ext: &[u8; 3],
        attr: u8,
        size: u32,
        first_cluster: u16,
    ) {
        buf[0..8].copy_from_slice(name);
        buf[8..11].copy_from_slice(ext);
        buf[11] = attr;
        buf[26..28].copy_from_slice(&first_cluster.to_le_bytes());
        buf[28..32].copy_from_slice(&size.to_le_bytes());
    }

    // --- Directory listing tests (all fail until T-004 implements list_dir) ---

    #[test]
    fn list_root_returns_expected_entries() {
        let vol = make_fat16_volume();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let entries = fs.list_dir("/").expect("root listing should succeed");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"HELLO.TXT"),
            "missing HELLO.TXT, got {names:?}"
        );
        assert!(names.contains(&"DIR"), "missing DIR, got {names:?}");
        // Volume label must not appear
        assert!(
            !names.contains(&"MYVOL"),
            "volume label must be excluded, got {names:?}"
        );
        // Verify sizes and types
        let hello = entries.iter().find(|e| e.name == "HELLO.TXT").unwrap();
        assert!(!hello.is_dir);
        assert_eq!(hello.size, 36);
        let dir = entries.iter().find(|e| e.name == "DIR").unwrap();
        assert!(dir.is_dir);
    }

    #[test]
    fn list_subdir_returns_nested_entry() {
        let vol = make_fat16_volume();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let entries = fs.list_dir("/DIR").expect("/DIR listing should succeed");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"NESTED.TXT"),
            "missing NESTED.TXT, got {names:?}"
        );
        let nested = entries.iter().find(|e| e.name == "NESTED.TXT").unwrap();
        assert!(!nested.is_dir);
        assert_eq!(nested.size, 17);
    }

    #[test]
    fn path_lookup_is_case_insensitive() {
        let vol = make_fat16_volume();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let lower = fs.list_dir("/dir").expect("/dir should work");
        let mixed = fs.list_dir("/Dir").expect("/Dir should work");
        for result in [&lower, &mixed] {
            let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
            assert!(
                names.contains(&"NESTED.TXT"),
                "missing NESTED.TXT in {names:?}"
            );
        }
    }

    #[test]
    fn trailing_slash_is_accepted() {
        let vol = make_fat16_volume();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let with_slash = fs.list_dir("/DIR/").expect("/DIR/ should succeed");
        let without = fs.list_dir("/DIR").expect("/DIR should succeed");
        assert_eq!(with_slash.len(), without.len());
    }

    #[test]
    fn path_not_found_returns_error() {
        let vol = make_fat16_volume();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let err = fs.list_dir("/MISSING").expect_err("/MISSING should fail");
        assert!(
            matches!(err, FatError::PathNotFound { .. }),
            "expected PathNotFound, got {err}"
        );
    }

    #[test]
    fn file_as_path_returns_not_a_directory() {
        let vol = make_fat16_volume();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let err = fs
            .list_dir("/HELLO.TXT")
            .expect_err("/HELLO.TXT should fail");
        assert!(
            matches!(err, FatError::NotADirectory { .. }),
            "expected NotADirectory, got {err}"
        );
    }

    #[test]
    fn no_panic_on_random_data() {
        let data = vec![0xFFu8; 1024];
        let reader = SliceReader(&data);
        // open() may succeed or fail; if it succeeds, list_dir must not panic
        match FatFileSystem::open(&reader) {
            Err(_) => {} // acceptable
            Ok(fs) => {
                let _ = fs.list_dir("/"); // must not panic; error is acceptable
            }
        }
    }

    #[test]
    fn no_panic_on_truncated_sector() {
        let data = vec![0u8; 511]; // one byte short of a sector
        let reader = SliceReader(&data);
        // open() must return an error, not panic
        assert!(
            FatFileSystem::open(&reader).is_err(),
            "truncated sector should be an error"
        );
    }

    // --- Synthetic FAT16 volume with file data for read_file tests ---

    /// Build a 44-sector FAT16 volume (22528 bytes) containing:
    /// Root: FILE.TXT (36B, cluster 3), BIGFILE.BIN (600B, clusters 4→5),
    ///       DIR (dir, cluster 2), EMPTY.TXT (0B), CYCLE.TXT (512B, cluster 7→7 loop),
    ///       SHORT.TXT (claims 600B, single cluster 8 = only 512B), INVALID.TXT (36B, cluster 0).
    /// /DIR: NESTED.TXT (17B, cluster 6).
    /// Data: cluster 3 = 0x41*36 + 0xBB*476; cluster 4 = 0x42*512;
    ///       cluster 5 = 0x43*88 + 0xCC*424; cluster 6 = 0x44*17 + 0xDD*495;
    ///       cluster 7 = 0x45*512; cluster 8 = 0x46*512.
    fn make_fat16_vol_with_files() -> Vec<u8> {
        const S: usize = 512;
        const FAT1: usize = 4 * S; // byte 2048
        const FAT2: usize = 20 * S; // byte 10240
        const ROOT: usize = 36 * S; // byte 18432
                                    // first_data_sector = 37; cluster N → sector 37+(N-2)
        const C2: usize = 37 * S; // byte 18944: DIR contents
        const C3: usize = 38 * S; // byte 19456: FILE.TXT
        const C4: usize = 39 * S; // byte 19968: BIGFILE part 1
        const C5: usize = 40 * S; // byte 20480: BIGFILE part 2
        const C6: usize = 41 * S; // byte 20992: NESTED.TXT
        const C7: usize = 42 * S; // byte 21504: CYCLE.TXT (self-loop)
        const C8: usize = 43 * S; // byte 22016: SHORT.TXT (EOF after one cluster)

        let mut vol = vec![0u8; 44 * S];

        // Sector 0: BPB
        let bpb = fat16_bpb(512, 1, 4, 2, 16, 16384, 16);
        vol[0..S].copy_from_slice(&bpb);

        // FAT1 entries (2 bytes LE per cluster, at byte 2048)
        let fat_entries: [(usize, u16); 9] = [
            (0, 0xFFF8),
            (1, 0xFFFF),
            (2, 0xFFFF), // DIR: EOF
            (3, 0xFFFF), // FILE.TXT: EOF
            (4, 0x0005), // BIGFILE: → cluster 5
            (5, 0xFFFF), // BIGFILE: EOF
            (6, 0xFFFF), // NESTED.TXT: EOF
            (7, 0x0007), // CYCLE.TXT: self-loop
            (8, 0xFFFF), // SHORT.TXT: EOF (only one cluster)
        ];
        for (cluster, val) in fat_entries {
            let off = FAT1 + cluster * 2;
            vol[off..off + 2].copy_from_slice(&val.to_le_bytes());
        }
        // Copy FAT1 header to FAT2
        vol.copy_within(FAT1..FAT1 + 18, FAT2);

        // Root directory entries (sector 36)
        write_dir_entry(&mut vol[ROOT..], b"FILE    ", b"TXT", 0x20, 36, 3);
        write_dir_entry(&mut vol[ROOT + 32..], b"BIGFILE ", b"BIN", 0x20, 600, 4);
        write_dir_entry(&mut vol[ROOT + 64..], b"DIR     ", b"   ", 0x10, 0, 2);
        write_dir_entry(&mut vol[ROOT + 96..], b"EMPTY   ", b"TXT", 0x20, 0, 0);
        write_dir_entry(&mut vol[ROOT + 128..], b"CYCLE   ", b"TXT", 0x20, 1024, 7);
        write_dir_entry(&mut vol[ROOT + 160..], b"SHORT   ", b"TXT", 0x20, 600, 8);
        write_dir_entry(&mut vol[ROOT + 192..], b"INVALID ", b"TXT", 0x20, 36, 0);
        // entry 7 at ROOT+224: 0x00 already (end-of-used-entries)

        // DIR cluster contents (cluster 2 = C2)
        write_dir_entry(&mut vol[C2..], b"NESTED  ", b"TXT", 0x20, 17, 6);
        // entry 1 at C2+32: 0x00 already

        // Data cluster contents
        vol[C3..C3 + 36].fill(0x41); // FILE.TXT: 36 'A' bytes
        vol[C3 + 36..C3 + S].fill(0xBB); // sentinel: must not appear in result
        vol[C4..C4 + S].fill(0x42); // BIGFILE part 1: 512 'B' bytes
        vol[C5..C5 + 88].fill(0x43); // BIGFILE part 2: 88 'C' bytes
        vol[C5 + 88..C5 + S].fill(0xCC); // sentinel: must not appear in result
        vol[C6..C6 + 17].fill(0x44); // NESTED.TXT: 17 'D' bytes
        vol[C6 + 17..C6 + S].fill(0xDD); // sentinel: must not appear in result
        vol[C7..C7 + S].fill(0x45); // CYCLE.TXT: irrelevant (cycle detected)
        vol[C8..C8 + S].fill(0x46); // SHORT.TXT: only 512 bytes (claims 600)

        vol
    }

    // --- read_file unit tests (all fail via unimplemented!() until T-002) ---

    #[test]
    fn read_file_returns_correct_bytes() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let bytes = fs
            .read_file("/FILE.TXT")
            .expect("FILE.TXT should be readable");
        assert_eq!(bytes.len(), 36, "expected 36 bytes");
        assert_eq!(bytes, vec![0x41u8; 36], "expected all 0x41 bytes");
        assert!(
            !bytes.contains(&0xBB),
            "result must not contain sentinel 0xBB bytes"
        );
    }

    #[test]
    fn read_file_final_cluster_trimmed() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let bytes = fs
            .read_file("/FILE.TXT")
            .expect("FILE.TXT should be readable");
        assert_eq!(
            *bytes.last().expect("result must be non-empty"),
            0x41u8,
            "last byte must be 0x41, not 0xBB sentinel"
        );
    }

    #[test]
    fn read_file_multi_cluster_file() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let bytes = fs
            .read_file("/BIGFILE.BIN")
            .expect("BIGFILE.BIN should be readable");
        assert_eq!(bytes.len(), 600, "expected 600 bytes");
        assert_eq!(
            &bytes[0..512],
            &[0x42u8; 512],
            "first 512 bytes must be 0x42"
        );
        assert_eq!(
            &bytes[512..600],
            &[0x43u8; 88],
            "next 88 bytes must be 0x43"
        );
        assert!(
            !bytes.contains(&0xCC),
            "result must not contain sentinel 0xCC bytes"
        );
    }

    #[test]
    fn read_file_empty_file_returns_empty_vec() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let bytes = fs
            .read_file("/EMPTY.TXT")
            .expect("EMPTY.TXT should be readable (size 0)");
        assert!(bytes.is_empty(), "expected empty Vec for zero-size file");
    }

    #[test]
    fn read_file_rejects_directory() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let err = fs
            .read_file("/DIR")
            .expect_err("DIR is a directory; read_file must fail");
        assert!(
            matches!(err, FatError::IsADirectory { .. }),
            "expected IsADirectory, got {err}"
        );
    }

    #[test]
    fn read_file_path_not_found() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let err = fs
            .read_file("/MISSING.TXT")
            .expect_err("/MISSING.TXT must fail");
        assert!(
            matches!(err, FatError::PathNotFound { .. }),
            "expected PathNotFound, got {err}"
        );
    }

    #[test]
    fn read_file_nested_path_not_found() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let err = fs
            .read_file("/DIR/MISSING.TXT")
            .expect_err("/DIR/MISSING.TXT must fail");
        assert!(
            matches!(err, FatError::PathNotFound { .. }),
            "expected PathNotFound, got {err}"
        );
    }

    #[test]
    fn read_file_case_insensitive_lookup() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let bytes = fs
            .read_file("/file.txt")
            .expect("/file.txt must resolve case-insensitively");
        assert_eq!(
            bytes,
            vec![0x41u8; 36],
            "must return same bytes as /FILE.TXT"
        );
    }

    #[test]
    fn read_file_nested_case_insensitive() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let bytes = fs
            .read_file("/dir/nested.txt")
            .expect("/dir/nested.txt must resolve case-insensitively");
        assert_eq!(
            bytes,
            vec![0x44u8; 17],
            "must return same bytes as /DIR/NESTED.TXT"
        );
    }

    #[test]
    fn read_file_detects_cluster_cycle() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        // CYCLE.TXT: start_cluster=7, FAT[7]=7 (self-loop)
        let err = fs
            .read_file("/CYCLE.TXT")
            .expect_err("CYCLE.TXT has a self-loop cluster chain; must fail");
        assert!(
            matches!(err, FatError::ClusterChainCycle | FatError::OutOfBounds),
            "expected ClusterChainCycle or OutOfBounds, got {err}"
        );
    }

    #[test]
    fn read_file_chain_too_short() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        // SHORT.TXT: size=600, but chain has only one cluster (512 bytes)
        let err = fs
            .read_file("/SHORT.TXT")
            .expect_err("SHORT.TXT chain is shorter than file_size; must fail");
        assert!(
            matches!(err, FatError::ClusterChainTooShort),
            "expected ClusterChainTooShort, got {err}"
        );
    }

    #[test]
    fn read_file_invalid_first_cluster() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        // INVALID.TXT: size=36 but first_cluster=0 (invalid for non-empty file)
        let err = fs
            .read_file("/INVALID.TXT")
            .expect_err("INVALID.TXT has cluster=0 for a non-empty file; must fail");
        assert!(
            matches!(err, FatError::OutOfBounds | FatError::InvalidCluster { .. }),
            "expected OutOfBounds or InvalidCluster, got {err}"
        );
    }

    #[test]
    fn read_file_rejects_out_of_range_cluster() {
        let mut vol = make_fat16_vol_with_files();
        let root = 36 * TEST_SECTOR_SIZE;
        let invalid_entry = root + 192;
        vol[invalid_entry + 26..invalid_entry + 28].copy_from_slice(&60000u16.to_le_bytes());
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");

        let err = fs
            .read_file("/INVALID.TXT")
            .expect_err("out-of-range file cluster must fail");

        assert!(matches!(err, FatError::InvalidCluster { .. }));
    }

    #[test]
    fn list_dir_rejects_out_of_range_cluster() {
        let mut vol = make_fat16_vol_with_files();
        let root = 36 * TEST_SECTOR_SIZE;
        let dir_entry = root + 64;
        vol[dir_entry + 26..dir_entry + 28].copy_from_slice(&60000u16.to_le_bytes());
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");

        let err = fs
            .list_dir("/DIR")
            .expect_err("out-of-range directory cluster must fail");

        assert!(matches!(err, FatError::InvalidCluster { .. }));
    }

    #[test]
    fn list_dir_detects_cluster_cycle() {
        let mut vol = make_fat16_vol_with_files();
        let fat1 = 4 * TEST_SECTOR_SIZE;
        vol[fat1 + 4..fat1 + 6].copy_from_slice(&2u16.to_le_bytes());
        let dir_cluster = 37 * TEST_SECTOR_SIZE;
        for slot in 1..16 {
            write_dir_entry(
                &mut vol[dir_cluster + slot * 32..],
                b"MORE    ",
                b"TXT",
                0x20,
                1,
                3,
            );
        }

        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let err = fs
            .list_dir("/DIR")
            .expect_err("cyclic directory chain must fail");

        assert!(matches!(err, FatError::ClusterChainCycle));
    }

    #[test]
    fn read_file_no_panic_on_random_data() {
        let data = vec![0xFFu8; 4096];
        let reader = SliceReader(&data);
        match FatFileSystem::open(&reader) {
            Err(_) => {} // acceptable: open fails on garbage data
            Ok(fs) => {
                let _ = fs.read_file("/ANY.TXT"); // must not panic; any result is fine
            }
        }
    }

    mod lfn_decode_tests {
        #[test]
        fn decode_bmp_umlaut() {
            // äöü ÄÖÜ ß in UTF-16LE (precomposed BMP code points),
            // terminated by U+0000, followed by U+FFFF padding.
            #[rustfmt::skip]
            let bytes: &[u8] = &[
                0xE4, 0x00, // ä U+00E4
                0xF6, 0x00, // ö U+00F6
                0xFC, 0x00, // ü U+00FC
                0x20, 0x00, // space
                0xC4, 0x00, // Ä U+00C4
                0xD6, 0x00, // Ö U+00D6
                0xDC, 0x00, // Ü U+00DC
                0x20, 0x00, // space
                0xDF, 0x00, // ß U+00DF
                0x00, 0x00, // U+0000 terminator
                0xFF, 0xFF, // U+FFFF padding (must not appear in result)
            ];
            let result = super::decode_utf16le_name(bytes);
            assert_eq!(result, "äöü ÄÖÜ ß");
        }

        #[test]
        fn decode_combining_diaeresis() {
            // 'a' (U+0061) + COMBINING DIAERESIS (U+0308) + U+0000 terminator.
            // Decomposed sequence must be preserved; must NOT silently NFC-normalize to 'ä'.
            let bytes: &[u8] = &[0x61, 0x00, 0x08, 0x03, 0x00, 0x00];
            let result = super::decode_utf16le_name(bytes);
            assert_eq!(result.chars().count(), 2);
        }

        #[test]
        fn decode_surrogate_pair_rocket() {
            // U+1F680 🚀 as UTF-16LE surrogate pair [0x3D,0xD8, 0x80,0xDE] + U+0000 terminator.
            let bytes: &[u8] = &[0x3D, 0xD8, 0x80, 0xDE, 0x00, 0x00];
            let result = super::decode_utf16le_name(bytes);
            assert_eq!(result, "🚀");
        }

        #[test]
        fn decode_surrogate_pair_sweat() {
            // U+1F605 😅 as UTF-16LE surrogate pair [0x3D,0xD8, 0x05,0xDE] + U+0000 terminator.
            let bytes: &[u8] = &[0x3D, 0xD8, 0x05, 0xDE, 0x00, 0x00];
            let result = super::decode_utf16le_name(bytes);
            assert_eq!(result, "😅");
        }

        #[test]
        fn decode_invalid_utf16_no_panic() {
            // Unpaired high surrogate 0xD800 [0x00,0xD8] followed by U+0000 terminator.
            // Must not panic; may return a replacement-character substitution or an error.
            let bytes: &[u8] = &[0x00, 0xD8, 0x00, 0x00];
            let _ = super::decode_utf16le_name(bytes);
        }

        #[test]
        fn decode_stops_at_null() {
            // 'A' (U+0041) + U+0000 terminator — nothing after the null must appear.
            let bytes: &[u8] = &[0x41, 0x00, 0x00, 0x00];
            let result = super::decode_utf16le_name(bytes);
            assert_eq!(result, "A");
        }

        #[test]
        fn decode_skips_ffff_padding() {
            // U+FFFF U+FFFF followed by U+0000 — all padding before null → empty string.
            let bytes: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];
            let result = super::decode_utf16le_name(bytes);
            assert_eq!(result, "");
        }
    }

    mod lfn_parse_tests {
        /// Build a 32-byte FAT LFN directory entry.
        ///
        /// Encodes `name_fragment` as UTF-16LE across the three LFN name fields:
        ///   bytes [1..11]  = first 5 code units (10 bytes)
        ///   bytes [14..24] = next 5 code units  (10 bytes)
        ///   bytes [26..32] = next 3 code units  (6 bytes)
        ///
        /// Unused slots are padded with 0xFFFF; a single 0x0000 terminator is written
        /// immediately after the last code unit when the fragment is shorter than the
        /// 13-code-unit capacity.
        fn make_lfn_entry(seq: u8, name_fragment: &str, checksum: u8) -> [u8; 32] {
            let mut entry = [0u8; 32];
            entry[0] = seq;
            entry[11] = 0x0F;
            entry[13] = checksum;

            let units: Vec<u16> = name_fragment.encode_utf16().collect();
            // (field_base_byte, number_of_code_units)
            let fields: [(usize, usize); 3] = [(1, 5), (14, 6), (28, 2)];
            let mut pos = 0usize;
            let mut terminated = false;
            for (base, count) in fields {
                for i in 0..count {
                    let off = base + i * 2;
                    if pos < units.len() {
                        let [lo, hi] = units[pos].to_le_bytes();
                        entry[off] = lo;
                        entry[off + 1] = hi;
                        pos += 1;
                    } else if !terminated {
                        // null terminator marks end of name within this slot
                        entry[off] = 0x00;
                        entry[off + 1] = 0x00;
                        terminated = true;
                    } else {
                        // padding for remaining slots
                        entry[off] = 0xFF;
                        entry[off + 1] = 0xFF;
                    }
                }
            }

            entry
        }

        /// Build a 32-byte standard 8.3 FAT directory entry.
        ///
        /// `name83` is the 11-byte padded name (8-char name + 3-char ext, space-padded).
        /// `cluster_hi` / `cluster_lo` are split and stored at bytes [20..22] and [26..28].
        fn make_short_entry(name83: &[u8; 11], is_dir: bool, size: u32, cluster: u32) -> [u8; 32] {
            let mut entry = [0u8; 32];
            entry[0..11].copy_from_slice(name83);
            entry[11] = if is_dir { 0x10 } else { 0x20 };
            let cluster_hi = ((cluster >> 16) as u16).to_le_bytes();
            let cluster_lo = ((cluster & 0xFFFF) as u16).to_le_bytes();
            entry[20..22].copy_from_slice(&cluster_hi);
            entry[26..28].copy_from_slice(&cluster_lo);
            entry[28..32].copy_from_slice(&size.to_le_bytes());
            entry
        }

        /// Compute the standard FAT LFN checksum for an 11-byte 8.3 directory name.
        ///
        /// Starting with 0u8, for each byte: `acc = acc.rotate_right(1).wrapping_add(byte)`.
        fn lfn_checksum_of(name83: &[u8; 11]) -> u8 {
            let mut acc = 0u8;
            for &b in name83 {
                acc = acc.rotate_right(1).wrapping_add(b);
            }
            acc
        }

        #[test]
        fn single_lfn_entry() {
            // One LFN entry (seq=0x41 = LAST_LONG_ENTRY|1) followed by its 8.3 short entry.
            // The reconstructed long name must equal "Hello" and short_name must equal "HELLO.TXT".
            let name83 = b"HELLO   TXT";
            let checksum = lfn_checksum_of(name83);
            let lfn = make_lfn_entry(0x41, "Hello", checksum);
            let short = make_short_entry(name83, false, 0, 2);
            let mut data = [0u8; 64];
            data[0..32].copy_from_slice(&lfn);
            data[32..64].copy_from_slice(&short);
            let result = super::parse_dir_entries_with_lfn(&data);
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].name, "Hello");
            assert_eq!(result[0].short_name, "HELLO.TXT");
        }

        #[test]
        fn multi_lfn_entry() {
            // "Project Notes Final" (19 chars) spans two LFN entries (13 chars each).
            // In directory order, the entry with the highest sequence number (carrying the
            // LAST_LONG_ENTRY flag) appears first; it holds the END of the name.
            //   seq=0x42 (first entry, LAST_LONG_ENTRY|2): chars 14–19 = " Final"
            //   seq=0x01 (second entry):                   chars  1–13 = "Project Notes"
            // Joined in ascending-sequence order: "Project Notes" + " Final".
            let full_name = "Project Notes Final";
            let name83 = b"PROJNOT TXT";
            let checksum = lfn_checksum_of(name83);

            // High-sequence entry appears first; holds the tail of the name.
            let frag_high = &full_name[13..]; // " Final"
                                              // Low-sequence entry appears second; holds the head of the name.
            let frag_low = &full_name[..13]; // "Project Notes"

            let lfn_high = make_lfn_entry(0x42, frag_high, checksum);
            let lfn_low = make_lfn_entry(0x01, frag_low, checksum);
            let short = make_short_entry(name83, false, 0, 2);

            let mut data = [0u8; 96];
            data[0..32].copy_from_slice(&lfn_high);
            data[32..64].copy_from_slice(&lfn_low);
            data[64..96].copy_from_slice(&short);

            let result = super::parse_dir_entries_with_lfn(&data);
            assert_eq!(result.len(), 1);
            assert!(
                result[0].name.contains("Project"),
                "expected name containing 'Project', got {:?}",
                result[0].name
            );
        }

        #[test]
        fn checksum_mismatch_falls_back() {
            // An LFN entry whose checksum does not match the owning short entry's computed
            // checksum must be discarded; the result must use the 8.3 short name as the
            // display name (i.e. name == short_name).
            let name83 = b"HELLO   TXT";
            let good = lfn_checksum_of(name83);
            let bad = good.wrapping_add(1); // deliberately wrong
            let lfn = make_lfn_entry(0x41, "Hello", bad);
            let short = make_short_entry(name83, false, 0, 2);
            let mut data = [0u8; 64];
            data[0..32].copy_from_slice(&lfn);
            data[32..64].copy_from_slice(&short);
            let result = super::parse_dir_entries_with_lfn(&data);
            assert_eq!(
                result.len(),
                1,
                "expected one entry even on checksum mismatch"
            );
            assert_eq!(
                result[0].name, result[0].short_name,
                "checksum mismatch must fall back to the 8.3 short name"
            );
        }

        #[test]
        fn orphaned_lfn_no_panic() {
            // Two consecutive LFN entries that both carry the LAST_LONG_ENTRY flag.
            // The first sequence is orphaned because a new LFN sequence starts before
            // any 8.3 short entry closes it.  The parser must not panic and must
            // silently discard the orphaned sequence.
            let name83 = b"HELLO   TXT";
            let checksum = lfn_checksum_of(name83);
            let lfn_orphan = make_lfn_entry(0x41, "Orphan", checksum);
            let lfn_valid = make_lfn_entry(0x41, "Hello", checksum);
            let short = make_short_entry(name83, false, 0, 2);

            let mut data = [0u8; 96];
            data[0..32].copy_from_slice(&lfn_orphan);
            data[32..64].copy_from_slice(&lfn_valid);
            data[64..96].copy_from_slice(&short);

            // Must not panic; result may be empty or contain the non-orphaned entry.
            let _result = super::parse_dir_entries_with_lfn(&data);
        }

        #[test]
        fn no_lfn_falls_back_to_short() {
            // A standalone 8.3 entry with no preceding LFN entries must be returned
            // with its formatted short name as the display name.
            let name83 = b"HELLO   TXT";
            let short = make_short_entry(name83, false, 100, 3);
            let mut data = [0u8; 64];
            data[0..32].copy_from_slice(&short);
            // data[32..64] is all zeros — end-of-directory marker.
            let result = super::parse_dir_entries_with_lfn(&data);
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].name, "HELLO.TXT");
        }
    }

    mod metadata_tests {
        #[test]
        fn parse_fat_attributes_file() {
            let attrs = super::parse_fat_attributes(0x20);
            assert!(attrs.archive);
            assert!(!attrs.read_only);
            assert!(!attrs.hidden);
            assert!(!attrs.system);
            assert!(!attrs.directory);
        }

        #[test]
        fn parse_fat_attributes_dir() {
            let attrs = super::parse_fat_attributes(0x10);
            assert!(attrs.directory);
            assert!(!attrs.archive);
            assert!(!attrs.read_only);
        }

        #[test]
        fn parse_fat_attributes_readonly() {
            let attrs = super::parse_fat_attributes(0x01);
            assert!(attrs.read_only);
            assert!(!attrs.archive);
        }

        #[test]
        fn parse_fat_date_valid() {
            // FAT date: bits[15:9]=year offset from 1980, bits[8:5]=month, bits[4:0]=day.
            // 2026-06-28: year_offset=46=0x2E, month=6, day=28
            // raw = (46 << 9) | (6 << 5) | 28 = 0x5C00 | 0x00C0 | 0x001C = 0x5CDC
            let date = super::parse_fat_date(0x5CDC);
            let date = date.expect("should parse valid date");
            assert_eq!(date.year, 2026);
            assert_eq!(date.month, 6);
            assert_eq!(date.day, 28);
        }

        #[test]
        fn parse_fat_date_zero() {
            let date = super::parse_fat_date(0u16);
            assert!(date.is_none());
        }

        #[test]
        fn parse_fat_time_valid() {
            // FAT time: bits[15:11]=hours, bits[10:5]=minutes, bits[4:0]=two-second count.
            // 14:31:20 -> hours=14, minutes=31, two_sec=10 (10*2=20)
            // raw = (14 << 11) | (31 << 5) | 10 = 0x7000 | 0x03E0 | 0x000A = 0x73EA
            let time = super::parse_fat_time(0x73EA);
            let time = time.expect("should parse valid time");
            assert_eq!(time.hour, 14);
            assert_eq!(time.minute, 31);
            assert_eq!(time.second, 20);
        }

        #[test]
        fn parse_fat_time_zero() {
            let time = super::parse_fat_time(0u16);
            assert!(time.is_none());
        }

        #[test]
        fn directory_entry_has_metadata() {
            // Build a synthetic 8.3 entry for HELLO.TXT with archive attr (0x20),
            // size 1024, and a valid modified date/time.
            let mut raw = [0u8; 32];
            raw[0..8].copy_from_slice(b"HELLO   ");
            raw[8..11].copy_from_slice(b"TXT");
            raw[11] = 0x20; // archive
                            // modified time at [22..24]: 14:31:20 -> 0x73EA
            raw[22..24].copy_from_slice(&0x73EAu16.to_le_bytes());
            // modified date at [24..26]: 2026-06-28 -> 0x5CDC
            raw[24..26].copy_from_slice(&0x5CDCu16.to_le_bytes());
            // size at [28..32]
            raw[28..32].copy_from_slice(&1024u32.to_le_bytes());
            // cluster lo at [26..28]
            raw[26..28].copy_from_slice(&2u16.to_le_bytes());

            let attrs = super::parse_fat_attributes(raw[11]);
            assert!(attrs.archive);
            assert!(!attrs.directory);

            let modified_time = super::parse_fat_time(u16::from_le_bytes([raw[22], raw[23]]));
            let modified_date = super::parse_fat_date(u16::from_le_bytes([raw[24], raw[25]]));
            assert!(modified_time.is_some());
            assert!(modified_date.is_some());
            let t = modified_time.unwrap();
            let d = modified_date.unwrap();
            assert_eq!(t.hour, 14);
            assert_eq!(d.year, 2026);
        }
    }

    mod path_lookup_tests {
        // Helper functions re-declared here (private to lfn_parse_tests, not re-exportable).

        fn lfn_checksum_of(name83: &[u8; 11]) -> u8 {
            name83
                .iter()
                .fold(0u8, |acc, &b| acc.rotate_right(1).wrapping_add(b))
        }

        fn make_lfn_entry(seq: u8, name_fragment: &str, checksum: u8) -> [u8; 32] {
            let mut entry = [0u8; 32];
            entry[0] = seq;
            entry[11] = 0x0F;
            entry[13] = checksum;
            let units: Vec<u16> = name_fragment.encode_utf16().collect();
            let fields: [(usize, usize); 3] = [(1, 5), (14, 6), (28, 2)];
            let mut pos = 0usize;
            let mut terminated = false;
            for (base, count) in fields {
                for i in 0..count {
                    let off = base + i * 2;
                    if pos < units.len() {
                        let [lo, hi] = units[pos].to_le_bytes();
                        entry[off] = lo;
                        entry[off + 1] = hi;
                        pos += 1;
                    } else if !terminated {
                        entry[off] = 0x00;
                        entry[off + 1] = 0x00;
                        terminated = true;
                    } else {
                        entry[off] = 0xFF;
                        entry[off + 1] = 0xFF;
                    }
                }
            }
            entry
        }

        fn make_short_entry(name83: &[u8; 11], is_dir: bool, size: u32, cluster: u32) -> [u8; 32] {
            let mut entry = [0u8; 32];
            entry[0..11].copy_from_slice(name83);
            entry[11] = if is_dir { 0x10 } else { 0x20 };
            let cluster_hi = ((cluster >> 16) as u16).to_le_bytes();
            let cluster_lo = ((cluster & 0xFFFF) as u16).to_le_bytes();
            entry[20..22].copy_from_slice(&cluster_hi);
            entry[26..28].copy_from_slice(&cluster_lo);
            entry[28..32].copy_from_slice(&size.to_le_bytes());
            entry
        }

        /// Build a minimal FAT16 image with the given root directory entries.
        ///
        /// The image covers only the boot sector, two FATs, and the root directory
        /// region. Tests that read files with `size == 0` do not need data clusters.
        ///
        /// Layout:
        ///   sector 0       – BPB
        ///   sectors 4..24  – FAT1 (20 sectors)
        ///   sectors 24..44 – FAT2 (20 sectors)
        ///   sectors 44..45 – root dir (16 × 32 = 512 bytes)
        ///
        /// With total_sectors=20480 and spc=4: count_of_clusters ≈ 5108 → FAT16.
        fn make_minimal_fat16_with_lfn_root(root_entries: &[[u8; 32]]) -> Vec<u8> {
            const BPS: u16 = 512;
            const SPC: u8 = 4;
            const RESERVED: u16 = 4;
            const FAT_COUNT: u8 = 2;
            const SPF: u16 = 20;
            const ROOT_ENTRY_COUNT: u16 = 16;
            // Declared sector count only — actual image is smaller (no data clusters).
            const TOTAL_SECTORS: u16 = 20480;

            let bps = BPS as usize;
            let fat1_off = RESERVED as usize * bps;
            let fat2_off = fat1_off + SPF as usize * bps;
            let root_off = fat2_off + SPF as usize * bps;
            let root_size = ROOT_ENTRY_COUNT as usize * 32;
            let img_size = root_off + root_size;

            let mut img = vec![0u8; img_size];

            // BPB at sector 0.
            let bpb = super::fat16_bpb(
                BPS,
                SPC,
                RESERVED,
                FAT_COUNT,
                ROOT_ENTRY_COUNT,
                TOTAL_SECTORS,
                SPF,
            );
            img[0..bps].copy_from_slice(&bpb);

            // FAT1 and FAT2: media descriptor + cluster-1 reserved + cluster-2 EOF.
            let fat_header: [u8; 6] = [0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
            img[fat1_off..fat1_off + 6].copy_from_slice(&fat_header);
            img[fat2_off..fat2_off + 6].copy_from_slice(&fat_header);

            // Root directory entries; null terminator already 0x00 from vec init.
            for (i, entry) in root_entries.iter().enumerate() {
                let off = root_off + i * 32;
                img[off..off + 32].copy_from_slice(entry);
            }

            img
        }

        /// Looking up a file by its long name succeeds when the name is ASCII-only.
        ///
        /// The current lookup uses `entry.name.to_ascii_uppercase()`, which means
        /// ASCII long names match case-insensitively. This test documents that the
        /// long-name path works correctly with the existing comparison.
        #[test]
        fn lookup_by_long_name_ascii() {
            let name83: [u8; 11] = *b"PROJEC~1TXT";
            let checksum = lfn_checksum_of(&name83);
            // "Project Notes" exactly fills one LFN entry (13 code units, capacity 13).
            let lfn = make_lfn_entry(0x41, "Project Notes", checksum);
            let short = make_short_entry(&name83, false, 0, 2);

            let img = make_minimal_fat16_with_lfn_root(&[lfn, short]);
            let reader = super::SliceReader(&img);
            let fs = super::FatFileSystem::open(&reader).expect("should open");

            // Listing must expose the long name and the formatted short name.
            let entries = fs.list_dir("/").expect("should list root");
            assert_eq!(entries.len(), 1, "expected exactly one entry");
            assert_eq!(entries[0].name, "Project Notes");
            assert_eq!(entries[0].short_name, "PROJEC~1.TXT");

            // Lookup by exact long name must not return PathNotFound.
            // size=0 → read_file returns Ok(vec![]) without reading any cluster.
            let result = fs.read_file("/Project Notes");
            assert!(
                !matches!(result, Err(super::FatError::PathNotFound { .. })),
                "long name lookup must not return PathNotFound; got {result:?}",
            );
        }

        /// Looking up a file by its 8.3 short name fails with the current code.
        ///
        /// When an LFN entry is present, `entry.name` holds the long name. The
        /// current comparison (`entry.name.to_ascii_uppercase() == upper`) only
        /// checks the long-name field, so a path component matching the short name
        /// but not the long name returns `PathNotFound`.
        ///
        /// This test is RED until T-008 adds `entry_matches()` which checks both
        /// `entry.name` (long) and `entry.short_name` (8.3).
        #[test]
        fn lookup_by_short_name_fallback() {
            let name83: [u8; 11] = *b"PROJEC~1TXT";
            let checksum = lfn_checksum_of(&name83);
            let lfn = make_lfn_entry(0x41, "Project Notes", checksum);
            let short = make_short_entry(&name83, false, 0, 2);

            let img = make_minimal_fat16_with_lfn_root(&[lfn, short]);
            let reader = super::SliceReader(&img);
            let fs = super::FatFileSystem::open(&reader).expect("should open");

            // With current code:
            //   upper_filename               = "PROJEC~1.TXT"
            //   entry.name.to_ascii_uppercase() = "PROJECT NOTES"
            //   → no match → PathNotFound
            //
            // This assertion FAILS (red) until T-008 adds short-name fallback lookup.
            let result = fs.read_file("/PROJEC~1.TXT");
            assert!(
                !matches!(result, Err(super::FatError::PathNotFound { .. })),
                "short name lookup must not return PathNotFound; got {result:?}",
            );
        }

        /// Looking up a file by its decomposed-Unicode long name succeeds.
        ///
        /// When the path uses the exact decomposed form returned by `list_dir`,
        /// `to_ascii_uppercase` leaves non-ASCII code points unchanged on both
        /// sides of the comparison, so the lookup matches. This test also verifies
        /// that the combining diaeresis (U+0308) is not silently NFC-normalised
        /// during UTF-16LE decode.
        #[test]
        fn lookup_by_decomposed_unicode_long_name() {
            // "File a\u{0308}.txt" = 'a' (U+0061) + COMBINING DIAERESIS (U+0308)
            // in decomposed form — 11 Unicode code points, fits in one LFN entry.
            let name83: [u8; 11] = *b"FILEA ~1TXT";
            let checksum = lfn_checksum_of(&name83);
            let long_name = "File a\u{0308}.txt";
            let lfn = make_lfn_entry(0x41, long_name, checksum);
            let short = make_short_entry(&name83, false, 0, 2);

            let img = make_minimal_fat16_with_lfn_root(&[lfn, short]);
            let reader = super::SliceReader(&img);
            let fs = super::FatFileSystem::open(&reader).expect("should open");

            // Listing must preserve the decomposed form exactly.
            let entries = fs.list_dir("/").expect("should list root");
            assert_eq!(entries.len(), 1);
            let decoded = &entries[0].name;
            // 11 code points: F-i-l-e-space-a-U+0308-.-t-x-t
            // A precomposed normalisation to 'ä' would reduce this to 10.
            assert_eq!(
                decoded.chars().count(),
                11,
                "name must have 11 code points (decomposed form preserved); got {decoded:?}",
            );

            // Lookup by the exact decoded path must not return PathNotFound.
            // to_ascii_uppercase leaves U+0308 unchanged on both sides → match.
            let lookup_path = format!("/{decoded}");
            let result = fs.read_file(&lookup_path);
            assert!(
                !matches!(result, Err(super::FatError::PathNotFound { .. })),
                "decomposed unicode lookup must not return PathNotFound; got {result:?}",
            );
        }
    }

    // --- Helpers and tests for read_file_to_writer streaming (T-002 red) ---

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

    /// Builds a compact FAT16 volume for cluster-boundary streaming tests.
    ///
    /// Root: ONE.TXT (1B, cluster 2), FULL.TXT (512B, cluster 3),
    ///       OVER.TXT (513B, clusters 4→5).
    fn make_fat16_boundary_vol() -> Vec<u8> {
        const S: usize = 512;
        const FAT1: usize = 4 * S;
        const FAT2: usize = 20 * S;
        const ROOT: usize = 36 * S;
        const C2: usize = 37 * S; // ONE.TXT
        const C3: usize = 38 * S; // FULL.TXT
        const C4: usize = 39 * S; // OVER.TXT part 1
        const C5: usize = 40 * S; // OVER.TXT part 2

        let mut vol = vec![0u8; 41 * S];

        let bpb = fat16_bpb(512, 1, 4, 2, 16, 16384, 16);
        vol[0..S].copy_from_slice(&bpb);

        let fat_entries: [(usize, u16); 6] = [
            (0, 0xFFF8),
            (1, 0xFFFF),
            (2, 0xFFFF), // ONE.TXT: EOF
            (3, 0xFFFF), // FULL.TXT: EOF
            (4, 0x0005), // OVER.TXT: → cluster 5
            (5, 0xFFFF), // OVER.TXT: EOF
        ];
        for (cluster, val) in fat_entries {
            let off = FAT1 + cluster * 2;
            vol[off..off + 2].copy_from_slice(&val.to_le_bytes());
        }
        vol.copy_within(FAT1..FAT1 + 12, FAT2);

        write_dir_entry(&mut vol[ROOT..], b"ONE     ", b"TXT", 0x20, 1, 2);
        write_dir_entry(&mut vol[ROOT + 32..], b"FULL    ", b"TXT", 0x20, 512, 3);
        write_dir_entry(&mut vol[ROOT + 64..], b"OVER    ", b"TXT", 0x20, 513, 4);

        vol[C2] = 0xAA;
        vol[C2 + 1..C2 + S].fill(0xBB); // sentinel: must not appear in ONE.TXT result
        vol[C3..C3 + S].fill(0xCC); // FULL.TXT: 512 × 0xCC
        vol[C4..C4 + S].fill(0xDD); // OVER.TXT part 1: 512 × 0xDD
        vol[C5] = 0xEE;
        vol[C5 + 1..C5 + S].fill(0xFF); // sentinel: must not appear in OVER.TXT result

        vol
    }

    #[test]
    fn stream_zero_byte_file() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/EMPTY.TXT", &mut out)
            .expect("EMPTY.TXT streaming must succeed");
        assert_eq!(n, 0, "zero-byte file: bytes_written must be 0");
        assert!(
            out.is_empty(),
            "zero-byte file: writer must receive no data"
        );
    }

    #[test]
    fn stream_one_byte_file() {
        let vol = make_fat16_boundary_vol();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/ONE.TXT", &mut out)
            .expect("ONE.TXT streaming must succeed");
        assert_eq!(n, 1);
        assert_eq!(out, vec![0xAAu8], "one-byte file must be exactly 0xAA");
    }

    #[test]
    fn stream_exact_cluster_size_file() {
        let vol = make_fat16_boundary_vol();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/FULL.TXT", &mut out)
            .expect("FULL.TXT streaming must succeed");
        assert_eq!(n, 512);
        assert_eq!(out, vec![0xCCu8; 512]);
    }

    #[test]
    fn stream_cluster_plus_one_file() {
        let vol = make_fat16_boundary_vol();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/OVER.TXT", &mut out)
            .expect("OVER.TXT streaming must succeed");
        assert_eq!(n, 513);
        let mut expected = vec![0xDDu8; 512];
        expected.push(0xEE);
        assert_eq!(out, expected);
    }

    #[test]
    fn stream_multi_cluster_file() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/BIGFILE.BIN", &mut out)
            .expect("BIGFILE.BIN streaming must succeed");
        assert_eq!(n, 600);
        assert_eq!(&out[0..512], &[0x42u8; 512]);
        assert_eq!(&out[512..600], &[0x43u8; 88]);
        assert!(!out.contains(&0xCC), "sentinel 0xCC must not appear");
    }

    #[test]
    fn stream_nested_file() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/DIR/NESTED.TXT", &mut out)
            .expect("/DIR/NESTED.TXT streaming must succeed");
        assert_eq!(n, 17);
        assert_eq!(out, vec![0x44u8; 17]);
    }

    #[test]
    fn stream_directory_returns_error() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/DIR", &mut out)
            .expect_err("/DIR must fail: source is a directory");
        assert!(
            matches!(err, FatError::IsADirectory { .. }),
            "expected IsADirectory, got {err}"
        );
    }

    #[test]
    fn stream_not_found_returns_error() {
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/MISSING.TXT", &mut out)
            .expect_err("/MISSING.TXT must fail: path not found");
        assert!(
            matches!(err, FatError::PathNotFound { .. }),
            "expected PathNotFound, got {err}"
        );
    }

    #[test]
    fn stream_writer_called_multiple_times_for_multi_cluster() {
        // BIGFILE.BIN spans 2 clusters (512 bytes each); the streaming
        // implementation reads one cluster at a time, so the writer must
        // receive at least 2 calls.
        let vol = make_fat16_vol_with_files();
        let reader = SliceReader(&vol);
        let fs = FatFileSystem::open(&reader).expect("volume should parse");
        let mut writer = CountingWriter::new();
        let n = fs
            .read_file_to_writer("/BIGFILE.BIN", &mut writer)
            .expect("BIGFILE.BIN streaming must succeed");
        assert_eq!(n, 600);
        assert_eq!(writer.data.len(), 600);
        assert!(
            writer.calls >= 2,
            "2-cluster file must produce >= 2 write calls, got {}",
            writer.calls
        );
    }
}
