//! Unified, filesystem-agnostic data models: [`FileEntry`], [`FileAttributes`],
//! [`AppTimestamp`], and [`FilesystemKind`].

use cryptovol_fs_exfat::{ExfatAttributes, ExfatEntry, ExfatTimestamp};
use cryptovol_fs_fat::{DirectoryEntry, FatAttributes, FatDate, FatTimestamp};
use cryptovol_fs_ntfs::{NtfsAttributes, NtfsEntry, NtfsTimestamp};
use cryptovol_tcvc::FilesystemProbeCandidate;

/// Filesystem family detected inside an opened volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemKind {
    /// FAT12/FAT16/FAT32.
    Fat,
    /// exFAT.
    ExFat,
    /// NTFS.
    Ntfs,
    /// No supported filesystem was recognized.
    Unknown,
}

impl From<FilesystemProbeCandidate> for FilesystemKind {
    fn from(candidate: FilesystemProbeCandidate) -> Self {
        match candidate {
            FilesystemProbeCandidate::FatLike => FilesystemKind::Fat,
            FilesystemProbeCandidate::ExFat => FilesystemKind::ExFat,
            FilesystemProbeCandidate::Ntfs => FilesystemKind::Ntfs,
            FilesystemProbeCandidate::Unknown => FilesystemKind::Unknown,
        }
    }
}

/// Filesystem-agnostic attribute flags for a file or directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileAttributes {
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

impl From<FatAttributes> for FileAttributes {
    fn from(attrs: FatAttributes) -> Self {
        FileAttributes {
            read_only: attrs.read_only,
            hidden: attrs.hidden,
            system: attrs.system,
            directory: attrs.directory,
            archive: attrs.archive,
        }
    }
}

impl From<ExfatAttributes> for FileAttributes {
    fn from(attrs: ExfatAttributes) -> Self {
        FileAttributes {
            read_only: attrs.read_only,
            hidden: attrs.hidden,
            system: attrs.system,
            directory: attrs.directory,
            archive: attrs.archive,
        }
    }
}

impl From<NtfsAttributes> for FileAttributes {
    fn from(attrs: NtfsAttributes) -> Self {
        FileAttributes {
            read_only: attrs.read_only,
            hidden: attrs.hidden,
            system: attrs.system,
            directory: attrs.directory,
            archive: attrs.archive,
        }
    }
}

/// A calendar timestamp shared across filesystems.
///
/// Interpretation depends on the source filesystem: FAT timestamps have
/// 2-second resolution and carry no timezone; exFAT timestamps are local
/// time with an optional UTC offset that is not applied here; NTFS
/// timestamps are derived from Windows FILETIME values and are UTC. This
/// type does not perform timezone conversion or normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppTimestamp {
    /// Four-digit Gregorian year.
    pub year: u16,
    /// Month of year (1-12).
    pub month: u8,
    /// Day of month (1-31).
    pub day: u8,
    /// Hour (0-23).
    pub hour: u8,
    /// Minute (0-59).
    pub minute: u8,
    /// Second (0-59).
    pub second: u8,
}

impl From<FatTimestamp> for AppTimestamp {
    fn from(ts: FatTimestamp) -> Self {
        AppTimestamp {
            year: ts.date.year,
            month: ts.date.month,
            day: ts.date.day,
            hour: ts.time.hour,
            minute: ts.time.minute,
            second: ts.time.second,
        }
    }
}

impl From<FatDate> for AppTimestamp {
    fn from(date: FatDate) -> Self {
        AppTimestamp {
            year: date.year,
            month: date.month,
            day: date.day,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

impl From<ExfatTimestamp> for AppTimestamp {
    fn from(ts: ExfatTimestamp) -> Self {
        AppTimestamp {
            year: ts.year,
            month: ts.month,
            day: ts.day,
            hour: ts.hour,
            minute: ts.minute,
            second: ts.second,
        }
    }
}

impl AppTimestamp {
    /// Converts an NTFS Unix-epoch-seconds timestamp into a calendar
    /// `AppTimestamp`.
    ///
    /// Returns `None` for timestamps before 1970-01-01 UTC, since
    /// `AppTimestamp` does not attempt to represent them; this mirrors the
    /// CLI's prior pre-1970 handling.
    pub fn from_ntfs(ts: &NtfsTimestamp) -> Option<Self> {
        let secs = ts.unix_seconds;
        if secs < 0 {
            return None;
        }
        let secs = secs as u64;
        let days = secs / 86_400;
        let time_secs = secs % 86_400;
        let hour = (time_secs / 3_600) as u8;
        let minute = ((time_secs % 3_600) / 60) as u8;
        let second = (time_secs % 60) as u8;
        let (year, month, day) = days_to_ymd(days);
        Some(AppTimestamp {
            year: year as u16,
            month: month as u8,
            day: day as u8,
            hour,
            minute,
            second,
        })
    }
}

/// Converts days since the Unix epoch into a Gregorian (year, month, day)
/// using Howard Hinnant's days-from-civil algorithm.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// A unified file or directory entry, independent of the source filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Entry name within its parent directory.
    pub name: String,
    /// Full container path, when known. Directory listings only know each
    /// entry's name and leave this `None`; `stat` fills it in with the
    /// queried path.
    pub path: Option<String>,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Size in bytes.
    pub size: u64,
    /// Filesystem attribute flags.
    pub attributes: FileAttributes,
    /// Creation timestamp, when present and valid.
    pub created: Option<AppTimestamp>,
    /// Last-modified timestamp, when present and valid.
    pub modified: Option<AppTimestamp>,
    /// Last-accessed timestamp, when present and valid.
    pub accessed: Option<AppTimestamp>,
    /// Source filesystem family.
    pub filesystem: FilesystemKind,
}

impl From<(DirectoryEntry, FilesystemKind)> for FileEntry {
    fn from((entry, filesystem): (DirectoryEntry, FilesystemKind)) -> Self {
        FileEntry {
            name: entry.name,
            path: None,
            is_dir: entry.is_dir,
            size: u64::from(entry.size),
            attributes: entry.attributes.into(),
            created: entry.created.map(AppTimestamp::from),
            modified: entry.modified.map(AppTimestamp::from),
            accessed: entry.accessed.map(AppTimestamp::from),
            filesystem,
        }
    }
}

impl From<(ExfatEntry, FilesystemKind)> for FileEntry {
    fn from((entry, filesystem): (ExfatEntry, FilesystemKind)) -> Self {
        FileEntry {
            name: entry.name,
            path: None,
            is_dir: entry.is_dir,
            size: entry.size,
            attributes: entry.attributes.into(),
            created: entry.created.map(AppTimestamp::from),
            modified: entry.modified.map(AppTimestamp::from),
            accessed: entry.accessed.map(AppTimestamp::from),
            filesystem,
        }
    }
}

impl From<(NtfsEntry, FilesystemKind)> for FileEntry {
    fn from((entry, filesystem): (NtfsEntry, FilesystemKind)) -> Self {
        FileEntry {
            name: entry.name,
            path: None,
            is_dir: entry.is_dir,
            size: entry.size,
            attributes: entry.attributes.into(),
            created: entry.created.as_ref().and_then(AppTimestamp::from_ntfs),
            modified: entry.modified.as_ref().and_then(AppTimestamp::from_ntfs),
            accessed: entry.accessed.as_ref().and_then(AppTimestamp::from_ntfs),
            filesystem,
        }
    }
}
