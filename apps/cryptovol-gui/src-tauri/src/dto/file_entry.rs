//! DTOs mirroring `cryptovol_app::FileEntry`/`AppTimestamp` for directory
//! listings and file metadata.

use crate::dto::filesystem_to_wire_str;
use cryptovol_app::{AppTimestamp, FileAttributes, FileEntry};
use serde::Serialize;

/// Filesystem attribute flags for a single entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAttributesDto {
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

impl From<FileAttributes> for FileAttributesDto {
    fn from(attributes: FileAttributes) -> Self {
        Self {
            read_only: attributes.read_only,
            hidden: attributes.hidden,
            system: attributes.system,
            directory: attributes.directory,
            archive: attributes.archive,
        }
    }
}

/// A calendar timestamp for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTimestampDto {
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

impl From<AppTimestamp> for AppTimestampDto {
    fn from(timestamp: AppTimestamp) -> Self {
        Self {
            year: timestamp.year,
            month: timestamp.month,
            day: timestamp.day,
            hour: timestamp.hour,
            minute: timestamp.minute,
            second: timestamp.second,
        }
    }
}

/// A single directory/file entry as shown to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntryDto {
    /// Entry name within its parent directory.
    pub name: String,
    /// Full container path, when known (`stat` fills this in; directory
    /// listings only know each entry's name and leave this `None`).
    pub path: Option<String>,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Size in bytes.
    pub size: u64,
    /// Filesystem attribute flags.
    pub attributes: FileAttributesDto,
    /// Creation timestamp, when present and valid.
    pub created: Option<AppTimestampDto>,
    /// Last-modified timestamp, when present and valid.
    pub modified: Option<AppTimestampDto>,
    /// Last-accessed timestamp, when present and valid.
    pub accessed: Option<AppTimestampDto>,
    /// Owning filesystem kind, as a lowercase wire string.
    pub filesystem: String,
}

impl From<FileEntry> for FileEntryDto {
    fn from(entry: FileEntry) -> Self {
        Self {
            name: entry.name,
            path: entry.path,
            is_dir: entry.is_dir,
            size: entry.size,
            attributes: entry.attributes.into(),
            created: entry.created.map(AppTimestampDto::from),
            modified: entry.modified.map(AppTimestampDto::from),
            accessed: entry.accessed.map(AppTimestampDto::from),
            filesystem: filesystem_to_wire_str(entry.filesystem).to_string(),
        }
    }
}
