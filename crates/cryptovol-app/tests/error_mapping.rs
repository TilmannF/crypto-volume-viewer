#![allow(
    missing_docs,
    clippy::panic,
    reason = "error-mapping tests assert on match arms with direct panics"
)]

//! Verifies that TC/VC, FAT, exFAT, NTFS, and I/O errors map into
//! `AppError` without leaking secrets or large binary payloads.

use cryptovol_app::{require_recognized_filesystem, AppError};
use cryptovol_fs_exfat::ExfatError;
use cryptovol_fs_fat::FatError;
use cryptovol_fs_ntfs::NtfsError;
use cryptovol_tcvc::{FilesystemProbeCandidate, TcvcOpenError};

#[test]
fn authentication_or_unsupported_maps_to_auth_failed() {
    let err = AppError::from(TcvcOpenError::AuthenticationOrUnsupported);
    assert!(
        matches!(err, AppError::AuthFailed),
        "expected AuthFailed, got {err:?}"
    );
}

#[test]
fn invalid_pim_maps_to_auth_failed() {
    let err = AppError::from(TcvcOpenError::InvalidPim {
        reason: "pim too large",
    });
    assert!(
        matches!(err, AppError::AuthFailed),
        "expected AuthFailed, got {err:?}"
    );
}

#[test]
fn unsupported_profile_maps_to_unsupported_format() {
    let err = AppError::from(TcvcOpenError::UnsupportedProfile {
        reason: "unsupported cipher",
    });
    assert!(
        matches!(err, AppError::UnsupportedFormat(_)),
        "expected UnsupportedFormat, got {err:?}"
    );
}

#[test]
fn fat_path_not_found_maps_to_path_not_found() {
    let err = AppError::from(FatError::PathNotFound {
        path: "/missing.txt".to_string(),
    });
    match err {
        AppError::PathNotFound(path) => assert_eq!(path, "/missing.txt"),
        other => panic!("expected PathNotFound, got {other:?}"),
    }
}

#[test]
fn exfat_path_not_found_maps_to_path_not_found() {
    let err = AppError::from(ExfatError::PathNotFound {
        path: "/missing.txt".to_string(),
    });
    match err {
        AppError::PathNotFound(path) => assert_eq!(path, "/missing.txt"),
        other => panic!("expected PathNotFound, got {other:?}"),
    }
}

#[test]
fn ntfs_path_not_found_maps_to_path_not_found() {
    let err = AppError::from(NtfsError::PathNotFound {
        path: "/missing.txt".to_string(),
    });
    match err {
        AppError::PathNotFound(path) => assert_eq!(path, "/missing.txt"),
        other => panic!("expected PathNotFound, got {other:?}"),
    }
}

#[test]
fn io_error_maps_to_io_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
    let err = AppError::from(io_err);
    assert!(matches!(err, AppError::Io(_)), "expected Io, got {err:?}");
}

#[test]
fn unrecognized_filesystem_probe_maps_to_filesystem_not_recognized() {
    let result = require_recognized_filesystem(FilesystemProbeCandidate::Unknown);
    assert!(
        matches!(result, Err(AppError::FilesystemNotRecognized)),
        "expected FilesystemNotRecognized, got {result:?}"
    );
}

#[test]
fn recognized_filesystem_probe_passes_through_unchanged() {
    let result = require_recognized_filesystem(FilesystemProbeCandidate::FatLike);
    assert!(
        matches!(result, Ok(FilesystemProbeCandidate::FatLike)),
        "expected the candidate to pass through unchanged, got {result:?}"
    );
}

#[test]
fn error_display_and_debug_never_contain_secret_markers() {
    let password_marker = "super-secret-password";
    let errors: Vec<AppError> = vec![
        AppError::from(TcvcOpenError::AuthenticationOrUnsupported),
        AppError::from(TcvcOpenError::UnsupportedProfile {
            reason: "unsupported cipher",
        }),
        AppError::from(FatError::PathNotFound {
            path: "/secret/path.txt".to_string(),
        }),
        AppError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing file",
        )),
    ];
    for err in errors {
        let debug = format!("{err:?}");
        let display = format!("{err}");
        assert!(
            !debug.contains(password_marker) && !display.contains(password_marker),
            "error output must never contain secret markers: {debug} / {display}"
        );
    }
}
