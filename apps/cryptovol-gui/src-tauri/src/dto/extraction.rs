//! DTOs for starting a single-file extraction.

use serde::{Deserialize, Serialize};

/// Request to extract one file from an opened volume to a host destination.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractFileRequestDto {
    /// Session the source file belongs to.
    pub session_id: String,
    /// Container-relative source path (uses `/` separators).
    pub source_path: String,
    /// Host destination path.
    pub destination_path: String,
    /// Overwrite the destination file if it already exists.
    pub overwrite: bool,
    /// Create missing parent directories of the destination.
    pub parents: bool,
}

/// Returned immediately after `extract_file` starts a background job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractStartedDto {
    /// Opaque id for this extraction job, used by `cancel_extract` and to
    /// correlate `extract://*` events.
    pub job_id: String,
    /// Session the job belongs to.
    pub session_id: String,
    /// Container-relative source path.
    pub source_path: String,
    /// Host destination path.
    pub destination_path: String,
    /// Total bytes to write, when known in advance.
    pub total_bytes: Option<u64>,
}
