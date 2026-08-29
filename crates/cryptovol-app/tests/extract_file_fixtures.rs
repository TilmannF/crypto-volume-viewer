#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "fixture tests assert against documented ground truth in docs/test-containers.md"
)]

//! Env-gated integration tests verifying `VolumeSession::extract_file`
//! against real FAT/exFAT/NTFS LFN fixtures: byte-for-byte extraction,
//! Started/Advanced/Finished progress reporting, and mid-extraction
//! cancellation.
//!
//! `#[ignore]`d like the other fixture tests, so `cargo test --workspace
//! --all-targets` passes without any static fixtures present. Run with the
//! documented `CRYPTOVOL_STATIC_*_LFN_FIXTURE` env vars and `-- --ignored`;
//! see `docs/test-containers.md`.

use cryptovol_app::{
    open_volume, AppError, CancellationToken, ExtractOptions, OpenVolumeRequest, ProgressEvent,
    VolumeSession,
};
use cryptovol_tcvc::TcvcKdf;
use secrecy::SecretString;
use std::path::{Path, PathBuf};

const TEST_PASSWORD: &str = "test-password";
const KNOWN_FILE: &str = "/Project Notes Final.txt";
const KNOWN_FILE_SIZE: u64 = 49;

fn fixture_path(env_var: &str) -> Option<PathBuf> {
    let val = std::env::var(env_var).ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live two levels below workspace root")
        .to_path_buf()
}

fn ground_truth_bytes() -> Vec<u8> {
    let path = workspace_root()
        .join("testdata/static/fs-fat-lfn-original")
        .join("Project Notes Final.txt");
    std::fs::read(&path).expect("ground truth file should be readable")
}

fn open_fixture(path: &Path) -> VolumeSession {
    open_volume(OpenVolumeRequest {
        container_path: path.to_path_buf(),
        password: SecretString::from(TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    })
    .expect("fixture should open with the documented test password")
}

fn assert_extracts_known_file_with_progress(session: &VolumeSession) {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("out.txt");
    let expected = ground_truth_bytes();

    let mut events = Vec::new();
    let summary = session
        .extract_file(KNOWN_FILE, &dst, ExtractOptions::default(), |event| {
            events.push(event);
        })
        .expect("extraction of a known file should succeed");

    assert_eq!(summary.bytes_written, KNOWN_FILE_SIZE);
    assert_eq!(std::fs::read(&dst).expect("read extracted file"), expected);

    assert!(
        matches!(
            events.first(),
            Some(ProgressEvent::Started {
                total_bytes: Some(n),
                ..
            }) if *n == KNOWN_FILE_SIZE
        ),
        "expected a leading Started event with the known total, got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ProgressEvent::Advanced { .. })),
        "expected at least one Advanced event, got {events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(ProgressEvent::Finished { bytes_written }) if *bytes_written == KNOWN_FILE_SIZE
        ),
        "expected a trailing Finished event with the final count, got {events:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn fat_extracts_known_file_with_progress() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_FAT_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);
    assert_extracts_known_file_with_progress(&session);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc"]
fn exfat_extracts_known_file_with_progress() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);
    assert_extracts_known_file_with_progress(&session);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc"]
fn ntfs_extracts_known_file_with_progress() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);
    assert_extracts_known_file_with_progress(&session);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc"]
fn cancellation_mid_extraction_leaves_no_destination_file() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("cancelled.jpg");
    // The largest known file, so cancellation after the first Advanced
    // event still has more data left to copy.
    let source = "/Sydney Sweeney at the 2025 Toronto International Film Festival.jpg";

    let token = CancellationToken::new();
    let cancel_after_first_advance = token.clone();
    let mut events = Vec::new();

    let options = ExtractOptions {
        overwrite: false,
        parents: false,
        cancellation_token: Some(token),
    };

    let result = session.extract_file(source, &dst, options, |event| {
        if matches!(event, ProgressEvent::Advanced { .. }) {
            cancel_after_first_advance.cancel();
        }
        events.push(event);
    });

    assert!(
        matches!(result, Err(AppError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProgressEvent::Finished { .. })),
        "no Finished event should fire on cancellation, got {events:?}"
    );
    assert!(
        !dst.exists(),
        "destination must not exist after a mid-extraction cancellation"
    );
    assert!(
        !dir.path()
            .read_dir()
            .expect("read temp dir")
            .any(|entry| entry.expect("dir entry").path() == dst),
        "no leftover destination-named file should remain in the destination directory"
    );
}
