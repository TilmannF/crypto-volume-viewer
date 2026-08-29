#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "fixture integration tests use direct assertions"
)]

use cryptovol_core::FileBlockReader;
use cryptovol_fs_ntfs::{NtfsError, NtfsFileSystem};
use cryptovol_tcvc::{
    open_with_options, probe_filesystem, FilesystemProbeCandidate, TcvcDataReader, TcvcKdf,
    TcvcOpenOptions,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn fixture_path() -> Option<PathBuf> {
    std::env::var("CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE")
        .ok()
        .map(PathBuf::from)
}

fn ground_truth_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .expect("workspace root should be two levels above crate")
        .join("testdata/static/fs-fat-lfn-original")
}

fn open_options_with_test_password() -> TcvcOpenOptions {
    TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    }
}

fn sha256_file(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).expect("ground truth file should exist");
    Sha256::digest(&bytes).to_vec()
}

fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

fn with_ntfs<T>(fixture: &Path, run: impl FnOnce(&NtfsFileSystem<&TcvcDataReader<'_>>) -> T) -> T {
    let reader = FileBlockReader::open(fixture).expect("fixture should open");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened
        .data_reader(&reader)
        .expect("data reader should open");
    let fs = NtfsFileSystem::open(&data_reader).expect("NTFS should open");
    run(&fs)
}

fn root_expected_names() -> [&'static str; 6] {
    [
        "Emoji Rocket \u{1F680} Test.txt",
        "Folder With Spaces",
        "Please Do Not Open \u{1F605}.txt",
        "Project Notes Final.txt",
        "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
        "Unicode Umlaut \u{00E4}\u{00F6}\u{00FC} \u{00C4}\u{00D6}\u{00DC} \u{00DF}.txt",
    ]
}

fn unicode_umlaut_paths() -> [&'static str; 2] {
    [
        "/Unicode Umlaut \u{00E4}\u{00F6}\u{00FC} \u{00C4}\u{00D6}\u{00DC} \u{00DF}.txt",
        "/Unicode Umlaut a\u{0308}o\u{0308}u\u{0308} A\u{0308}O\u{0308}U\u{0308} \u{00DF}.txt",
    ]
}

fn unicode_umlaut_ground_truth_paths() -> [PathBuf; 2] {
    [
        ground_truth_path()
            .join("Unicode Umlaut \u{00E4}\u{00F6}\u{00FC} \u{00C4}\u{00D6}\u{00DC} \u{00DF}.txt"),
        ground_truth_path().join(
            "Unicode Umlaut a\u{0308}o\u{0308}u\u{0308} A\u{0308}O\u{0308}U\u{0308} \u{00DF}.txt",
        ),
    ]
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_tcvc_opens() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("fixture should open");
    let options = open_options_with_test_password();

    let result = open_with_options(&reader, &options);

    assert!(
        result.is_ok(),
        "TC/VC should open with test-password: {result:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_probe_fs_detects_ntfs() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("fixture should open");
    let options = open_options_with_test_password();
    let opened = open_with_options(&reader, &options).expect("TC/VC should open");
    let data_reader = opened
        .data_reader(&reader)
        .expect("data reader should open");

    let candidate = probe_filesystem(&data_reader).expect("probe should succeed");

    assert_eq!(candidate, FilesystemProbeCandidate::Ntfs);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_root_listing() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let entries = fs.list_dir("/").expect("root listing should succeed");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();

        for expected in root_expected_names() {
            assert!(
                names.contains(&expected)
                    || (expected.starts_with("Unicode Umlaut")
                        && unicode_umlaut_paths()
                            .iter()
                            .map(|path| path.trim_start_matches('/'))
                            .any(|candidate| names.contains(&candidate))),
                "root listing must contain '{expected}'; got: {names:?}"
            );
        }
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_nested_listing() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let entries = fs
            .list_dir("/Folder With Spaces")
            .expect("nested listing should succeed");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();

        assert!(
            names.contains(&"Rocket Science \u{1F680} For Beginners.txt"),
            "nested listing must contain expected file; got: {names:?}"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_emoji_rocket() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let extracted = fs
            .read_file("/Emoji Rocket \u{1F680} Test.txt")
            .expect("emoji file should read");
        let expected = sha256_file(&ground_truth_path().join("Emoji Rocket \u{1F680} Test.txt"));

        assert_eq!(sha256_bytes(&extracted), expected);

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer("/Emoji Rocket \u{1F680} Test.txt", &mut streamed)
            .expect("streaming should succeed for emoji file");
        assert_eq!(
            n as usize,
            streamed.len(),
            "emoji file streaming byte count mismatch"
        );
        assert_eq!(
            streamed, extracted,
            "emoji file streaming output must match read_file"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_project_notes() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let extracted = fs
            .read_file("/Project Notes Final.txt")
            .expect("project notes should read");
        let expected = sha256_file(&ground_truth_path().join("Project Notes Final.txt"));

        assert_eq!(sha256_bytes(&extracted), expected);

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer("/Project Notes Final.txt", &mut streamed)
            .expect("streaming should succeed for project notes");
        assert_eq!(
            n as usize,
            streamed.len(),
            "project notes streaming byte count mismatch"
        );
        assert_eq!(
            streamed, extracted,
            "project notes streaming output must match read_file"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_jpg() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let name = "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg";
        let extracted = fs.read_file(&format!("/{name}")).expect("JPG should read");
        let expected = sha256_file(&ground_truth_path().join(name));

        assert_eq!(sha256_bytes(&extracted), expected);

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer(&format!("/{name}"), &mut streamed)
            .expect("streaming should succeed for JPG");
        assert_eq!(
            n as usize,
            streamed.len(),
            "JPG streaming byte count mismatch"
        );
        assert_eq!(
            streamed, extracted,
            "JPG streaming output must match read_file"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_unicode_umlaut() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let (extracted, working_path) = unicode_umlaut_paths()
            .iter()
            .find_map(|p| fs.read_file(p).ok().map(|data| (data, *p)))
            .expect("one Unicode umlaut path variant should read");
        let expected = unicode_umlaut_ground_truth_paths()
            .iter()
            .find(|path| path.exists())
            .map(|path| sha256_file(path))
            .expect("one Unicode umlaut ground truth variant should exist");

        assert_eq!(sha256_bytes(&extracted), expected);

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer(working_path, &mut streamed)
            .expect("streaming should succeed for working umlaut path");
        assert_eq!(
            n as usize,
            streamed.len(),
            "umlaut streaming byte count mismatch"
        );
        assert_eq!(
            streamed, extracted,
            "umlaut streaming output must match read_file"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_extract_nested() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let name = "Rocket Science \u{1F680} For Beginners.txt";
        let nested_path = format!("/Folder With Spaces/{name}");
        let extracted = fs.read_file(&nested_path).expect("nested file should read");
        let expected = sha256_file(&ground_truth_path().join("Folder With Spaces").join(name));

        assert_eq!(sha256_bytes(&extracted), expected);

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer(&nested_path, &mut streamed)
            .expect("streaming should succeed for nested file");
        assert_eq!(
            n as usize,
            streamed.len(),
            "nested file streaming byte count mismatch"
        );
        assert_eq!(
            streamed, extracted,
            "nested file streaming output must match read_file"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_wrong_password_fails() {
    let Some(path) = fixture_path() else {
        return;
    };
    let reader = FileBlockReader::open(&path).expect("fixture should open");
    let options = TcvcOpenOptions {
        password: b"wrong-password".to_vec(),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    };

    let result = open_with_options(&reader, &options);

    assert!(result.is_err(), "wrong password must fail to open");
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_path_not_found() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let result = fs.stat("/this-does-not-exist.txt");

        assert!(
            matches!(result, Err(NtfsError::PathNotFound { .. })),
            "unknown path must return PathNotFound, got: {result:?}"
        );
        let error_text = format!("{:?}", result.expect_err("path should be missing"));
        assert!(
            !error_text.contains("test-password"),
            "error must not expose password: {error_text}"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_directory_extract_fails() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let result = fs.read_file("/Folder With Spaces");

        assert!(
            matches!(result, Err(NtfsError::AttemptedDirectoryExtraction { .. })),
            "reading a directory must return AttemptedDirectoryExtraction, got: {result:?}"
        );
    });
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to the .hc fixture"]
fn fixture_unicode_emoji_path_extract_succeeds() {
    let Some(path) = fixture_path() else {
        return;
    };
    with_ntfs(&path, |fs| {
        let result = fs.read_file("/Emoji Rocket \u{1F680} Test.txt");

        assert!(
            result.is_ok(),
            "emoji path extraction should succeed: {result:?}"
        );
        let extracted = result.expect("emoji path should succeed");

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer("/Emoji Rocket \u{1F680} Test.txt", &mut streamed)
            .expect("streaming should succeed for emoji path");
        assert_eq!(
            n as usize,
            streamed.len(),
            "emoji path streaming byte count mismatch"
        );
        assert_eq!(
            streamed, extracted,
            "emoji path streaming output must match read_file"
        );
    });
}
