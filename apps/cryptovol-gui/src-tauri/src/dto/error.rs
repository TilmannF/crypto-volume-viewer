//! The sanitized, stable-coded error DTO returned to the frontend by every
//! Tauri command.

use serde::Serialize;

/// Volume authentication failed, or the volume uses unsupported open parameters.
pub const AUTH_FAILED: &str = "auth_failed";
/// The container or volume profile is outside current support.
pub const UNSUPPORTED_FORMAT: &str = "unsupported_format";
/// The decrypted volume's filesystem could not be recognized.
pub const FILESYSTEM_NOT_RECOGNIZED: &str = "filesystem_not_recognized";
/// The requested container path does not exist.
pub const PATH_NOT_FOUND: &str = "path_not_found";
/// The caller attempted to extract a directory as if it were a file.
pub const DIRECTORY_EXTRACTION_UNSUPPORTED: &str = "directory_extraction_unsupported";
/// The requested operation is not supported by this filesystem or backend.
pub const UNSUPPORTED_FEATURE: &str = "unsupported_feature";
/// The caller cancelled the operation.
pub const CANCELLED: &str = "cancelled";
/// A host filesystem or container read/write operation failed.
pub const IO_ERROR: &str = "io_error";
/// The caller supplied a request that cannot be fulfilled as given.
pub const INVALID_INPUT: &str = "invalid_input";
/// The given session id does not refer to an open session.
pub const SESSION_NOT_FOUND: &str = "session_not_found";
/// The given job id does not refer to a known extraction job.
pub const JOB_NOT_FOUND: &str = "job_not_found";
/// An unexpected internal GUI-layer condition occurred.
pub const INTERNAL_ERROR: &str = "internal_error";

/// Sanitized error returned to the frontend by every Tauri command.
///
/// Messages are short and user-facing; they must never include raw `Debug`
/// output, stack traces, secrets, or raw binary data from the underlying
/// error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiErrorDto {
    /// Stable, machine-readable error code (see the constants in this module).
    pub code: String,
    /// Short, sanitized, user-facing message.
    pub message: String,
}

impl GuiErrorDto {
    /// Builds a `GuiErrorDto` from a stable code and a sanitized message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
