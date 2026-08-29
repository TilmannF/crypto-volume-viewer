//! Event-name constants and payload types emitted to the frontend during
//! extraction (`extract://started`, `extract://progress`, `extract://finished`,
//! `extract://cancelled`, `extract://failed`).
//!
//! Payloads may carry job/session ids, paths, and byte counts, but must never
//! carry passwords, keys, decrypted file contents, or decrypted header bytes.

use serde::Serialize;

/// Emitted once, when extraction of a job begins.
pub const EXTRACT_STARTED: &str = "extract://started";
/// Emitted after each chunk written to the destination.
pub const EXTRACT_PROGRESS: &str = "extract://progress";
/// Emitted once, when extraction completes successfully.
pub const EXTRACT_FINISHED: &str = "extract://finished";
/// Emitted once, when extraction is cancelled.
pub const EXTRACT_CANCELLED: &str = "extract://cancelled";
/// Emitted once, when extraction fails for a reason other than cancellation.
pub const EXTRACT_FAILED: &str = "extract://failed";

/// Payload for [`EXTRACT_STARTED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractStartedEvent {
    /// The extraction job this event belongs to.
    pub job_id: String,
    /// The session the job belongs to.
    pub session_id: String,
    /// Container-relative source path.
    pub source_path: String,
    /// Host destination path.
    pub destination_path: String,
    /// Total bytes to write, when known in advance.
    pub total_bytes: Option<u64>,
}

/// Payload for [`EXTRACT_PROGRESS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProgressEvent {
    /// The extraction job this event belongs to.
    pub job_id: String,
    /// The session the job belongs to.
    pub session_id: String,
    /// Cumulative bytes written so far.
    pub bytes_written: u64,
    /// Total bytes to write, when known in advance.
    pub total_bytes: Option<u64>,
}

/// Payload for [`EXTRACT_FINISHED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractFinishedEvent {
    /// The extraction job this event belongs to.
    pub job_id: String,
    /// The session the job belongs to.
    pub session_id: String,
    /// Total bytes written to the destination.
    pub bytes_written: u64,
}

/// Payload for [`EXTRACT_CANCELLED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractCancelledEvent {
    /// The extraction job this event belongs to.
    pub job_id: String,
    /// The session the job belongs to.
    pub session_id: String,
}

/// Payload for [`EXTRACT_FAILED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractFailedEvent {
    /// The extraction job this event belongs to.
    pub job_id: String,
    /// The session the job belongs to.
    pub session_id: String,
    /// Stable, sanitized error code (see `crate::dto::error`).
    pub code: String,
    /// Short, sanitized, user-facing message.
    pub message: String,
}

/// A single extraction-lifecycle event, tagged with its wire event name via
/// [`ExtractionEvent::name`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionEvent {
    /// See [`ExtractStartedEvent`].
    Started(ExtractStartedEvent),
    /// See [`ExtractProgressEvent`].
    Progress(ExtractProgressEvent),
    /// See [`ExtractFinishedEvent`].
    Finished(ExtractFinishedEvent),
    /// See [`ExtractCancelledEvent`].
    Cancelled(ExtractCancelledEvent),
    /// See [`ExtractFailedEvent`].
    Failed(ExtractFailedEvent),
}

impl ExtractionEvent {
    /// Returns this event's wire event name (one of the `EXTRACT_*` constants).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Started(_) => EXTRACT_STARTED,
            Self::Progress(_) => EXTRACT_PROGRESS,
            Self::Finished(_) => EXTRACT_FINISHED,
            Self::Cancelled(_) => EXTRACT_CANCELLED,
            Self::Failed(_) => EXTRACT_FAILED,
        }
    }
}
