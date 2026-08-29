#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "fixture integration tests use direct assertions"
)]

use cryptovol_core::FileBlockReader;
use cryptovol_fs_exfat::{ExfatError, ExfatFileSystem};
use cryptovol_tcvc::{
    open_with_options, probe_filesystem, FilesystemProbeCandidate, TcvcKdf, TcvcOpenOptions,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn fixture_path() -> Option<PathBuf> {
    std::env::var("CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE")
        .ok()
        .map(PathBuf::from)
}

fn ground_truth_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .unwrap()
        .join("testdata/static/fs-fat-lfn-original")
}

fn open_options_with_test_password() -> TcvcOpenOptions {
    TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    }
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_tcvc_opens_with_correct_password() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let result = open_with_options(&reader, &options);
    assert!(
        result.is_ok(),
        "TC/VC should open with test-password: {result:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_probe_fs_detects_exfat() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let candidate = probe_filesystem(&data_reader).expect("probe should succeed");
    assert_eq!(
        candidate,
        FilesystemProbeCandidate::ExFat,
        "fixture must be detected as exFAT"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_root_listing_contains_all_expected_files() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let entries = fs.list_dir("/").expect("root listing should succeed");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    let expected = [
        "Emoji Rocket \u{1F680} Test.txt",
        "Folder With Spaces",
        "Please Do Not Open \u{1F605}.txt",
        "Project Notes Final.txt",
        "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
        "Unicode Umlaut \u{00E4}\u{00F6}\u{00FC} \u{00C4}\u{00D6}\u{00DC} \u{00DF}.txt",
    ];
    for expected_name in &expected {
        assert!(
            names.contains(expected_name),
            "root listing must contain '{expected_name}'; got: {names:?}"
        );
    }
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_nested_listing() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let entries = fs
        .list_dir("/Folder With Spaces")
        .expect("nested listing should succeed");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    assert!(
        names.contains(&"Rocket Science \u{1F680} For Beginners.txt"),
        "nested listing must contain expected file; got: {names:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_emoji_file_matches_ground_truth() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let extracted = fs
        .read_file("/Emoji Rocket \u{1F680} Test.txt")
        .expect("read_file should succeed");
    let expected = fs::read(ground_truth_path().join("Emoji Rocket \u{1F680} Test.txt"))
        .expect("ground truth file should exist");

    assert_eq!(
        extracted, expected,
        "extracted bytes must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer("/Emoji Rocket \u{1F680} Test.txt", &mut streamed)
        .expect("read_file_to_writer should succeed for emoji file");
    assert_eq!(
        n as usize,
        streamed.len(),
        "emoji file streaming byte count mismatch"
    );
    assert_eq!(
        streamed, extracted,
        "emoji file streaming output must match read_file"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_project_notes_matches_ground_truth() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let extracted = fs
        .read_file("/Project Notes Final.txt")
        .expect("read_file should succeed");
    let expected =
        fs::read(ground_truth_path().join("Project Notes Final.txt")).expect("ground truth exists");

    assert_eq!(
        extracted, expected,
        "extracted bytes must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer("/Project Notes Final.txt", &mut streamed)
        .expect("read_file_to_writer should succeed for project notes");
    assert_eq!(
        n as usize,
        streamed.len(),
        "project notes streaming byte count mismatch"
    );
    assert_eq!(
        streamed, extracted,
        "project notes streaming output must match read_file"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_jpg_matches_ground_truth() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let name = "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg";
    let extracted = fs
        .read_file(&format!("/{name}"))
        .expect("read_file should succeed");
    let ground_truth = fs::read(ground_truth_path().join(name)).expect("ground truth exists");

    let extracted_hash = Sha256::digest(&extracted);
    let expected_hash = Sha256::digest(&ground_truth);
    assert_eq!(
        extracted_hash, expected_hash,
        "SHA-256 of extracted JPG must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&format!("/{name}"), &mut streamed)
        .expect("read_file_to_writer should succeed for JPG");
    assert_eq!(
        n as usize,
        streamed.len(),
        "JPG streaming byte count mismatch"
    );
    assert_eq!(
        streamed, extracted,
        "JPG streaming output must match read_file"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_unicode_umlaut_matches_ground_truth() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let name = "Unicode Umlaut \u{00E4}\u{00F6}\u{00FC} \u{00C4}\u{00D6}\u{00DC} \u{00DF}.txt";
    let extracted = fs
        .read_file(&format!("/{name}"))
        .expect("read_file should succeed");
    let ground_truth = fs::read(ground_truth_path().join(name)).expect("ground truth exists");

    assert_eq!(
        extracted, ground_truth,
        "extracted bytes must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&format!("/{name}"), &mut streamed)
        .expect("read_file_to_writer should succeed for umlaut file");
    assert_eq!(
        n as usize,
        streamed.len(),
        "umlaut file streaming byte count mismatch"
    );
    assert_eq!(
        streamed, extracted,
        "umlaut file streaming output must match read_file"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_nested_file_matches_ground_truth() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let name = "Rocket Science \u{1F680} For Beginners.txt";
    let nested_path = format!("/Folder With Spaces/{name}");
    let extracted = fs
        .read_file(&nested_path)
        .expect("read_file should succeed");
    let ground_truth = fs::read(ground_truth_path().join("Folder With Spaces").join(name))
        .expect("ground truth exists");

    assert_eq!(
        extracted, ground_truth,
        "extracted bytes must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&nested_path, &mut streamed)
        .expect("read_file_to_writer should succeed for nested file");
    assert_eq!(
        n as usize,
        streamed.len(),
        "nested file streaming byte count mismatch"
    );
    assert_eq!(
        streamed, extracted,
        "nested file streaming output must match read_file"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_wrong_password_fails() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    // Use kdf_hint to avoid exhausting all KDF profiles (slow); fixture is SHA-512.
    let options = TcvcOpenOptions {
        password: b"wrong-password".to_vec(),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    };
    let result = open_with_options(&reader, &options);
    assert!(result.is_err(), "wrong password must fail to open");
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_unknown_path_fails() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let result = fs.stat("/this-does-not-exist.txt");
    assert!(
        matches!(result, Err(ExfatError::PathNotFound { .. })),
        "unknown path must return PathNotFound, got: {result:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_directory_fails() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("open fixture");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = ExfatFileSystem::open(&data_reader).expect("exFAT should open");

    let result = fs.read_file("/Folder With Spaces");
    assert!(
        matches!(result, Err(ExfatError::AttemptedDirectoryExtraction { .. })),
        "reading a directory must return AttemptedDirectoryExtraction, got: {result:?}"
    );
}
