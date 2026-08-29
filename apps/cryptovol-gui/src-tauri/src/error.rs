//! GUI error mapping from `cryptovol_app::AppError` to the sanitized,
//! stable-coded `GuiErrorDto` returned to the frontend.
//!
//! Mapped messages stay short and user-facing; they never include the
//! underlying error's raw `Debug` output, stack traces, secrets, or raw
//! binary data.

use crate::dto::error::{
    GuiErrorDto, AUTH_FAILED, CANCELLED, DIRECTORY_EXTRACTION_UNSUPPORTED,
    FILESYSTEM_NOT_RECOGNIZED, INVALID_INPUT, IO_ERROR, PATH_NOT_FOUND, UNSUPPORTED_FEATURE,
    UNSUPPORTED_FORMAT,
};
use cryptovol_app::AppError;

impl From<AppError> for GuiErrorDto {
    fn from(err: AppError) -> Self {
        match err {
            // Matches the CLI's documented test-open wording (AGENTS.md) and
            // avoids echoing the word "password" next to any user-entered
            // value.
            AppError::AuthFailed => GuiErrorDto::new(
                AUTH_FAILED,
                "authentication failed or unsupported parameters",
            ),
            AppError::UnsupportedFormat(_) => GuiErrorDto::new(
                UNSUPPORTED_FORMAT,
                "this container's format or profile is not supported",
            ),
            AppError::FilesystemNotRecognized => GuiErrorDto::new(
                FILESYSTEM_NOT_RECOGNIZED,
                "the volume's filesystem could not be recognized",
            ),
            AppError::PathNotFound(path) => {
                GuiErrorDto::new(PATH_NOT_FOUND, format!("path not found: {path}"))
            }
            AppError::DirectoryExtractionUnsupported(path) => GuiErrorDto::new(
                DIRECTORY_EXTRACTION_UNSUPPORTED,
                format!("cannot extract a directory: {path}"),
            ),
            AppError::UnsupportedFeature(feature) => GuiErrorDto::new(
                UNSUPPORTED_FEATURE,
                format!("unsupported feature: {feature}"),
            ),
            AppError::Cancelled => GuiErrorDto::new(CANCELLED, "operation cancelled"),
            // ExtractionFailed shares io_error's "a read or write operation
            // failed" meaning rather than getting its own GUI-facing code.
            AppError::Io(_) | AppError::ExtractionFailed(_) => {
                GuiErrorDto::new(IO_ERROR, "a read or write operation failed")
            }
            AppError::InvalidInput(reason) => {
                GuiErrorDto::new(INVALID_INPUT, format!("invalid input: {reason}"))
            }
        }
    }
}
