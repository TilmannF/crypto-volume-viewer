//! Read-only NTFS directory listing and single-file extraction.
//!
//! This crate implements boot sector parsing, MFT access, attribute parsing,
//! runlist decoding, directory index reading, and single-file extraction for
//! NTFS-formatted volumes. It does not support write operations, directory
//! extraction, alternate data streams, or mounting.

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
use thiserror::Error;

/// Errors returned by the NTFS reader.
#[derive(Debug, Error)]
pub enum NtfsError {
    /// The boot sector is invalid or does not identify as NTFS.
    #[error("invalid NTFS boot sector: {reason}")]
    InvalidBootSector {
        /// Human-readable reason for the rejection.
        reason: &'static str,
    },
    /// The MFT location derived from the boot sector is invalid.
    #[error("invalid MFT location")]
    InvalidMftLocation,
    /// A FILE record is malformed or has an unrecognised signature.
    #[error("invalid FILE record")]
    InvalidFileRecord,
    /// The Update Sequence Array fixup check failed for a FILE record.
    #[error("fixup validation failed")]
    FixupValidationFailed,
    /// An attribute header is malformed or out of bounds.
    #[error("malformed attribute")]
    MalformedAttribute,
    /// An `$ATTRIBUTE_LIST` attribute was encountered; not supported in this MVP.
    #[error("$ATTRIBUTE_LIST not supported")]
    UnsupportedAttributeList,
    /// A data runlist is malformed or truncated.
    #[error("malformed runlist")]
    MalformedRunlist,
    /// A compressed, sparse, or encrypted attribute was encountered.
    #[error("compressed/sparse/encrypted data not supported")]
    UnsupportedCompressedData,
    /// A named `$DATA` alternate data stream was encountered.
    #[error("named $DATA streams not supported")]
    UnsupportedNamedDataStream,
    /// The directory index structure is malformed.
    #[error("invalid directory index")]
    InvalidDirectoryIndex,
    /// A cluster number is out of the valid range.
    #[error("invalid cluster number {cluster}")]
    InvalidClusterNumber {
        /// The rejected cluster number (stored as u64 for display; may wrap from i64).
        cluster: u64,
    },
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
    /// An underlying block read failed.
    #[error("read error: {0}")]
    ReadError(#[from] CryptovolError),
    /// A write to the destination failed during streaming extraction.
    #[error("write error: {0}")]
    WriteError(std::io::Error),
}

/// File-system attributes for an NTFS directory entry.
#[derive(Debug, Clone, Default)]
pub struct NtfsAttributes {
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

/// An NTFS timestamp converted to seconds since the Unix epoch.
///
/// NTFS stores timestamps as 100-nanosecond intervals since 1601-01-01 UTC
/// (Windows FILETIME format). The conversion subtracts the Windows-to-Unix
/// epoch offset (116 444 736 000 000 000 ticks) and divides by 10 000 000
/// to obtain Unix seconds. Pre-1970 dates produce negative values. A zero
/// NTFS tick value is treated as absent and produces `None` from
/// [`NtfsTimestamp::from_windows_ticks`].
#[derive(Debug, Clone)]
pub struct NtfsTimestamp {
    /// Seconds since the Unix epoch (1970-01-01 00:00:00 UTC).
    /// Negative for pre-1970 dates.
    pub unix_seconds: i64,
}

impl NtfsTimestamp {
    /// Converts a Windows FILETIME tick count to an [`NtfsTimestamp`].
    ///
    /// Returns `None` when `ticks` is zero (sentinel for "no timestamp") or
    /// when the value predates the Unix epoch by more than can be represented.
    pub fn from_windows_ticks(ticks: u64) -> Option<Self> {
        /// 100-ns ticks between 1601-01-01 and 1970-01-01.
        const WINDOWS_TO_UNIX_OFFSET: u64 = 116_444_736_000_000_000;
        if ticks == 0 {
            return None;
        }
        let unix_ticks = ticks.checked_sub(WINDOWS_TO_UNIX_OFFSET)?;
        Some(Self {
            unix_seconds: (unix_ticks / 10_000_000) as i64,
        })
    }
}

/// A decoded NTFS directory entry (file or directory).
#[derive(Debug, Clone)]
pub struct NtfsEntry {
    /// On-disk filename, decoded from UTF-16LE without NFC normalization.
    pub name: String,
    /// `true` when this entry represents a directory.
    pub is_dir: bool,
    /// Valid data length in bytes (from the `$DATA` attribute or directory index).
    pub size: u64,
    /// File-system attributes parsed from `$STANDARD_INFORMATION`.
    pub attributes: NtfsAttributes,
    /// Creation timestamp, if present and non-zero.
    pub created: Option<NtfsTimestamp>,
    /// Last-modified timestamp, if present and non-zero.
    pub modified: Option<NtfsTimestamp>,
    /// Last-accessed timestamp, if present and non-zero.
    pub accessed: Option<NtfsTimestamp>,
}

/// Volume geometry decoded from the NTFS BPB.
#[allow(dead_code)] // fields consumed by later MFT/index tasks
struct NtfsGeometry {
    bytes_per_sector: u64,
    sectors_per_cluster: u64,
    cluster_size: u64,
    total_sectors: u64,
    mft_offset: u64,
    mft_mirror_offset: u64,
    file_record_size: u64,
    index_buffer_size: u64,
    volume_serial: u64,
}

/// Read-only NTFS filesystem accessor.
///
/// Open a volume with [`NtfsFileSystem::open`], then use [`list_dir`],
/// [`stat`], and [`read_file`] to inspect and extract content.
///
/// [`list_dir`]: NtfsFileSystem::list_dir
/// [`stat`]: NtfsFileSystem::stat
/// [`read_file`]: NtfsFileSystem::read_file
pub struct NtfsFileSystem<R> {
    reader: R,
    geo: NtfsGeometry,
    mft_runs: Vec<(u64, u64)>,
}

impl<R> core::fmt::Debug for NtfsFileSystem<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NtfsFileSystem").finish_non_exhaustive()
    }
}

impl<R: BlockReader> NtfsFileSystem<R> {
    /// Opens an NTFS volume backed by `reader`.
    ///
    /// Parses the boot sector, bootstraps the MFT, and validates the volume
    /// geometry. Returns an error if the volume is not a recognisable NTFS
    /// volume or if required structures are malformed.
    ///
    /// # Errors
    ///
    /// Returns [`NtfsError`] on any parse or I/O failure.
    pub fn open(reader: R) -> Result<Self, NtfsError> {
        let mut buf = [0u8; 512];
        reader.read_at(0, &mut buf)?;
        let geo = parse_boot_sector(&buf, reader.len())?;
        let mft_runs = bootstrap_mft_runs(&reader, &geo)?;
        Ok(Self {
            reader,
            geo,
            mft_runs,
        })
    }

    /// Lists the entries in the directory at `path`.
    ///
    /// `path` must use `/` as a separator. The root directory is `"/"`.
    ///
    /// # Errors
    ///
    /// Returns [`NtfsError::PathNotFound`] when the path does not exist.
    pub fn list_dir(&self, path: &str) -> Result<Vec<NtfsEntry>, NtfsError> {
        let resolved = self.resolve_path(path)?;
        if !resolved.is_dir {
            return Err(NtfsError::PathNotFound {
                path: path.to_string(),
            });
        }
        self.list_record(resolved.record)
    }

    /// Returns metadata for the entry at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`NtfsError::PathNotFound`] when the path does not exist.
    pub fn stat(&self, path: &str) -> Result<NtfsEntry, NtfsError> {
        let resolved = self.resolve_path(path)?;
        self.entry_from_record(resolved.record, path, Some(&resolved))
    }

    /// Reads and returns the full content of the file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`NtfsError::PathNotFound`] when the path does not exist,
    /// [`NtfsError::AttemptedDirectoryExtraction`] when the path is a directory,
    /// [`NtfsError::WriteError`] when `writer` returns an error, or
    /// [`NtfsError::ReadError`] on I/O failure.
    pub fn read_file_to_writer<W: std::io::Write>(
        &self,
        path: &str,
        writer: &mut W,
    ) -> Result<u64, NtfsError> {
        let resolved = self.resolve_path(path)?;
        if resolved.is_dir {
            return Err(NtfsError::AttemptedDirectoryExtraction {
                path: path.to_string(),
            });
        }
        let record = self.read_mft_record(resolved.record)?;
        let (attr_offset, hdr) =
            find_attribute(&record, 0x80, true)?.ok_or_else(|| NtfsError::PathNotFound {
                path: path.to_string(),
            })?;
        check_unnamed_data(&hdr)?;
        check_unsupported_flags(&hdr)?;
        let attr = attribute_slice(&record, attr_offset, &hdr)?;
        if hdr.non_resident {
            let runlist = runlist_slice(attr, &hdr)?;
            let runs = parse_runlist(runlist)?;
            stream_nonresident_data(
                &self.reader,
                &runs,
                self.geo.cluster_size,
                hdr.data_size,
                writer,
            )
        } else {
            let bytes = read_resident_data(attr, &hdr)?;
            let n = bytes.len() as u64;
            writer.write_all(&bytes).map_err(NtfsError::WriteError)?;
            Ok(n)
        }
    }

    /// Reads and returns the full content of the file at `path` into a `Vec<u8>`.
    ///
    /// Delegates to [`Self::read_file_to_writer`]; prefer `read_file_to_writer`
    /// for large files to avoid buffering the entire content in memory.
    ///
    /// # Errors
    ///
    /// Returns [`NtfsError::PathNotFound`] when the path does not exist,
    /// [`NtfsError::AttemptedDirectoryExtraction`] when the path is a directory.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, NtfsError> {
        let mut out = Vec::new();
        self.read_file_to_writer(path, &mut out)?;
        Ok(out)
    }

    fn read_mft_record(&self, record_number: u64) -> Result<Vec<u8>, NtfsError> {
        if self.mft_runs.is_empty() {
            return Err(NtfsError::InvalidMftLocation);
        }
        let record_byte = record_number
            .checked_mul(self.geo.file_record_size)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        let record_vcn = record_byte / self.geo.cluster_size;
        let offset_in_cluster = record_byte % self.geo.cluster_size;
        let mut base_vcn = 0u64;

        for &(run_lcn, run_len) in &self.mft_runs {
            let run_end_vcn = base_vcn
                .checked_add(run_len)
                .ok_or(NtfsError::OutOfBoundsRead)?;
            if record_vcn >= base_vcn && record_vcn < run_end_vcn {
                let cluster_delta = record_vcn - base_vcn;
                let physical_lcn = run_lcn
                    .checked_add(cluster_delta)
                    .ok_or(NtfsError::OutOfBoundsRead)?;
                let physical_offset = physical_lcn
                    .checked_mul(self.geo.cluster_size)
                    .and_then(|v| v.checked_add(offset_in_cluster))
                    .ok_or(NtfsError::OutOfBoundsRead)?;
                let record_len = usize::try_from(self.geo.file_record_size)
                    .map_err(|_| NtfsError::OutOfBoundsRead)?;
                let mut record = vec![0u8; record_len];
                self.reader.read_at(physical_offset, &mut record)?;
                apply_file_record_fixup(&mut record, self.geo.bytes_per_sector)?;
                return Ok(record);
            }
            base_vcn = run_end_vcn;
        }

        Err(NtfsError::InvalidMftLocation)
    }

    fn list_record(&self, record_number: u64) -> Result<Vec<NtfsEntry>, NtfsError> {
        Ok(self
            .index_entries(record_number)?
            .into_iter()
            .map(NtfsEntry::from)
            .collect())
    }

    fn index_entries(&self, record_number: u64) -> Result<Vec<IndexEntry>, NtfsError> {
        let record = self.read_mft_record(record_number)?;
        let (attr_offset, hdr) =
            find_attribute(&record, 0x90, false)?.ok_or(NtfsError::InvalidDirectoryIndex)?;
        if hdr.non_resident {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        let attr = attribute_slice(&record, attr_offset, &hdr)?;
        let value = read_resident_data(attr, &hdr)?;
        let mut entries = parse_index_root_entries(&value)?;
        entries.extend(self.index_allocation_entries(&record)?);
        Ok(entries)
    }

    fn index_allocation_entries(&self, record: &[u8]) -> Result<Vec<IndexEntry>, NtfsError> {
        let Some((attr_offset, hdr)) = find_attribute(record, 0xA0, false)? else {
            return Ok(Vec::new());
        };
        if !hdr.non_resident {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        check_unsupported_flags(&hdr)?;
        let attr = attribute_slice(record, attr_offset, &hdr)?;
        let runs = parse_runlist(runlist_slice(attr, &hdr)?)?;
        let bytes =
            read_nonresident_data(&self.reader, &runs, self.geo.cluster_size, hdr.data_size)?;
        parse_index_allocation_entries(
            &bytes,
            self.geo.index_buffer_size,
            self.geo.bytes_per_sector,
        )
    }

    fn resolve_path(&self, path: &str) -> Result<ResolvedPath, NtfsError> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return Ok(ResolvedPath {
                record: 5,
                is_dir: true,
                size: 0,
            });
        }

        let mut current_record = 5u64;
        let mut current_is_dir = true;
        let mut current_size = 0u64;
        for (idx, part) in parts.iter().enumerate() {
            if !current_is_dir {
                return Err(NtfsError::PathNotFound {
                    path: path.to_string(),
                });
            }
            let entries = self.index_entries(current_record)?;
            let found = entries
                .into_iter()
                .find(|entry| entry.name == *part)
                .or_else(|| {
                    self.index_entries(current_record).ok().and_then(|entries| {
                        entries
                            .into_iter()
                            .find(|entry| entry.name.eq_ignore_ascii_case(part))
                    })
                })
                .ok_or_else(|| NtfsError::PathNotFound {
                    path: path.to_string(),
                })?;
            current_record = found.mft_record;
            let target_record = self.read_mft_record(current_record)?;
            current_is_dir = found.is_dir || record_flags_is_dir(&target_record);
            current_size = file_name_info_from_record(&target_record)?
                .map(|info| info.size)
                .unwrap_or(found.file_size);
            if idx + 1 < parts.len() && !current_is_dir {
                return Err(NtfsError::PathNotFound {
                    path: path.to_string(),
                });
            }
        }

        Ok(ResolvedPath {
            record: current_record,
            is_dir: current_is_dir,
            size: current_size,
        })
    }

    fn entry_from_record(
        &self,
        record_number: u64,
        path: &str,
        resolved: Option<&ResolvedPath>,
    ) -> Result<NtfsEntry, NtfsError> {
        let record = self.read_mft_record(record_number)?;
        let names = file_names_from_record(&record)?;
        let best_name = best_filename(&names)
            .map(str::to_string)
            .or_else(|| {
                path.rsplit('/')
                    .find(|part| !part.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| ".".to_string());
        let mut entry = resolved
            .map(|r| NtfsEntry {
                name: best_name.clone(),
                is_dir: r.is_dir,
                size: r.size,
                attributes: NtfsAttributes::default(),
                created: None,
                modified: None,
                accessed: None,
            })
            .unwrap_or_else(|| NtfsEntry {
                name: best_name.clone(),
                is_dir: record_flags_is_dir(&record),
                size: 0,
                attributes: NtfsAttributes::default(),
                created: None,
                modified: None,
                accessed: None,
            });

        if let Some(file_name) = file_name_info_from_record(&record)? {
            // OR rather than overwrite: `entry.is_dir` may already reflect the
            // FILE record header's own directory flag (`record_flags_is_dir`),
            // which is the more authoritative source and should not be
            // discarded if `file_name.is_dir` were ever wrong.
            entry.is_dir = entry.is_dir || file_name.is_dir;
            entry.size = file_name.size;
            entry.attributes = file_name.attributes;
        }
        if let Some(si) = standard_info_from_record(&record)? {
            entry.attributes = si.attributes;
            entry.created = si.created;
            entry.modified = si.modified;
            entry.accessed = si.accessed;
        }
        if let Some((attr_offset, hdr)) = find_attribute(&record, 0x80, true)? {
            check_unnamed_data(&hdr)?;
            check_unsupported_flags(&hdr)?;
            entry.size = if hdr.non_resident {
                hdr.data_size
            } else {
                let _ = attribute_slice(&record, attr_offset, &hdr)?;
                u64::from(hdr.resident_value_length)
            };
        }
        entry.name = best_name;
        // `$STANDARD_INFORMATION`'s attributes field never encodes
        // directory-ness (see `FILE_NAME_DIRECTORY_FLAG`), so its overwrite
        // above may have reset `attributes.directory` to `false` even for a
        // real directory. Resync it from the already-correct `entry.is_dir`.
        entry.attributes.directory = entry.is_dir;
        Ok(entry)
    }
}

#[derive(Debug, Clone)]
struct ResolvedPath {
    record: u64,
    is_dir: bool,
    size: u64,
}

#[derive(Debug, Clone)]
struct FileNameInfo {
    namespace: u8,
    name: String,
    size: u64,
    is_dir: bool,
    attributes: NtfsAttributes,
}

#[derive(Debug, Clone)]
struct StandardInfo {
    attributes: NtfsAttributes,
    created: Option<NtfsTimestamp>,
    modified: Option<NtfsTimestamp>,
    accessed: Option<NtfsTimestamp>,
}

fn bootstrap_mft_runs<R: BlockReader>(
    reader: &R,
    geo: &NtfsGeometry,
) -> Result<Vec<(u64, u64)>, NtfsError> {
    let record_len =
        usize::try_from(geo.file_record_size).map_err(|_| NtfsError::OutOfBoundsRead)?;
    let record_end = geo
        .mft_offset
        .checked_add(geo.file_record_size)
        .ok_or(NtfsError::OutOfBoundsRead)?;
    if record_end > reader.len() {
        return Ok(Vec::new());
    }

    let mut record = vec![0u8; record_len];
    reader.read_at(geo.mft_offset, &mut record)?;
    apply_file_record_fixup(&mut record, geo.bytes_per_sector)?;
    let (attr_offset, hdr) =
        find_attribute(&record, 0x80, true)?.ok_or(NtfsError::InvalidMftLocation)?;
    check_unnamed_data(&hdr)?;
    check_unsupported_flags(&hdr)?;
    if !hdr.non_resident {
        return Err(NtfsError::InvalidMftLocation);
    }
    let attr = attribute_slice(&record, attr_offset, &hdr)?;
    let runs = parse_runlist(runlist_slice(attr, &hdr)?)?;
    if runs.is_empty() {
        return Err(NtfsError::InvalidMftLocation);
    }
    Ok(runs)
}

fn first_attr_offset(record: &[u8]) -> Result<usize, NtfsError> {
    if record.len() < 22 {
        return Err(NtfsError::InvalidFileRecord);
    }
    let offset = usize::from(u16::from_le_bytes([record[20], record[21]]));
    if offset >= record.len() {
        return Err(NtfsError::InvalidFileRecord);
    }
    Ok(offset)
}

fn find_attribute(
    record: &[u8],
    attr_type: u32,
    unnamed_only: bool,
) -> Result<Option<(usize, AttrHeader)>, NtfsError> {
    let mut pos = first_attr_offset(record)?;
    loop {
        let Some(hdr) = parse_attribute_header(record, pos)? else {
            return Ok(None);
        };
        if hdr.attr_type == 0x20 {
            return Err(NtfsError::UnsupportedAttributeList);
        }
        if hdr.attr_type == attr_type && (!unnamed_only || hdr.name_length == 0) {
            return Ok(Some((pos, hdr)));
        }
        pos = pos
            .checked_add(hdr.length as usize)
            .ok_or(NtfsError::MalformedAttribute)?;
        if pos >= record.len() {
            return Err(NtfsError::MalformedAttribute);
        }
    }
}

fn attribute_slice<'a>(
    record: &'a [u8],
    attr_offset: usize,
    hdr: &AttrHeader,
) -> Result<&'a [u8], NtfsError> {
    let end = attr_offset
        .checked_add(hdr.length as usize)
        .ok_or(NtfsError::OutOfBoundsRead)?;
    if end > record.len() {
        return Err(NtfsError::OutOfBoundsRead);
    }
    Ok(&record[attr_offset..end])
}

fn runlist_slice<'a>(attr: &'a [u8], hdr: &AttrHeader) -> Result<&'a [u8], NtfsError> {
    let start = usize::from(hdr.data_runs_offset);
    if start > attr.len() {
        return Err(NtfsError::MalformedRunlist);
    }
    Ok(&attr[start..])
}

fn record_flags_is_dir(record: &[u8]) -> bool {
    record
        .get(22..24)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) & 0x02 != 0)
        .unwrap_or(false)
}

fn file_names_from_record(record: &[u8]) -> Result<Vec<(u8, String)>, NtfsError> {
    let infos = all_file_name_info(record)?;
    Ok(infos
        .into_iter()
        .map(|info| (info.namespace, info.name))
        .collect())
}

fn file_name_info_from_record(record: &[u8]) -> Result<Option<FileNameInfo>, NtfsError> {
    let infos = all_file_name_info(record)?;
    if infos.is_empty() {
        return Ok(None);
    }
    let idx = infos
        .iter()
        .position(|info| info.namespace != 2)
        .unwrap_or(0);
    Ok(Some(infos[idx].clone()))
}

fn all_file_name_info(record: &[u8]) -> Result<Vec<FileNameInfo>, NtfsError> {
    let mut infos = Vec::new();
    let mut pos = first_attr_offset(record)?;
    loop {
        let Some(hdr) = parse_attribute_header(record, pos)? else {
            return Ok(infos);
        };
        if hdr.attr_type == 0x30 {
            if hdr.non_resident {
                return Err(NtfsError::MalformedAttribute);
            }
            let attr = attribute_slice(record, pos, &hdr)?;
            let value = read_resident_data(attr, &hdr)?;
            infos.push(parse_file_name_value(&value)?);
        }
        pos = pos
            .checked_add(hdr.length as usize)
            .ok_or(NtfsError::MalformedAttribute)?;
        if pos >= record.len() {
            return Err(NtfsError::MalformedAttribute);
        }
    }
}

fn parse_file_name_value(value: &[u8]) -> Result<FileNameInfo, NtfsError> {
    if value.len() < 66 {
        return Err(NtfsError::MalformedAttribute);
    }
    let size = u64_le(value, 48);
    let raw_attrs = u32_le(value, 56) as u32;
    let filename_length = value[64] as usize;
    let namespace = value[65];
    let name_bytes = filename_length
        .checked_mul(2)
        .and_then(|len| 66usize.checked_add(len))
        .ok_or(NtfsError::MalformedAttribute)?;
    if name_bytes > value.len() {
        return Err(NtfsError::MalformedAttribute);
    }
    let units: Vec<u16> = (0..filename_length)
        .map(|i| u16::from_le_bytes([value[66 + i * 2], value[66 + i * 2 + 1]]))
        .collect();
    let name = decode_utf16le(&units)?;
    let attributes = ntfs_attributes_from_bits(raw_attrs);
    Ok(FileNameInfo {
        namespace,
        name,
        size,
        is_dir: attributes.directory,
        attributes,
    })
}

fn standard_info_from_record(record: &[u8]) -> Result<Option<StandardInfo>, NtfsError> {
    let Some((attr_offset, hdr)) = find_attribute(record, 0x10, true)? else {
        return Ok(None);
    };
    if hdr.non_resident {
        return Err(NtfsError::MalformedAttribute);
    }
    let attr = attribute_slice(record, attr_offset, &hdr)?;
    let value = read_resident_data(attr, &hdr)?;
    if value.len() < 32 {
        return Ok(Some(StandardInfo {
            attributes: NtfsAttributes::default(),
            created: None,
            modified: None,
            accessed: None,
        }));
    }
    let raw_attrs = if value.len() >= 36 {
        u32_le(&value, 32) as u32
    } else {
        0
    };
    Ok(Some(StandardInfo {
        attributes: ntfs_attributes_from_bits(raw_attrs),
        created: NtfsTimestamp::from_windows_ticks(u64_le(&value, 0)),
        modified: NtfsTimestamp::from_windows_ticks(u64_le(&value, 8)),
        accessed: NtfsTimestamp::from_windows_ticks(u64_le(&value, 24)),
    }))
}

/// In an NTFS `$FILE_NAME` attribute's "file attributes" field, a directory
/// is signaled by this bit — not by the standard DOS `FILE_ATTRIBUTE_DIRECTORY`
/// bit (`0x10`), which is never set in this field. Windows repurposes the
/// high bits of this field for index-context flags: `0x1000_0000` marks a
/// directory, `0x2000_0000` marks an index view (e.g. `$Secure`) rather than
/// a plain directory. `$STANDARD_INFORMATION`'s attributes field never sets
/// either bit, so checking this bit there is harmlessly always `false`.
const FILE_NAME_DIRECTORY_FLAG: u32 = 0x1000_0000;

fn ntfs_attributes_from_bits(bits: u32) -> NtfsAttributes {
    NtfsAttributes {
        read_only: bits & 0x0001 != 0,
        hidden: bits & 0x0002 != 0,
        system: bits & 0x0004 != 0,
        directory: bits & FILE_NAME_DIRECTORY_FLAG != 0,
        archive: bits & 0x0020 != 0,
    }
}

/// Parsed NTFS attribute header (resident form).
///
/// Non-resident fields are decoded in a later pass once the runlist parser is in place.
#[allow(dead_code)] // fields consumed by attribute-scanning tasks
#[derive(Debug, Clone)]
pub(crate) struct AttrHeader {
    pub attr_type: u32,
    pub length: u32,
    pub non_resident: bool,
    pub name_length: u8,
    pub name_offset: u16,
    pub flags: u16,
    pub attr_id: u16,
    pub resident_value_length: u32,
    pub resident_value_offset: u16,
    pub data_runs_offset: u16,
    pub data_size: u64,
}

impl From<IndexEntry> for NtfsEntry {
    fn from(entry: IndexEntry) -> Self {
        NtfsEntry {
            name: entry.name,
            is_dir: entry.is_dir,
            size: entry.file_size,
            attributes: NtfsAttributes {
                directory: entry.is_dir,
                ..NtfsAttributes::default()
            },
            created: None,
            modified: None,
            accessed: None,
        }
    }
}

/// Validates a FILE record's Update Sequence Array and restores the original
/// sector-end bytes in-place.
#[allow(dead_code)] // called by MFT reader tasks (T-011)
pub(crate) fn apply_file_record_fixup(
    record: &mut [u8],
    sector_size: u64,
) -> Result<(), NtfsError> {
    if record.len() < 8 {
        return Err(NtfsError::InvalidFileRecord);
    }
    if &record[0..4] != b"FILE" {
        return Err(NtfsError::InvalidFileRecord);
    }

    let usa_offset = usize::from(u16::from_le_bytes([record[4], record[5]]));
    let usa_count = usize::from(u16::from_le_bytes([record[6], record[7]]));

    if usa_offset < 8 || usa_count < 2 || usa_offset + usa_count * 2 > record.len() {
        return Err(NtfsError::InvalidFileRecord);
    }

    let usa_n = u16::from_le_bytes([record[usa_offset], record[usa_offset + 1]]);
    let sector_sz = sector_size as usize;

    for sector_index in 1..usa_count {
        let sector_end = sector_index * sector_sz;
        if sector_end < 2 || sector_end > record.len() {
            continue;
        }
        let endpoint = u16::from_le_bytes([record[sector_end - 2], record[sector_end - 1]]);
        if endpoint != usa_n {
            return Err(NtfsError::FixupValidationFailed);
        }
        let fix_off = usa_offset + sector_index * 2;
        let fixup = u16::from_le_bytes([record[fix_off], record[fix_off + 1]]);
        let bytes = fixup.to_le_bytes();
        record[sector_end - 2] = bytes[0];
        record[sector_end - 1] = bytes[1];
    }
    Ok(())
}

/// Parses a single attribute header from `record` at `offset`.
///
/// Returns `Ok(None)` when the end-of-attributes marker (`0xFFFFFFFF`) is encountered.
#[allow(dead_code)] // called by attribute-scanning tasks (T-007+)
pub(crate) fn parse_attribute_header(
    record: &[u8],
    offset: usize,
) -> Result<Option<AttrHeader>, NtfsError> {
    let attr_type_end = offset.checked_add(4).ok_or(NtfsError::MalformedAttribute)?;
    if attr_type_end > record.len() {
        return Err(NtfsError::MalformedAttribute);
    }
    let attr_type = u32::from_le_bytes([
        record[offset],
        record[offset + 1],
        record[offset + 2],
        record[offset + 3],
    ]);
    if attr_type == 0xFFFF_FFFF {
        return Ok(None);
    }
    let common_header_end = offset
        .checked_add(16)
        .ok_or(NtfsError::MalformedAttribute)?;
    if common_header_end > record.len() {
        return Err(NtfsError::MalformedAttribute);
    }
    let length = u32::from_le_bytes([
        record[offset + 4],
        record[offset + 5],
        record[offset + 6],
        record[offset + 7],
    ]);
    let attr_end = offset
        .checked_add(length as usize)
        .ok_or(NtfsError::MalformedAttribute)?;
    if length < 16 || attr_end > record.len() {
        return Err(NtfsError::MalformedAttribute);
    }
    let non_resident = record[offset + 8];
    let name_length = record[offset + 9];
    let name_offset = u16::from_le_bytes([record[offset + 10], record[offset + 11]]);
    let flags = u16::from_le_bytes([record[offset + 12], record[offset + 13]]);
    let attr_id = u16::from_le_bytes([record[offset + 14], record[offset + 15]]);

    let (resident_value_length, resident_value_offset, data_runs_offset, data_size) =
        if non_resident == 0 {
            let resident_header_end = offset
                .checked_add(24)
                .ok_or(NtfsError::MalformedAttribute)?;
            if resident_header_end > record.len() {
                return Err(NtfsError::MalformedAttribute);
            }
            let val_len = u32::from_le_bytes([
                record[offset + 16],
                record[offset + 17],
                record[offset + 18],
                record[offset + 19],
            ]);
            let val_off = u16::from_le_bytes([record[offset + 20], record[offset + 21]]);
            let value_end = usize::from(val_off)
                .checked_add(val_len as usize)
                .ok_or(NtfsError::MalformedAttribute)?;
            if value_end > length as usize {
                return Err(NtfsError::MalformedAttribute);
            }
            (val_len, val_off, 0, u64::from(val_len))
        } else {
            let nonresident_header_end = offset
                .checked_add(64)
                .ok_or(NtfsError::MalformedAttribute)?;
            if nonresident_header_end > record.len() || length < 64 {
                return Err(NtfsError::MalformedAttribute);
            }
            let runs_off = u16::from_le_bytes([record[offset + 32], record[offset + 33]]);
            if usize::from(runs_off) > length as usize {
                return Err(NtfsError::MalformedAttribute);
            }
            let valid_data_length = u64::from_le_bytes([
                record[offset + 56],
                record[offset + 57],
                record[offset + 58],
                record[offset + 59],
                record[offset + 60],
                record[offset + 61],
                record[offset + 62],
                record[offset + 63],
            ]);
            (0, 0, runs_off, valid_data_length)
        };

    Ok(Some(AttrHeader {
        attr_type,
        length,
        non_resident: non_resident != 0,
        name_length,
        name_offset,
        flags,
        attr_id,
        resident_value_length,
        resident_value_offset,
        data_runs_offset,
        data_size,
    }))
}

// ── runlist byte-parsing helpers ──────────────────────────────────────────────

/// Zero-extends up to 8 little-endian bytes into a `u64`.
#[allow(dead_code)]
fn read_le_uint(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        val |= (b as u64) << (i * 8);
    }
    val
}

/// Sign-extends little-endian bytes into an `i64`.
///
/// The MSB of the last byte determines whether upper bits are filled with 1s.
#[allow(dead_code)]
fn read_le_int_signed(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    let mut val = 0i64;
    for (i, &b) in bytes.iter().enumerate() {
        val |= (b as i64) << (i * 8);
    }
    let shift = bytes.len().saturating_mul(8);
    if bytes[bytes.len() - 1] & 0x80 != 0 && shift < 64 {
        val |= !0i64 << shift;
    }
    val
}

// ── public(crate) parsers and readers ─────────────────────────────────────────

/// Parses an NTFS data runlist into `(absolute_lcn, run_length_in_clusters)` pairs.
///
/// Signed LCN deltas accumulate; a negative absolute LCN is rejected.
/// Sparse runs (`off_size == 0`) are not supported.
#[allow(dead_code)] // called by non-resident $DATA path (T-011+)
pub(crate) fn parse_runlist(bytes: &[u8]) -> Result<Vec<(u64, u64)>, NtfsError> {
    let mut runs = Vec::new();
    let mut current_lcn: i64 = 0;
    let mut i = 0;
    loop {
        if i >= bytes.len() {
            return Err(NtfsError::MalformedRunlist);
        }
        let header = bytes[i];
        i += 1;
        if header == 0x00 {
            break;
        }
        let len_size = (header & 0x0F) as usize;
        let off_size = ((header >> 4) & 0x0F) as usize;
        if len_size == 0 {
            return Err(NtfsError::MalformedRunlist);
        }
        if i + len_size > bytes.len() {
            return Err(NtfsError::MalformedRunlist);
        }
        let run_len = read_le_uint(&bytes[i..i + len_size]);
        i += len_size;
        if off_size == 0 {
            return Err(NtfsError::UnsupportedCompressedData);
        }
        if i + off_size > bytes.len() {
            return Err(NtfsError::MalformedRunlist);
        }
        let lcn_delta = read_le_int_signed(&bytes[i..i + off_size]);
        i += off_size;
        current_lcn = current_lcn
            .checked_add(lcn_delta)
            .ok_or(NtfsError::MalformedRunlist)?;
        if current_lcn < 0 {
            return Err(NtfsError::InvalidClusterNumber {
                cluster: current_lcn as u64,
            });
        }
        runs.push((current_lcn as u64, run_len));
    }
    Ok(runs)
}

/// Reads the value bytes of a resident attribute.
///
/// `record` must begin at the attribute header (byte 0 = type field).
/// `hdr.resident_value_offset` is relative to that same byte 0.
#[allow(dead_code)] // called by attribute reader (T-011+)
pub(crate) fn read_resident_data(record: &[u8], hdr: &AttrHeader) -> Result<Vec<u8>, NtfsError> {
    let value_start = hdr.resident_value_offset as usize;
    let value_end = value_start
        .checked_add(hdr.resident_value_length as usize)
        .ok_or(NtfsError::OutOfBoundsRead)?;
    if value_end > record.len() {
        return Err(NtfsError::OutOfBoundsRead);
    }
    Ok(record[value_start..value_end].to_vec())
}

/// Reads the full logical data of a non-resident attribute by walking its run list.
///
/// `data_size` is the valid-data-length; the result is trimmed to that length.
#[allow(dead_code)] // called by attribute reader (T-011+)
pub(crate) fn read_nonresident_data(
    reader: &dyn BlockReader,
    runs: &[(u64, u64)],
    cluster_size: u64,
    data_size: u64,
) -> Result<Vec<u8>, NtfsError> {
    let mut result = Vec::new();
    for &(lcn, run_len) in runs {
        let byte_offset = lcn
            .checked_mul(cluster_size)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        let read_len = run_len
            .checked_mul(cluster_size)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        let read_len_usize = usize::try_from(read_len).map_err(|_| NtfsError::OutOfBoundsRead)?;
        let end = byte_offset
            .checked_add(read_len)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        if end > reader.len() {
            return Err(NtfsError::OutOfBoundsRead);
        }
        let mut buf = vec![0u8; read_len_usize];
        reader.read_at(byte_offset, &mut buf)?;
        result.extend_from_slice(&buf);
    }
    let data_size_usize = usize::try_from(data_size).map_err(|_| NtfsError::OutOfBoundsRead)?;
    if result.len() > data_size_usize {
        result.truncate(data_size_usize);
    }
    Ok(result)
}

/// Streams `data_size` bytes from a non-resident attribute run list to `writer`.
///
/// Each run is read in chunks of at most [`EXTRACTION_CHUNK_SIZE`] bytes so that
/// peak RAM stays constant regardless of file size. The total bytes written is
/// trimmed to `data_size` (the valid-data-length), matching the trim performed
/// by [`read_nonresident_data`].
fn stream_nonresident_data<W: std::io::Write>(
    reader: &dyn BlockReader,
    runs: &[(u64, u64)],
    cluster_size: u64,
    data_size: u64,
    writer: &mut W,
) -> Result<u64, NtfsError> {
    let cluster_size_usize =
        usize::try_from(cluster_size).map_err(|_| NtfsError::OutOfBoundsRead)?;
    let buf_size = cluster_size_usize.min(EXTRACTION_CHUNK_SIZE);
    let mut buf = vec![0u8; buf_size];
    let mut remaining = usize::try_from(data_size).map_err(|_| NtfsError::OutOfBoundsRead)?;
    let mut written: u64 = 0;

    for &(lcn, run_len) in runs {
        if remaining == 0 {
            break;
        }
        let byte_offset = lcn
            .checked_mul(cluster_size)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        let run_bytes = run_len
            .checked_mul(cluster_size)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        let run_end = byte_offset
            .checked_add(run_bytes)
            .ok_or(NtfsError::OutOfBoundsRead)?;
        if run_end > reader.len() {
            return Err(NtfsError::OutOfBoundsRead);
        }
        let run_bytes_usize = usize::try_from(run_bytes).map_err(|_| NtfsError::OutOfBoundsRead)?;
        let to_emit = remaining.min(run_bytes_usize);
        let mut run_pos: usize = 0;
        while run_pos < to_emit {
            let chunk = (to_emit - run_pos).min(buf_size);
            reader.read_at(byte_offset + run_pos as u64, &mut buf[..chunk])?;
            writer
                .write_all(&buf[..chunk])
                .map_err(NtfsError::WriteError)?;
            written += chunk as u64;
            run_pos += chunk;
        }
        remaining -= to_emit;
    }
    Ok(written)
}

/// Returns an error when `hdr` describes a named `$DATA` alternate data stream.
#[allow(dead_code)] // called by file-read path (T-011+)
pub(crate) fn check_unnamed_data(hdr: &AttrHeader) -> Result<(), NtfsError> {
    if hdr.attr_type == 0x80 && hdr.name_length != 0 {
        Err(NtfsError::UnsupportedNamedDataStream)
    } else {
        Ok(())
    }
}

/// Returns an error when `hdr` carries unsupported flags (compressed / encrypted / sparse).
#[allow(dead_code)] // called by file-read path (T-011+)
pub(crate) fn check_unsupported_flags(hdr: &AttrHeader) -> Result<(), NtfsError> {
    const COMPRESSED: u16 = 0x0001;
    const ENCRYPTED: u16 = 0x4000;
    const SPARSE: u16 = 0x8000;
    if hdr.flags & (COMPRESSED | ENCRYPTED | SPARSE) != 0 {
        Err(NtfsError::UnsupportedCompressedData)
    } else {
        Ok(())
    }
}

// ── index parsing ─────────────────────────────────────────────────────────────

/// A directory entry parsed from an NTFS `$FILE_NAME` index.
#[allow(dead_code)] // fields read by index tests and later list_dir (T-011+)
#[derive(Debug, Clone)]
pub(crate) struct IndexEntry {
    pub mft_record: u64,
    pub file_size: u64,
    pub is_dir: bool,
    pub namespace: u8,
    pub name: String,
}

/// Decodes a slice of UTF-16 code units into a `String`, resolving surrogate pairs.
///
/// Returns [`NtfsError::InvalidUtf16`] for unpaired or mismatched surrogates.
#[allow(dead_code)]
pub(crate) fn decode_utf16le(units: &[u16]) -> Result<String, NtfsError> {
    char::decode_utf16(units.iter().cloned())
        .map(|r| r.map_err(|_| NtfsError::InvalidUtf16))
        .collect::<Result<String, _>>()
}

/// Parses directory entries from a `$INDEX_ROOT` attribute value.
///
/// Skips the last-entry terminator; returns only real file/directory records.
#[allow(dead_code)]
pub(crate) fn parse_index_root_entries(attr_value: &[u8]) -> Result<Vec<IndexEntry>, NtfsError> {
    if attr_value.len() < 32 {
        return Err(NtfsError::InvalidDirectoryIndex);
    }
    parse_index_entries_from_node(attr_value, 16)
}

fn parse_index_entries_from_node(
    bytes: &[u8],
    node_header_offset: usize,
) -> Result<Vec<IndexEntry>, NtfsError> {
    if node_header_offset + 16 > bytes.len() {
        return Err(NtfsError::InvalidDirectoryIndex);
    }
    let first_entry_offset = u32_le(bytes, node_header_offset) as usize;
    let total_entries_size = u32_le(bytes, node_header_offset + 4) as usize;
    let entries_start = node_header_offset
        .checked_add(first_entry_offset)
        .ok_or(NtfsError::InvalidDirectoryIndex)?;
    let entries_end = node_header_offset
        .checked_add(total_entries_size)
        .ok_or(NtfsError::InvalidDirectoryIndex)?;
    if entries_start > bytes.len() || entries_end > bytes.len() || entries_start > entries_end {
        return Err(NtfsError::InvalidDirectoryIndex);
    }

    let mut pos = entries_start;
    let mut entries = Vec::new();
    loop {
        if pos + 16 > entries_end {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        let entry_length = u16_le(bytes, pos + 8) as usize;
        let flags = u16_le(bytes, pos + 12);
        if entry_length < 16 {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        if flags & 0x02 != 0 {
            break;
        }
        let indexed_data_length = u16_le(bytes, pos + 10) as usize;
        if indexed_data_length < 66 {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        if pos + 16 + indexed_data_length > entries_end {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        let file_ref_raw = u64_le(bytes, pos);
        let mft_record = file_ref_raw & 0x0000_FFFF_FFFF_FFFF;
        let data = &bytes[pos + 16..pos + 16 + indexed_data_length];
        let data_size = u64_le(data, 48);
        let is_dir = (u32_le(data, 56) as u32 & FILE_NAME_DIRECTORY_FLAG) != 0;
        let filename_length = data[64] as usize;
        let namespace = data[65];
        if 66 + filename_length * 2 > data.len() {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        let units: Vec<u16> = (0..filename_length)
            .map(|i| u16::from_le_bytes([data[66 + i * 2], data[66 + i * 2 + 1]]))
            .collect();
        let name = decode_utf16le(&units)?;
        entries.push(IndexEntry {
            mft_record,
            file_size: data_size,
            is_dir,
            namespace,
            name,
        });
        pos = pos
            .checked_add(entry_length)
            .ok_or(NtfsError::InvalidDirectoryIndex)?;
        if pos > entries_end {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
    }
    Ok(entries)
}

fn parse_index_allocation_entries(
    bytes: &[u8],
    index_buffer_size: u64,
    sector_size: u64,
) -> Result<Vec<IndexEntry>, NtfsError> {
    let buffer_size =
        usize::try_from(index_buffer_size).map_err(|_| NtfsError::InvalidDirectoryIndex)?;
    if buffer_size == 0 {
        return Err(NtfsError::InvalidDirectoryIndex);
    }

    let mut entries = Vec::new();
    for chunk in bytes.chunks(buffer_size) {
        if chunk.len() < 4 || chunk[0..4].iter().all(|byte| *byte == 0) {
            continue;
        }
        if chunk.len() < buffer_size {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        let mut buffer = chunk.to_vec();
        apply_index_buffer_fixup(&mut buffer, sector_size)?;
        entries.extend(parse_index_entries_from_node(&buffer, 24)?);
    }
    Ok(entries)
}

fn apply_index_buffer_fixup(buffer: &mut [u8], sector_size: u64) -> Result<(), NtfsError> {
    if buffer.len() < 24 || &buffer[0..4] != b"INDX" {
        return Err(NtfsError::InvalidDirectoryIndex);
    }
    let usa_offset = usize::from(u16::from_le_bytes([buffer[4], buffer[5]]));
    let usa_count = usize::from(u16::from_le_bytes([buffer[6], buffer[7]]));
    if usa_offset < 8 || usa_count < 2 || usa_offset + usa_count * 2 > buffer.len() {
        return Err(NtfsError::InvalidDirectoryIndex);
    }
    let sector_size = usize::try_from(sector_size).map_err(|_| NtfsError::InvalidDirectoryIndex)?;
    let usa_n = u16::from_le_bytes([buffer[usa_offset], buffer[usa_offset + 1]]);
    for sector_index in 1..usa_count {
        let sector_end = sector_index
            .checked_mul(sector_size)
            .ok_or(NtfsError::InvalidDirectoryIndex)?;
        if sector_end < 2 || sector_end > buffer.len() {
            return Err(NtfsError::InvalidDirectoryIndex);
        }
        let endpoint = u16::from_le_bytes([buffer[sector_end - 2], buffer[sector_end - 1]]);
        if endpoint != usa_n {
            return Err(NtfsError::FixupValidationFailed);
        }
        let fixup_offset = usa_offset + sector_index * 2;
        let fixup = u16::from_le_bytes([buffer[fixup_offset], buffer[fixup_offset + 1]]);
        buffer[sector_end - 2..sector_end].copy_from_slice(&fixup.to_le_bytes());
    }
    Ok(())
}

/// Selects the best filename from a `(namespace, name)` list.
///
/// Preference: Win32AndDos (3) = Win32 (1) = POSIX (0) > DOS (2).
/// Returns `None` for an empty list.
#[allow(dead_code)]
pub(crate) fn best_filename(names: &[(u8, String)]) -> Option<&str> {
    names
        .iter()
        .find(|(ns, _)| *ns != 2)
        .or_else(|| names.first())
        .map(|(_, name)| name.as_str())
}

/// Decodes the byte count for a file-record or index-buffer size field.
///
/// NTFS encodes `ClustersPerFileRecordSegment` and `ClustersPerIndexBuffer`
/// as a signed byte: positive means `value × cluster_size`; negative means `2^abs(value)`.
/// Returns an error for zero or overflow.
pub(crate) fn decode_sized_field(raw: i8, cluster_size: u64) -> Result<u64, NtfsError> {
    if raw == 0 {
        return Err(NtfsError::InvalidBootSector {
            reason: "zero clusters-per-record field",
        });
    }
    if raw > 0 {
        let n = raw as u64;
        n.checked_mul(cluster_size)
            .ok_or(NtfsError::InvalidBootSector {
                reason: "file record size overflow",
            })
    } else {
        let exponent = raw.unsigned_abs();
        if exponent > 63 {
            return Err(NtfsError::InvalidBootSector {
                reason: "file record size exponent too large",
            });
        }
        Ok(1u64 << exponent)
    }
}

// ── byte-level helpers (only safe after a 512-byte length check) ─────────────

fn u16_le(buf: &[u8], off: usize) -> u64 {
    u16::from_le_bytes([buf[off], buf[off + 1]]) as u64
}

fn u32_le(buf: &[u8], off: usize) -> u64 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as u64
}

fn u64_le(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ])
}

// ── boot sector parser ────────────────────────────────────────────────────────

fn parse_boot_sector(buf: &[u8], _reader_len: u64) -> Result<NtfsGeometry, NtfsError> {
    if buf.len() < 512 {
        return Err(NtfsError::InvalidBootSector {
            reason: "boot sector too short",
        });
    }

    if &buf[3..11] != b"NTFS    " {
        return Err(NtfsError::InvalidBootSector {
            reason: "wrong OEM ID, not NTFS",
        });
    }

    let bytes_per_sector = u16_le(buf, 11);
    if !(512..=4096).contains(&bytes_per_sector) || !bytes_per_sector.is_power_of_two() {
        return Err(NtfsError::InvalidBootSector {
            reason: "invalid BytesPerSector",
        });
    }

    let sectors_per_cluster = buf[13] as u64;
    if !sectors_per_cluster.is_power_of_two() {
        return Err(NtfsError::InvalidBootSector {
            reason: "invalid SectorsPerCluster",
        });
    }

    let cluster_size =
        bytes_per_sector
            .checked_mul(sectors_per_cluster)
            .ok_or(NtfsError::InvalidBootSector {
                reason: "cluster size overflow",
            })?;

    let total_sectors = u64_le(buf, 40);
    if total_sectors == 0 {
        return Err(NtfsError::InvalidBootSector {
            reason: "zero TotalSectors",
        });
    }

    let mft_lcn = u64_le(buf, 48);
    let mft_offset = mft_lcn
        .checked_mul(cluster_size)
        .ok_or(NtfsError::InvalidBootSector {
            reason: "MFT LCN overflow",
        })?;
    if let Some(vol) = total_sectors.checked_mul(bytes_per_sector) {
        if mft_offset >= vol {
            return Err(NtfsError::InvalidBootSector {
                reason: "MFT LCN past end of volume",
            });
        }
    }

    let mft_mirror_lcn = u64_le(buf, 56);
    let mft_mirror_offset =
        mft_mirror_lcn
            .checked_mul(cluster_size)
            .ok_or(NtfsError::InvalidBootSector {
                reason: "MFT mirror LCN overflow",
            })?;

    let file_record_raw = buf[64] as i8;
    let file_record_size = decode_sized_field(file_record_raw, cluster_size)?;
    if file_record_size < 512 || !file_record_size.is_power_of_two() {
        return Err(NtfsError::InvalidBootSector {
            reason: "invalid file record size",
        });
    }

    let index_buf_raw = buf[68] as i8;
    let index_buffer_size = decode_sized_field(index_buf_raw, cluster_size)?;

    let volume_serial = u64_le(buf, 72);

    Ok(NtfsGeometry {
        bytes_per_sector,
        sectors_per_cluster,
        cluster_size,
        total_sectors,
        mft_offset,
        mft_mirror_offset,
        file_record_size,
        index_buffer_size,
        volume_serial,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cryptovol_core::{BlockReader, CryptovolError};

    // ── in-memory block reader for unit tests ─────────────────────────────────

    struct MemReader(Vec<u8>);

    impl BlockReader for MemReader {
        fn len(&self) -> u64 {
            self.0.len() as u64
        }

        fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
            let Ok(start) = usize::try_from(offset) else {
                return Err(CryptovolError::OutOfBounds {
                    offset,
                    length: buf.len(),
                    file_len: self.0.len() as u64,
                });
            };
            let end = start + buf.len();
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

    // ── boot-sector fixture ───────────────────────────────────────────────────

    /// Returns a minimal, valid NTFS boot sector (512 bytes).
    ///
    /// Geometry:
    ///   BytesPerSector = 512, SectorsPerCluster = 8  → cluster_size = 4 096
    ///   TotalSectors = 127                            → volume ≈ 65 024 bytes
    ///   MftLcn = 4                                   → MFT at byte 16 384 (within volume)
    ///   MftMirrorLcn = 64
    ///   ClustersPerFileRecordSegment = -10            → file_record_size = 1 024
    ///   ClustersPerIndexBuffer = -11                  → index_buffer_size = 2 048
    fn make_ntfs_boot_sector() -> [u8; 512] {
        let mut buf = [0u8; 512];
        buf[0..3].copy_from_slice(&[0xEB, 0x52, 0x90]); // JMP + NOP
        buf[3..11].copy_from_slice(b"NTFS    "); // OEM ID
        buf[11..13].copy_from_slice(&512u16.to_le_bytes()); // BytesPerSector
        buf[13] = 8; // SectorsPerCluster
        buf[21] = 0xF8; // MediaDescriptor
        buf[40..48].copy_from_slice(&127u64.to_le_bytes()); // TotalSectors
        buf[48..56].copy_from_slice(&4u64.to_le_bytes()); // MftLcn
        buf[56..64].copy_from_slice(&64u64.to_le_bytes()); // MftMirrorLcn
        buf[64] = (-10i8) as u8; // ClustersPerFileRecordSegment
        buf[68] = (-11i8) as u8; // ClustersPerIndexBuffer
        buf[72..80].copy_from_slice(&0x1234_5678_90AB_CDEFu64.to_le_bytes()); // VolumeSerialNumber
        buf[510] = 0x55; // Boot signature
        buf[511] = 0xAA;
        buf
    }

    // ── boot-sector tests (AC-001, AC-002) ───────────────────────────────────

    #[test]
    fn boot_sector_valid_parses_correctly() {
        let boot = make_ntfs_boot_sector();
        let result = NtfsFileSystem::open(MemReader(boot.to_vec()));
        assert!(
            result.is_ok(),
            "valid NTFS boot sector should open successfully, got: {result:?}"
        );
    }

    #[test]
    fn boot_sector_wrong_oem_id_rejected() {
        let mut boot = make_ntfs_boot_sector();
        boot[3..11].copy_from_slice(b"FAT32   ");
        let result = NtfsFileSystem::open(MemReader(boot.to_vec()));
        let Err(NtfsError::InvalidBootSector { reason }) = result else {
            panic!("expected InvalidBootSector, got: {result:?}");
        };
        assert!(
            reason.contains("OEM") || reason.contains("NTFS"),
            "expected OEM-related error reason, got: {reason}"
        );
    }

    #[test]
    fn boot_sector_zero_bytes_per_sector_rejected() {
        let mut boot = make_ntfs_boot_sector();
        boot[11..13].copy_from_slice(&0u16.to_le_bytes());
        let result = NtfsFileSystem::open(MemReader(boot.to_vec()));
        let Err(NtfsError::InvalidBootSector { reason }) = result else {
            panic!("expected InvalidBootSector, got: {result:?}");
        };
        assert!(
            reason.to_lowercase().contains("sector"),
            "expected sector-size error, got: {reason}"
        );
    }

    #[test]
    fn boot_sector_mft_lcn_past_end_rejected() {
        let mut boot = make_ntfs_boot_sector();
        boot[48..56].copy_from_slice(&u64::MAX.to_le_bytes()); // MftLcn = u64::MAX
        let result = NtfsFileSystem::open(MemReader(boot.to_vec()));
        let Err(NtfsError::InvalidBootSector { reason }) = result else {
            panic!("expected InvalidBootSector, got: {result:?}");
        };
        assert!(
            reason.to_lowercase().contains("mft") || reason.to_lowercase().contains("overflow"),
            "expected MFT location error, got: {reason}"
        );
    }

    // ── file-record / index-buffer size decoding (AC-003) ────────────────────

    #[test]
    fn file_record_size_positive_value() {
        // ClustersPerFileRecordSegment = 2, cluster_size = 4096 → 2 * 4096 = 8192
        let result = decode_sized_field(2, 4096);
        assert_eq!(result.unwrap(), 8192);
    }

    #[test]
    fn file_record_size_negative_value() {
        // ClustersPerFileRecordSegment = -10 → 2^10 = 1024
        let result = decode_sized_field(-10, 4096);
        assert_eq!(result.unwrap(), 1024);
    }

    #[test]
    fn file_record_size_zero_is_invalid() {
        let result = decode_sized_field(0, 4096);
        assert!(
            matches!(result, Err(NtfsError::InvalidBootSector { .. })),
            "zero sized-field should be rejected, got: {result:?}"
        );
    }

    // ── runlist / resident / non-resident / flag tests (AC-005, AC-006, AC-007) ──

    #[test]
    fn runlist_single_run() {
        // 0x11 = len_size 1, off_size 1; length 4 clusters; LCN delta +2; 0x00 end
        let result = parse_runlist(&[0x11, 0x04, 0x02, 0x00]);
        assert_eq!(result.unwrap(), vec![(2, 4)]);
    }

    #[test]
    fn runlist_multi_run_signed_delta() {
        // run1: length=3, delta=+5 → abs=5
        // run2: length=2, delta=0xFF=-1 (i8) → abs=4
        let result = parse_runlist(&[0x11, 0x03, 0x05, 0x11, 0x02, 0xFF, 0x00]);
        assert_eq!(result.unwrap(), vec![(5, 3), (4, 2)]);
    }

    #[test]
    fn runlist_zero_run_length() {
        // 0x10 = len_size=0, off_size=1 → sparse run; not supported
        let result = parse_runlist(&[0x10, 0x02, 0x00]);
        assert!(
            matches!(
                result,
                Err(NtfsError::UnsupportedCompressedData) | Err(NtfsError::MalformedRunlist)
            ),
            "sparse run should be rejected, got: {result:?}"
        );
    }

    #[test]
    fn runlist_truncated_rejected() {
        // Header 0x11 promises 1 length byte + 1 offset byte, but only 1 byte follows
        let result = parse_runlist(&[0x11, 0x03]);
        assert!(
            matches!(result, Err(NtfsError::MalformedRunlist)),
            "truncated runlist should return MalformedRunlist, got: {result:?}"
        );
    }

    #[test]
    fn runlist_negative_absolute_lcn_rejected() {
        // run1: length=3, delta=+5 → abs=5
        // run2: length=2, delta=0xF6=-10 (i8) → abs=5+(-10)=-5 → invalid
        let result = parse_runlist(&[0x11, 0x03, 0x05, 0x11, 0x02, 0xF6, 0x00]);
        assert!(
            matches!(result, Err(NtfsError::InvalidClusterNumber { .. })),
            "negative absolute LCN should return InvalidClusterNumber, got: {result:?}"
        );
    }

    #[test]
    fn runlist_end_marker_only() {
        let result = parse_runlist(&[0x00]);
        assert_eq!(
            result.unwrap(),
            vec![],
            "end marker only should produce empty runlist"
        );
        // Distinguish real impl from stub: a non-empty runlist must not produce empty output
        let result2 = parse_runlist(&[0x11, 0x04, 0x02, 0x00]);
        assert!(
            !result2.as_ref().map(|v| v.is_empty()).unwrap_or(false),
            "single-run runlist should not return empty vec, got: {result2:?}"
        );
    }

    #[test]
    fn read_resident_data_returns_exact_bytes() {
        // Pass the attribute slice directly (byte 0 = attribute type field).
        let mut attr = vec![0u8; 32];
        attr[24..29].copy_from_slice(b"hello"); // value at offset 24 within attr
        let hdr = AttrHeader {
            attr_type: 0x80,
            length: 32,
            non_resident: false,
            name_length: 0,
            name_offset: 0x18,
            flags: 0,
            attr_id: 0,
            resident_value_length: 5,
            resident_value_offset: 24,
            data_runs_offset: 0,
            data_size: 5,
        };
        let result = read_resident_data(&attr, &hdr);
        assert_eq!(result.unwrap(), b"hello".to_vec());
    }

    #[test]
    fn read_nonresident_data_single_run() {
        // Cluster size 512; run at absolute LCN 4 → byte offset 4*512 = 2048.
        let mut data = vec![0u8; 2560];
        data[2048..2053].copy_from_slice(b"world");
        let reader = MemReader(data);
        let result = read_nonresident_data(&reader, &[(4u64, 1u64)], 512, 5);
        assert_eq!(result.unwrap(), b"world".to_vec());
    }

    #[test]
    fn read_nonresident_data_trims_to_data_size() {
        let mut data = vec![0u8; 2560];
        data[2048..2053].copy_from_slice(b"hello");
        let reader = MemReader(data);
        let result = read_nonresident_data(&reader, &[(4u64, 1u64)], 512, 3);
        assert_eq!(result.unwrap(), b"hel".to_vec());
    }

    #[test]
    fn named_data_stream_in_attribute_is_rejected() {
        let hdr = AttrHeader {
            attr_type: 0x80,
            length: 40,
            non_resident: false,
            name_length: 3, // named → alternate data stream
            name_offset: 0x18,
            flags: 0,
            attr_id: 0,
            resident_value_length: 0,
            resident_value_offset: 0,
            data_runs_offset: 0,
            data_size: 0,
        };
        let result = check_unnamed_data(&hdr);
        assert!(
            matches!(result, Err(NtfsError::UnsupportedNamedDataStream)),
            "named $DATA should return UnsupportedNamedDataStream, got: {result:?}"
        );
    }

    #[test]
    fn compressed_attribute_rejected() {
        let hdr = AttrHeader {
            attr_type: 0x80,
            length: 40,
            non_resident: true,
            name_length: 0,
            name_offset: 0,
            flags: 0x0001, // COMPRESSED bit
            attr_id: 0,
            resident_value_length: 0,
            resident_value_offset: 0,
            data_runs_offset: 0,
            data_size: 0,
        };
        let result = check_unsupported_flags(&hdr);
        assert!(
            matches!(result, Err(NtfsError::UnsupportedCompressedData)),
            "compressed attribute should return UnsupportedCompressedData, got: {result:?}"
        );
    }

    // ── NtfsTimestamp (already implemented in scaffold) ──────────────────────

    #[test]
    fn ntfs_timestamp_from_valid_ticks() {
        // 132 000 000 000 000 000 ticks ≈ 2019 in Windows FILETIME
        let ts = NtfsTimestamp::from_windows_ticks(132_000_000_000_000_000);
        let ts = ts.expect("should produce a timestamp");
        assert!(
            ts.unix_seconds > 0,
            "expected positive unix_seconds, got {}",
            ts.unix_seconds
        );
    }

    #[test]
    fn ntfs_timestamp_zero_ticks_returns_none() {
        assert!(NtfsTimestamp::from_windows_ticks(0).is_none());
    }

    // ── FILE record fixup + attribute header tests (AC-004) ──────────────────

    /// Builds a minimal valid NTFS FILE record of `record_size` bytes.
    ///
    /// The record has two 512-byte sectors, so `record_size` must be >= 1024.
    /// The USA sequence number is 0x0001; the fixup values (what to restore) are
    /// 0x0000.  Before the real fixup is applied the sector endpoints hold 0x0001.
    fn make_file_record(record_size: usize) -> Vec<u8> {
        let mut r = vec![0u8; record_size];
        r[0..4].copy_from_slice(b"FILE"); // signature
        r[4..6].copy_from_slice(&48u16.to_le_bytes()); // USA offset
        r[6..8].copy_from_slice(&3u16.to_le_bytes()); // USA count (1 seq + 2 fixup)
        r[16..18].copy_from_slice(&1u16.to_le_bytes()); // sequence number
        r[18..20].copy_from_slice(&1u16.to_le_bytes()); // link count
        r[20..22].copy_from_slice(&56u16.to_le_bytes()); // first attribute offset
        r[22..24].copy_from_slice(&1u16.to_le_bytes()); // flags: in-use
        r[24..28].copy_from_slice(&(record_size as u32).to_le_bytes()); // real size
        r[28..32].copy_from_slice(&(record_size as u32).to_le_bytes()); // alloc size
        r[40..42].copy_from_slice(&1u16.to_le_bytes()); // next attr id
                                                        // USA block at offset 48
        r[48..50].copy_from_slice(&0x0001u16.to_le_bytes()); // sequence number
        r[50..52].copy_from_slice(&0x0000u16.to_le_bytes()); // fixup sector 1
        r[52..54].copy_from_slice(&0x0000u16.to_le_bytes()); // fixup sector 2
                                                             // End-of-attributes marker
        r[56..60].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        // Sector endpoints set to USA_n (real fixup would restore them to 0x0000)
        r[510] = 0x01;
        r[511] = 0x00;
        if record_size >= 1024 {
            r[1022] = 0x01;
            r[1023] = 0x00;
        }
        r
    }

    #[test]
    fn file_record_fixup_valid_succeeds() {
        let mut record = make_file_record(1024);
        let result = apply_file_record_fixup(&mut record, 512);
        assert!(
            result.is_ok(),
            "valid fixup should succeed, got: {result:?}"
        );
        assert_eq!(
            &record[510..512],
            &[0x00, 0x00],
            "fixup should restore sector 1 tail bytes to the saved fixup value [0x00, 0x00]"
        );
    }

    #[test]
    fn file_record_fixup_wrong_sequence_fails() {
        let mut record = make_file_record(1024);
        record[510] = 0xAB; // corrupt endpoint: does not match USA_n 0x0001
        record[511] = 0xCD;
        let result = apply_file_record_fixup(&mut record, 512);
        assert!(
            matches!(result, Err(NtfsError::FixupValidationFailed)),
            "mismatched sector endpoint should return FixupValidationFailed, got: {result:?}"
        );
    }

    #[test]
    fn file_record_wrong_signature_fails() {
        let mut record = make_file_record(1024);
        record[0..4].copy_from_slice(b"BAAD");
        let result = apply_file_record_fixup(&mut record, 512);
        assert!(
            matches!(result, Err(NtfsError::InvalidFileRecord)),
            "wrong signature should return InvalidFileRecord, got: {result:?}"
        );
    }

    #[test]
    fn file_record_too_short_fails() {
        let mut tiny = vec![0u8; 12];
        let result = apply_file_record_fixup(&mut tiny, 512);
        assert!(
            matches!(result, Err(NtfsError::InvalidFileRecord)),
            "12-byte record should return InvalidFileRecord, got: {result:?}"
        );
    }

    #[test]
    fn attribute_header_end_marker_returns_none() {
        let record = make_file_record(1024);
        // Offset 56 holds 0xFFFFFFFF (end-of-attributes marker).
        let result = parse_attribute_header(&record, 56);
        assert!(
            matches!(result, Ok(None)),
            "end-of-attributes marker should return Ok(None), got: {result:?}"
        );
        // Verify the stub distinguishes end-marker from a real attribute type.
        let mut record2 = make_file_record(1024);
        record2[56..60].copy_from_slice(&0x10u32.to_le_bytes()); // $STANDARD_INFORMATION
        let result2 = parse_attribute_header(&record2, 56);
        assert!(
            !matches!(result2, Ok(None)),
            "a resident attribute should not return Ok(None), got: {result2:?}"
        );
    }

    #[test]
    fn attribute_header_resident_returns_correct_type() {
        let mut record = make_file_record(1024);
        // Write a minimal $STANDARD_INFORMATION resident attribute at offset 56.
        record[56..60].copy_from_slice(&0x10u32.to_le_bytes()); // type
        record[60..64].copy_from_slice(&72u32.to_le_bytes()); // total length
        record[64] = 0x00; // non-resident = false
        record[65] = 0x00; // name length = 0
        record[66..68].copy_from_slice(&0x18u16.to_le_bytes()); // name offset = 24
        record[68..70].copy_from_slice(&0x0000u16.to_le_bytes()); // flags
        record[70..72].copy_from_slice(&0x0000u16.to_le_bytes()); // attr id
        record[72..76].copy_from_slice(&48u32.to_le_bytes()); // value length = 48
        record[76..78].copy_from_slice(&0x18u16.to_le_bytes()); // value offset = 24
                                                                // bytes 78-79: indexed=0, reserved=0 (already zero)
                                                                // bytes 80-127: 48 bytes of value data (already zero)
                                                                // End-of-attributes marker immediately after this attribute
        record[128..132].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let result = parse_attribute_header(&record, 56);
        let Ok(Some(hdr)) = result else {
            panic!("expected Ok(Some(_)), got: {result:?}");
        };
        assert_eq!(
            hdr.attr_type, 0x10,
            "expected $STANDARD_INFORMATION type 0x10"
        );
        assert!(!hdr.non_resident, "expected resident attribute");
        assert_eq!(hdr.resident_value_length, 48, "expected value length 48");
        assert_eq!(hdr.length, 72, "expected attribute total length 72");
    }

    // ── $INDEX_ROOT / UTF-16LE / namespace helpers ─────────────────────────────

    /// Builds a complete `$INDEX_ROOT` attribute value for testing.
    ///
    /// `entries`: `(name, mft_record_number, is_dir, namespace)`.
    /// Appends a last-entry terminator after all real entries.
    fn make_index_root_value(entries: &[(&str, u64, bool, u8)]) -> Vec<u8> {
        let mut entry_bytes: Vec<u8> = Vec::new();
        for &(name, mft_num, is_dir, namespace) in entries {
            let utf16: Vec<u16> = name.encode_utf16().collect();
            let filename_len = utf16.len() as u8;
            let indexed_data_len = 82 + utf16.len() * 2;
            let entry_len_raw = 16 + indexed_data_len;
            let entry_len = (entry_len_raw + 7) & !7_usize;

            let mut entry = vec![0u8; entry_len];
            let file_ref: u64 = (1u64 << 48) | mft_num;
            entry[0..8].copy_from_slice(&file_ref.to_le_bytes());
            entry[8..10].copy_from_slice(&(entry_len as u16).to_le_bytes());
            entry[10..12].copy_from_slice(&(indexed_data_len as u16).to_le_bytes());
            // flags = 0x0000 (not last, no sub-node)
            // Indexed data at entry[16..]:
            //   [16..24] parent file reference
            //   [24..32] created time
            //   [32..40] modified time
            //   [40..48] mft_modified time
            //   [48..56] accessed time
            //   [56..64] allocated size
            //   [64..72] data size = 512
            //   [72..76] file attributes (bit 0x1000_0000 = dir; see FILE_NAME_DIRECTORY_FLAG)
            //   [76..80] reparse / packed
            //   [80]     filename length
            //   [81]     filename namespace
            //   [82..]   filename UTF-16LE
            entry[64..72].copy_from_slice(&512u64.to_le_bytes());
            let file_attrs: u32 = if is_dir { FILE_NAME_DIRECTORY_FLAG } else { 0 };
            entry[72..76].copy_from_slice(&file_attrs.to_le_bytes());
            entry[80] = filename_len;
            entry[81] = namespace;
            for (i, &unit) in utf16.iter().enumerate() {
                entry[82 + i * 2] = (unit & 0xFF) as u8;
                entry[82 + i * 2 + 1] = (unit >> 8) as u8;
            }
            entry_bytes.extend_from_slice(&entry);
        }

        // Last-entry marker (flags bit 1 = 1, no file reference)
        let mut last = vec![0u8; 16];
        last[8..10].copy_from_slice(&16u16.to_le_bytes());
        last[12..14].copy_from_slice(&2u16.to_le_bytes()); // flags: last entry
        entry_bytes.extend_from_slice(&last);

        let entries_total = entry_bytes.len() as u32;

        // Root value: 16-byte root header + 16-byte node header + entry_bytes
        let mut root = vec![0u8; 32];
        // Root header (bytes 0-15):
        root[0..4].copy_from_slice(&0x30u32.to_le_bytes()); // indexed attr type = $FILE_NAME
        root[4..8].copy_from_slice(&1u32.to_le_bytes()); // collation rule
        root[8..12].copy_from_slice(&4096u32.to_le_bytes()); // index buffer byte size
        root[12] = 1; // clusters per index buffer
                      // Node header (bytes 16-31):
        root[16..20].copy_from_slice(&16u32.to_le_bytes()); // first_entry_offset from node header
        root[20..24].copy_from_slice(&(16 + entries_total).to_le_bytes()); // total size of entries
        root[24..28].copy_from_slice(&(16 + entries_total).to_le_bytes()); // allocated size
        root[28] = 0; // flags: leaf node
        root.extend_from_slice(&entry_bytes);
        root
    }

    // ── $INDEX_ROOT parsing tests ───────────────────────────────────────────────

    #[test]
    fn index_root_three_entries() {
        let value = make_index_root_value(&[
            ("hello.txt", 6, false, 1),
            ("Folder With Spaces", 7, true, 1),
        ]);
        let entries = parse_index_root_entries(&value).expect("parse should succeed");
        assert_eq!(
            entries.len(),
            2,
            "expected 2 entries, got {:?} entries",
            entries.len()
        );
        assert_eq!(entries[0].name, "hello.txt", "first entry name");
        assert_eq!(entries[0].mft_record, 6, "first entry mft_record");
        assert!(!entries[0].is_dir, "first entry should be a file");
        assert_eq!(entries[1].name, "Folder With Spaces", "second entry name");
        assert!(entries[1].is_dir, "second entry should be a directory");
    }

    #[test]
    fn index_root_empty() {
        let value = make_index_root_value(&[]);
        let result = parse_index_root_entries(&value);
        assert!(
            result.unwrap().is_empty(),
            "empty index root should yield no entries"
        );
        // Verify stub distinguishes empty from non-empty.
        let non_empty = make_index_root_value(&[("x.txt", 5, false, 1)]);
        let r2 = parse_index_root_entries(&non_empty).expect("single-entry parse should succeed");
        assert_eq!(
            r2.len(),
            1,
            "single-entry index root must yield exactly 1 entry"
        );
    }

    // ── UTF-16LE decoding tests ─────────────────────────────────────────────────

    #[test]
    fn decode_utf16le_ascii() {
        let units = [0x0068u16, 0x0065, 0x006C, 0x006C, 0x006F];
        let result = decode_utf16le(&units);
        assert_eq!(result.unwrap(), "hello", "ASCII decode failed");
    }

    #[test]
    fn decode_utf16le_emoji_surrogate_pair() {
        // Rocket emoji U+1F680 = surrogate pair [0xD83D, 0xDE80]
        let units = [0xD83Du16, 0xDE80];
        let result = decode_utf16le(&units);
        assert_eq!(result.unwrap(), "🚀", "surrogate pair decode failed");
    }

    #[test]
    fn decode_utf16le_decomposed_combining() {
        // Latin 'a' (U+0061) + combining diaeresis (U+0308) — must NOT be folded to 'ä'
        let units = [0x0061u16, 0x0308];
        let result = decode_utf16le(&units);
        assert_eq!(
            result.unwrap(),
            "a\u{0308}",
            "decomposed combining sequence must be preserved as-is"
        );
    }

    #[test]
    fn decode_utf16le_invalid_lone_surrogate() {
        let units = [0xD83Du16]; // high surrogate without low surrogate
        let result = decode_utf16le(&units);
        assert!(
            matches!(result, Err(NtfsError::InvalidUtf16)),
            "lone high surrogate must return InvalidUtf16, got: {result:?}"
        );
    }

    #[test]
    fn decode_utf16le_bmp_umlaut() {
        // 'ä' U+00E4, 'ö' U+00F6, 'ü' U+00FC — all BMP, no surrogates needed
        let units = [0x00E4u16, 0x00F6, 0x00FC];
        let result = decode_utf16le(&units);
        assert_eq!(result.unwrap(), "äöü", "BMP umlaut decode failed");
    }

    // ── Filename namespace preference tests ─────────────────────────────────────

    #[test]
    fn filename_namespace_prefers_win32_over_dos() {
        let names = [
            (2u8, "ROCKETSC.TXT".to_string()),
            (1u8, "Rocket Science 🚀 For Beginners.txt".to_string()),
        ];
        let result = best_filename(&names);
        assert_eq!(
            result,
            Some("Rocket Science 🚀 For Beginners.txt"),
            "Win32 name (namespace 1) must win over DOS name (namespace 2)"
        );
    }

    #[test]
    fn filename_namespace_win32anddos_preferred() {
        let names = [(3u8, "hello.txt".to_string())];
        let result = best_filename(&names);
        assert_eq!(
            result,
            Some("hello.txt"),
            "Win32AndDos (namespace 3) must be returned"
        );
    }

    #[test]
    fn filename_namespace_dos_only_returns_it() {
        let names = [(2u8, "HELLO~1.TXT".to_string())];
        let result = best_filename(&names);
        assert_eq!(
            result,
            Some("HELLO~1.TXT"),
            "DOS-only fallback (namespace 2) must be returned when nothing better exists"
        );
    }

    // ── read_file_to_writer red tests (T-006) ───────────────────────────────
    //
    // Build a minimal in-memory NTFS volume and test the (not-yet-existing)
    // read_file_to_writer method against it. All tests fail to compile
    // because NtfsFileSystem has no such method.
    //
    // Volume geometry (same as make_ntfs_boot_sector):
    //   BytesPerSector = 512, SectorsPerCluster = 8 → cluster_size = 4 096
    //   MftLcn = 4                                  → MFT at byte 16 384
    //   file_record_size = 1 024
    //
    // MFT records:
    //   0  $MFT        non-resident $DATA  runlist [(LCN=4, len=10)]
    //   5  root dir    $INDEX_ROOT         RESIDENT.TXT→6, NONRES.TXT→8,
    //                                      MULTI.TXT→9, subdir→7, COMPRESSED.TXT→11
    //   6  RESIDENT.TXT  resident $DATA    b"resident content"
    //   7  subdir        directory         nested.txt→10
    //   8  NONRES.TXT    non-resident $DATA  1 run at LCN=14, data_size=16
    //   9  MULTI.TXT     non-resident $DATA  2 runs LCN=15+16, data_size=4099
    //  10  nested.txt    resident $DATA    b"nested"
    //  11  COMPRESSED.TXT resident $DATA   flags=0x0001 (COMPRESSED)
    //
    // Data clusters:
    //  14 (byte 57 344): NONRES.TXT  [0xBB; 16]
    //  15 (byte 61 440): MULTI.TXT run 1  [0xCC; 4096]
    //  16 (byte 65 536): MULTI.TXT run 2  [0xDD; 3] + zeros

    struct StreamCountingWriter {
        calls: usize,
        data: Vec<u8>,
    }

    impl StreamCountingWriter {
        fn new() -> Self {
            Self {
                calls: 0,
                data: Vec::new(),
            }
        }
    }

    impl std::io::Write for StreamCountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Builds a resident attribute byte-string (for embedding in a FILE record).
    fn ntfs_resident_attr(
        attr_type: u32,
        value: &[u8],
        flags: u16,
        attr_id: u16,
        name_length: u8,
    ) -> Vec<u8> {
        let header: usize = 24; // common (16) + resident extension (8)
        let total_raw = header + value.len();
        let total = (total_raw + 7) & !7; // align to 8
        let mut a = vec![0u8; total];
        a[0..4].copy_from_slice(&attr_type.to_le_bytes());
        a[4..8].copy_from_slice(&(total as u32).to_le_bytes());
        // a[8] = 0 (resident)
        a[9] = name_length;
        a[10..12].copy_from_slice(&(header as u16).to_le_bytes()); // name_offset
        a[12..14].copy_from_slice(&flags.to_le_bytes());
        a[14..16].copy_from_slice(&attr_id.to_le_bytes());
        a[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes()); // value_length
        a[20..22].copy_from_slice(&(header as u16).to_le_bytes()); // value_offset
        a[header..header + value.len()].copy_from_slice(value);
        a
    }

    /// Builds a non-resident attribute byte-string.
    fn ntfs_nonresident_attr(
        attr_type: u32,
        runlist: &[u8],
        data_size: u64,
        flags: u16,
        attr_id: u16,
    ) -> Vec<u8> {
        let header: usize = 64; // common (16) + non-resident extension (48)
        let total_raw = header + runlist.len();
        let total = (total_raw + 7) & !7;
        let mut a = vec![0u8; total];
        a[0..4].copy_from_slice(&attr_type.to_le_bytes());
        a[4..8].copy_from_slice(&(total as u32).to_le_bytes());
        a[8] = 1; // non_resident
                  // a[9] = 0 (name_length = 0)
        a[10..12].copy_from_slice(&(header as u16).to_le_bytes()); // name_offset
        a[12..14].copy_from_slice(&flags.to_le_bytes());
        a[14..16].copy_from_slice(&attr_id.to_le_bytes());
        // 16..24: lowest_vcn = 0
        // 24..32: highest_vcn = 0
        a[32..34].copy_from_slice(&(header as u16).to_le_bytes()); // data_runs_offset = 64
                                                                   // 34..36: compression_unit = 0
                                                                   // 36..40: padding
        a[40..48].copy_from_slice(&data_size.to_le_bytes()); // allocated_size
        a[48..56].copy_from_slice(&data_size.to_le_bytes()); // data_size
        a[56..64].copy_from_slice(&data_size.to_le_bytes()); // initialized_size (→ hdr.data_size)
        a[header..header + runlist.len()].copy_from_slice(runlist);
        a
    }

    /// Writes a FILE record with `attrs` at `rec_num` into `vol`.
    fn ntfs_place_record(vol: &mut [u8], rec_num: usize, is_dir: bool, attrs: &[Vec<u8>]) {
        const RS: usize = 1024;
        const CS: usize = 4096;
        const MFT_OFF: usize = 4 * CS; // MftLcn=4

        let rb = rec_num * RS;
        let off = MFT_OFF + (rb / CS) * CS + (rb % CS);

        let mut r = make_file_record(RS);
        if is_dir {
            let flags = u16::from_le_bytes([r[22], r[23]]) | 0x02;
            r[22..24].copy_from_slice(&flags.to_le_bytes());
        }
        // Place attributes after first_attr_offset = 56
        let mut pos = 56;
        for attr in attrs {
            r[pos..pos + attr.len()].copy_from_slice(attr);
            pos += attr.len();
        }
        // End-of-attributes marker
        r[pos..pos + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        vol[off..off + RS].copy_from_slice(&r);
    }

    /// Builds a minimal, fully navigable in-memory NTFS volume for streaming tests.
    fn build_ntfs_streaming_vol() -> Vec<u8> {
        const CS: usize = 4096;
        let mut vol = vec![0u8; 17 * CS]; // 69 632 bytes

        // Boot sector — BytesPerSector=4096 so sector_end (4096) > record_size (1024)
        // in apply_file_record_fixup, skipping all USA endpoint checks and preventing
        // large resident attributes from causing FixupValidationFailed.
        vol[3..11].copy_from_slice(b"NTFS    ");
        vol[11..13].copy_from_slice(&4096u16.to_le_bytes()); // BytesPerSector
        vol[13] = 1; // SectorsPerCluster → cluster_size = 4096
        vol[21] = 0xF8; // MediaDescriptor
        vol[40..48].copy_from_slice(&127u64.to_le_bytes()); // TotalSectors
        vol[48..56].copy_from_slice(&4u64.to_le_bytes()); // MftLcn
        vol[56..64].copy_from_slice(&64u64.to_le_bytes()); // MftMirrorLcn (unused)
        vol[64] = (-10i8) as u8; // file_record_size = 2^10 = 1 024
        vol[68] = (-11i8) as u8; // index_buffer_size = 2^11 = 2 048
        vol[510] = 0x55;
        vol[511] = 0xAA; // boot signature

        // Record 0 ($MFT): non-resident $DATA, runlist [(LCN=4, len=10)]
        // Runlist: header 0x11 (len_size=1, off_size=1), len=0x0A, lcn=0x04, term=0x00
        let mft_runlist = [0x11u8, 0x0A, 0x04, 0x00];
        ntfs_place_record(
            &mut vol,
            0,
            false,
            &[ntfs_nonresident_attr(
                0x80,
                &mft_runlist,
                (10 * CS) as u64,
                0,
                1,
            )],
        );

        // Record 5 (root directory): $INDEX_ROOT
        let root_idx = make_index_root_value(&[
            ("RESIDENT.TXT", 6, false, 1),
            ("NONRES.TXT", 8, false, 1),
            ("MULTI.TXT", 9, false, 1),
            ("subdir", 7, true, 1),
            ("COMPRESSED.TXT", 11, false, 1),
        ]);
        ntfs_place_record(
            &mut vol,
            5,
            true,
            &[ntfs_resident_attr(0x90, &root_idx, 0, 1, 0)],
        );

        // Record 6 (RESIDENT.TXT): resident $DATA b"resident content"
        ntfs_place_record(
            &mut vol,
            6,
            false,
            &[ntfs_resident_attr(0x80, b"resident content", 0, 1, 0)],
        );

        // Record 7 (subdir/): $INDEX_ROOT with nested.txt → record 10
        let sub_idx = make_index_root_value(&[("nested.txt", 10, false, 1)]);
        ntfs_place_record(
            &mut vol,
            7,
            true,
            &[ntfs_resident_attr(0x90, &sub_idx, 0, 1, 0)],
        );

        // Record 8 (NONRES.TXT): 1 run at LCN=14, data_size=16
        // Runlist: [0x11, 0x01, 0x0E, 0x00]  (len=1 cluster at LCN 14)
        let nonres_rl = [0x11u8, 0x01, 0x0E, 0x00];
        ntfs_place_record(
            &mut vol,
            8,
            false,
            &[ntfs_nonresident_attr(0x80, &nonres_rl, 16, 0, 1)],
        );
        vol[14 * CS..14 * CS + 16].fill(0xBB); // NONRES.TXT data

        // Record 9 (MULTI.TXT): 2 runs (LCN=15, len=1) + (delta=+1, len=1), data_size=4099
        // Runlist: [0x11,0x01,0x0F,  0x11,0x01,0x01,  0x00]
        let multi_rl = [0x11u8, 0x01, 0x0F, 0x11, 0x01, 0x01, 0x00];
        ntfs_place_record(
            &mut vol,
            9,
            false,
            &[ntfs_nonresident_attr(0x80, &multi_rl, 4099, 0, 1)],
        );
        vol[15 * CS..15 * CS + CS].fill(0xCC); // run 1: 4 096 bytes of 0xCC
        vol[16 * CS..16 * CS + 3].fill(0xDD); // run 2 first 3 bytes: 0xDD (rest 0x00)

        // Record 10 (nested.txt inside subdir/): resident $DATA b"nested"
        ntfs_place_record(
            &mut vol,
            10,
            false,
            &[ntfs_resident_attr(0x80, b"nested", 0, 1, 0)],
        );

        // Record 11 (COMPRESSED.TXT): resident $DATA with COMPRESSED flag (0x0001)
        // check_unsupported_flags fires when flags & 0x0001 != 0.
        ntfs_place_record(
            &mut vol,
            11,
            false,
            &[ntfs_resident_attr(0x80, b"", 0x0001, 1, 0)],
        );

        vol
    }

    #[test]
    fn stream_ntfs_resident_file() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/RESIDENT.TXT", &mut out)
            .expect("RESIDENT.TXT streaming must succeed");
        assert_eq!(n, 16, "byte count must match data length");
        assert_eq!(out, b"resident content", "content must match");
    }

    #[test]
    fn stream_ntfs_nonresident_single_run() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/NONRES.TXT", &mut out)
            .expect("NONRES.TXT streaming must succeed");
        assert_eq!(n, 16);
        assert_eq!(out, vec![0xBBu8; 16]);
    }

    #[test]
    fn stream_ntfs_nonresident_multi_run() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/MULTI.TXT", &mut out)
            .expect("MULTI.TXT streaming must succeed");
        assert_eq!(n, 4099);
        let mut expected = vec![0xCCu8; 4096];
        expected.extend_from_slice(&[0xDD, 0xDD, 0xDD]);
        assert_eq!(out, expected);
    }

    #[test]
    fn stream_ntfs_nested_path() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let n = fs
            .read_file_to_writer("/subdir/nested.txt", &mut out)
            .expect("/subdir/nested.txt streaming must succeed");
        assert_eq!(n, 6);
        assert_eq!(out, b"nested");
    }

    #[test]
    fn stream_ntfs_directory_returns_error() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/subdir", &mut out)
            .expect_err("/subdir must fail: source is a directory");
        assert!(
            matches!(err, NtfsError::AttemptedDirectoryExtraction { .. }),
            "expected AttemptedDirectoryExtraction, got {err}"
        );
    }

    #[test]
    fn stream_ntfs_not_found_returns_error() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/MISSING.TXT", &mut out)
            .expect_err("/MISSING.TXT must fail");
        assert!(
            matches!(err, NtfsError::PathNotFound { .. }),
            "expected PathNotFound, got {err}"
        );
    }

    #[test]
    fn stream_ntfs_compressed_returns_unsupported() {
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut out = Vec::new();
        let err = fs
            .read_file_to_writer("/COMPRESSED.TXT", &mut out)
            .expect_err("compressed file must fail");
        assert!(
            matches!(err, NtfsError::UnsupportedCompressedData),
            "expected UnsupportedCompressedData, got {err}"
        );
    }

    #[test]
    fn stream_ntfs_multi_run_writer_multiple_calls() {
        // MULTI.TXT spans 2 clusters; streaming must produce >= 2 write calls.
        let fs = NtfsFileSystem::open(MemReader(build_ntfs_streaming_vol())).unwrap();
        let mut writer = StreamCountingWriter::new();
        let n = fs
            .read_file_to_writer("/MULTI.TXT", &mut writer)
            .expect("MULTI.TXT streaming must succeed");
        assert_eq!(n, 4099);
        assert_eq!(writer.data.len(), 4099);
        assert!(
            writer.calls >= 2,
            "2-run non-resident file must produce >= 2 write calls, got {}",
            writer.calls
        );
    }
}
