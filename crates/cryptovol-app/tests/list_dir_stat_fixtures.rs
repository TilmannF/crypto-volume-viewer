#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "fixture tests assert against documented ground truth in docs/test-containers.md"
)]

//! Env-gated integration tests verifying `VolumeSession::list_dir` and
//! `VolumeSession::stat` dispatch correctly for FAT, exFAT, and NTFS,
//! including FAT's `stat` fallback (`FatFileSystem` has no native stat).
//!
//! These tests are `#[ignore]`d so `cargo test --workspace --all-targets`
//! passes without any static fixtures present, matching the existing
//! fixture-test convention in `crates/cryptovol-cli/tests/fixture_workflow.rs`.
//! Run with the documented `CRYPTOVOL_STATIC_*_LFN_FIXTURE` env vars and
//! `-- --ignored` to exercise them for real; see `docs/test-containers.md`.

use cryptovol_app::{open_volume, AppError, FileEntry, FilesystemKind, OpenVolumeRequest};
use cryptovol_tcvc::TcvcKdf;
use secrecy::SecretString;
use std::path::PathBuf;

const TEST_PASSWORD: &str = "test-password";

fn fixture_path(env_var: &str) -> Option<PathBuf> {
    let val = std::env::var(env_var).ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn open_fixture(path: &std::path::Path) -> cryptovol_app::VolumeSession {
    open_volume(OpenVolumeRequest {
        container_path: path.to_path_buf(),
        password: SecretString::from(TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    })
    .expect("fixture should open with the documented test password")
}

/// (name, size) entries common to the FAT/exFAT/NTFS LFN fixtures' root
/// directory, per `docs/test-containers.md`.
const KNOWN_ROOT_FILES: &[(&str, u64)] = &[
    ("Emoji Rocket \u{1F680} Test.txt", 22),
    ("Please Do Not Open \u{1F605}.txt", 23),
    ("Project Notes Final.txt", 49),
    (
        "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
        89489,
    ),
];

fn assert_known_root_entries(entries: &[FileEntry], filesystem: FilesystemKind) {
    for (name, size) in KNOWN_ROOT_FILES {
        let entry = entries
            .iter()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("root listing missing {name:?}: {entries:?}"));
        assert_eq!(entry.size, *size, "size mismatch for {name:?}");
        assert!(!entry.is_dir, "{name:?} should be a file");
        assert_eq!(entry.filesystem, filesystem);
    }

    let folder = entries
        .iter()
        .find(|e| e.name == "Folder With Spaces")
        .unwrap_or_else(|| panic!("root listing missing Folder With Spaces: {entries:?}"));
    assert!(
        folder.is_dir,
        "Folder With Spaces should be a directory: {folder:?}"
    );
}

fn assert_stat_matches_listing(session: &cryptovol_app::VolumeSession, entries: &[FileEntry]) {
    let stat_entry = session
        .stat("/Project Notes Final.txt")
        .expect("stat should succeed for a known file");
    let listed_entry = entries
        .iter()
        .find(|e| e.name == "Project Notes Final.txt")
        .expect("listed entry should exist");
    assert_eq!(stat_entry.name, listed_entry.name);
    assert_eq!(stat_entry.size, listed_entry.size);
    assert_eq!(stat_entry.is_dir, listed_entry.is_dir);
    assert_eq!(
        stat_entry.path.as_deref(),
        Some("/Project Notes Final.txt"),
        "stat must fill in the queried path, per FileEntry::path's documented contract \
         (GUI extraction depends on this to build ExtractFileRequestDto.source_path)"
    );

    let missing = session.stat("/does-not-exist.txt");
    assert!(
        matches!(missing, Err(AppError::PathNotFound(_))),
        "expected PathNotFound, got {missing:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn fat_list_dir_and_stat_match_documented_fixture() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_FAT_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);

    let entries = session.list_dir("/").expect("root listing should succeed");
    assert_known_root_entries(&entries, FilesystemKind::Fat);

    // FAT has no native stat(); this exercises the parent-listing fallback.
    assert_stat_matches_listing(&session, &entries);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc"]
fn exfat_list_dir_and_stat_match_documented_fixture() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);

    let entries = session.list_dir("/").expect("root listing should succeed");
    assert_known_root_entries(&entries, FilesystemKind::ExFat);

    // exFAT has a native stat(); this exercises that path.
    assert_stat_matches_listing(&session, &entries);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc"]
fn ntfs_list_dir_and_stat_match_documented_fixture() {
    let Some(path) = fixture_path("CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE") else {
        eprintln!("skipped: CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE not set or absent");
        return;
    };
    let session = open_fixture(&path);

    let entries = session.list_dir("/").expect("root listing should succeed");
    assert_known_root_entries(&entries, FilesystemKind::Ntfs);

    // NTFS has a native stat(); this exercises that path.
    assert_stat_matches_listing(&session, &entries);
}
