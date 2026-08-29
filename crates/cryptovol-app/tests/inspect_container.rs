#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "inspect_container tests build synthetic temp files with direct assertions"
)]

//! Verifies that `inspect_container` reports safe, non-secret container
//! metadata without requiring a password and without attempting decryption.

use cryptovol_app::{inspect_container, ContainerBackupHeaderCandidate, ContainerHeaderInspection};
use std::io::Write as _;

fn write_temp_file(dir: &tempfile::TempDir, name: &str, size: usize) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).expect("create temp file");
    let bytes = vec![0xAB_u8; size];
    file.write_all(&bytes).expect("write temp file contents");
    path
}

#[test]
fn reports_container_path_and_size_without_a_password() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_temp_file(&dir, "container.bin", 2048);

    // inspect_container takes only a path: no password parameter exists.
    let info = inspect_container(&path).expect("inspect_container should succeed");

    assert_eq!(info.container_path, path);
    assert_eq!(info.container_size_bytes, 2048);
}

#[test]
fn too_small_file_reports_too_small_state() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_temp_file(&dir, "tiny.bin", 256);

    let info = inspect_container(&path).expect("inspect_container should succeed");

    match info.header_inspection {
        ContainerHeaderInspection::TooSmall { required_minimum } => {
            assert_eq!(required_minimum, 512);
        }
        other => panic!("expected TooSmall, got {other:?}"),
    }
}

#[test]
fn exact_minimum_size_reports_backup_overlapping_primary() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_temp_file(&dir, "exact.bin", 512);

    let info = inspect_container(&path).expect("inspect_container should succeed");

    match info.header_inspection {
        ContainerHeaderInspection::Candidates { primary, backup } => {
            assert_eq!(primary.offset, 0);
            assert_eq!(primary.length, 512);
            assert!(primary.readable);
            assert!(matches!(
                backup,
                ContainerBackupHeaderCandidate::OverlapsPrimary
            ));
        }
        other => panic!("expected Candidates, got {other:?}"),
    }
}

#[test]
fn larger_file_reports_distinct_primary_and_backup_candidates() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_temp_file(&dir, "normal.bin", 2048);

    let info = inspect_container(&path).expect("inspect_container should succeed");

    match info.header_inspection {
        ContainerHeaderInspection::Candidates { primary, backup } => {
            assert_eq!(primary.offset, 0);
            assert_eq!(primary.length, 512);
            assert!(primary.readable);
            match backup {
                ContainerBackupHeaderCandidate::Candidate(candidate) => {
                    assert_eq!(candidate.offset, 2048 - 512);
                    assert_eq!(candidate.length, 512);
                    assert!(candidate.readable);
                }
                ContainerBackupHeaderCandidate::OverlapsPrimary => {
                    panic!("expected a distinct backup candidate for a 2048-byte file")
                }
            }
        }
        other => panic!("expected Candidates, got {other:?}"),
    }
}
