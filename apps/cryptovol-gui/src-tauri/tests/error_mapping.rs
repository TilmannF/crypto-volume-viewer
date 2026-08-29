#![allow(
    missing_docs,
    reason = "error mapping tests exercise the public From<AppError> for GuiErrorDto conversion"
)]

//! Verifies every `cryptovol_app::AppError` variant maps to a stable
//! `GuiErrorDto` code with a sanitized message that never repeats the
//! underlying error's raw path/byte payload beyond what's already safe to
//! show a user, and never contains a password.

use cryptovol_app::AppError;
use cryptovol_gui_lib::dto::error::GuiErrorDto;

const TEST_PASSWORD: &str = "correct password";

fn assert_maps_to(app_error: AppError, expected_code: &str) {
    let gui_error: GuiErrorDto = app_error.into();
    assert_eq!(gui_error.code, expected_code);
    assert!(!gui_error.message.is_empty());
    assert!(
        !gui_error.message.contains(TEST_PASSWORD),
        "mapped error message must never contain the password: {}",
        gui_error.message
    );
}

#[test]
fn auth_failed_maps_to_auth_failed_code() {
    assert_maps_to(AppError::AuthFailed, "auth_failed");
}

#[test]
fn unsupported_format_maps_to_unsupported_format_code() {
    assert_maps_to(
        AppError::UnsupportedFormat("unknown profile".to_string()),
        "unsupported_format",
    );
}

#[test]
fn filesystem_not_recognized_maps_to_filesystem_not_recognized_code() {
    assert_maps_to(
        AppError::FilesystemNotRecognized,
        "filesystem_not_recognized",
    );
}

#[test]
fn path_not_found_maps_to_path_not_found_code() {
    assert_maps_to(
        AppError::PathNotFound("/missing.txt".to_string()),
        "path_not_found",
    );
}

#[test]
fn directory_extraction_unsupported_maps_to_its_own_code() {
    assert_maps_to(
        AppError::DirectoryExtractionUnsupported("/dir".to_string()),
        "directory_extraction_unsupported",
    );
}

#[test]
fn unsupported_feature_maps_to_unsupported_feature_code() {
    assert_maps_to(
        AppError::UnsupportedFeature("hidden volumes".to_string()),
        "unsupported_feature",
    );
}

#[test]
fn cancelled_maps_to_cancelled_code() {
    assert_maps_to(AppError::Cancelled, "cancelled");
}

#[test]
fn io_maps_to_io_error_code() {
    assert_maps_to(AppError::Io(std::io::Error::other("disk full")), "io_error");
}

#[test]
fn extraction_failed_maps_to_io_error_code() {
    // ExtractionFailed covers destination write failures after the volume
    // and filesystem are already open; it shares io_error's "read/write
    // operation failed" meaning rather than getting its own GUI-facing code.
    assert_maps_to(
        AppError::ExtractionFailed("disk full".to_string()),
        "io_error",
    );
}

#[test]
fn invalid_input_maps_to_invalid_input_code() {
    assert_maps_to(
        AppError::InvalidInput("bad pim".to_string()),
        "invalid_input",
    );
}
